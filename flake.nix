{
  description = "Profile-aware Google Workspace CLI for humans and agents";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      gwsVersion = "0.22.5";

      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        gw = pkgs.callPackage ./nix/gw.nix { inherit gwsVersion; };
        default = gw;
      });

      checks = forAllSystems (
        pkgs:
        let
          gw = self.packages.${pkgs.system}.gw;
        in
        {
          tests = pkgs.callPackage ./nix/gw.nix {
            inherit gwsVersion;
            bindGws = false;
          };

          gws-backend = pkgs.runCommand "gw-gws-backend" { } ''
            expected="gws ${pkgs.lib.getExe pkgs.gws}"
            if ! ${pkgs.lib.getExe gw} --version | grep -qxF "$expected"; then
              echo "gw does not resolve the pinned backend: $expected" >&2
              ${pkgs.lib.getExe gw} --version >&2
              exit 1
            fi
            touch $out
          '';
        }
      );

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = [
            pkgs.cargo
            pkgs.rustc
            pkgs.clippy
            pkgs.rustfmt
            pkgs.rust-analyzer
            pkgs.gws
          ];
        };
      });

      formatter = forAllSystems (
        pkgs:
        pkgs.writeShellApplication {
          name = "gw-fmt";
          runtimeInputs = [
            pkgs.cargo
            pkgs.findutils
            pkgs.nixfmt
            pkgs.rustfmt
          ];
          text = ''
            find . -name target -prune -o -name '*.nix' -print0 | xargs -0 -r nixfmt
            cargo fmt
          '';
        }
      );
    };
}
