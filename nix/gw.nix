{
  lib,
  rustPlatform,
  gws,
  gwsVersion,
  bindGws ? true,
}:

let
  pinnedGws =
    assert lib.assertMsg (gws.version == gwsVersion)
      "gw pins gws ${gwsVersion} but nixpkgs provides gws ${gws.version}; review upstream changes and update gwsVersion in flake.nix";
    gws;

  manifest = (lib.importTOML ../Cargo.toml).package;
in
rustPlatform.buildRustPackage {
  pname = manifest.name;
  inherit (manifest) version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../tests
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  env = lib.optionalAttrs bindGws { GW_GWS_BIN = lib.getExe pinnedGws; };

  doCheck = !bindGws;

  meta = {
    inherit (manifest) description;
    homepage = manifest.repository;
    license = lib.licenses.mit;
    mainProgram = "gw";
    platforms = lib.platforms.unix;
  };
}
