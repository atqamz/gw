# gw

Profile-aware Google Workspace CLI for humans and agents.

`gw` is a small facade around [`googleworkspace/cli`](https://github.com/googleworkspace/cli) (`gws`).
It adds named profiles, requires an explicit profile for every operation, and gives each profile a fully isolated `gws` configuration directory.

`gws` owns Google. `gw` owns identity selection and isolation.

## Why

`gws` has broad, Discovery-generated Google Workspace coverage, but no reliable first-class multi-account support.
Native multi-account support was removed upstream ([googleworkspace/cli#439](https://github.com/googleworkspace/cli/issues/439)), and pointing `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE` at a different file is not sufficient isolation because cached authentication state can still belong to another account ([googleworkspace/cli#572](https://github.com/googleworkspace/cli/issues/572)).

That matters most for AI agents. Separate Google accounts are separate trust boundaries, and a write must never silently land in the wrong identity.

`gw` fixes exactly that and nothing else:

- Each profile gets its own `GOOGLE_WORKSPACE_CLI_CONFIG_DIR`, so OAuth client config, encrypted credentials, token cache, and other cached runtime state never mix.
- There is no default profile, no `GW_PROFILE` fallback, and no `--all` mode. A missing profile is an error.
- `gw` replaces itself with `gws` via `execve`, so stdin, stdout, stderr, exit codes, signals, streaming, and binary output are untouched.

`gw` implements no Google API, no OAuth, no HTTP client, no daemon, and no output transformation.

## Install

```bash
nix profile install github:atqamz/gw
```

Or in a flake:

```nix
{
  inputs.gw.url = "github:atqamz/gw";
}
```

The Nix package embeds the absolute store path of a pinned `gws` (currently `0.22.5`), so `gw` never depends on whatever `gws` happens to be on `$PATH`. You do not install or manage `gws` yourself.

```bash
gw --version
# gw 0.1.0
# gws /nix/store/...-gws-0.22.5/bin/gws
```

Non-Nix builds (`cargo build --release`) fall back to `gws` from `$PATH`.

## Usage

```bash
gw profile add personal --account user@example.com
gw personal auth login

gw personal gmail +triage
gw personal calendar +agenda
gw work tasks tasklists list
gw work drive files list
```

Everything after the profile name is passed to `gws` verbatim.

### Profile management

```bash
gw profile add <name> --account <email>
gw profile list
gw profile list --json
gw profile show <name>
gw profile show <name> --json
```

`profile list --json`:

```json
{"profiles":[{"name":"personal","account":"user@example.com","gws_config_dir":"/home/you/.config/gw/profiles/personal/gws"}]}
```

There is no `profile remove` in v0.1. Once a profile owns OAuth credentials, token cache, and encryption state, deletion semantics are security sensitive and deserve a separate design.

### Missing profile

```console
$ gw gmail +triage
error: profile required

usage: gw <profile> <gws arguments...>
```

## Layout

```text
~/.config/gw/
└── profiles/
    ├── personal/
    │   ├── account
    │   └── gws/
    └── work/
        ├── account
        └── gws/
```

The root honours `$XDG_CONFIG_HOME` and otherwise falls back to `$HOME/.config`.

`account` is non-secret metadata. `gws/` is opaque: `gw` never reads, parses, prints, or migrates anything inside it.

## Security model

- Profile names are validated against `[A-Za-z0-9._-]`, must not be empty, must not start with `-` or `.`, and are capped at 64 characters. `.`, `..`, path separators, absolute paths, and control characters are all rejected, and the resolved directory is checked to be a direct child of the profile root.
- Profile directories are created `0700`; the `account` file is `0600`.
- Profile metadata never becomes child environment variables. The only variable `gw` sets is `GOOGLE_WORKSPACE_CLI_CONFIG_DIR`.
- Identity-bearing `gws` variables inherited from the ambient environment are removed before exec, so an ambient credential cannot leak across the profile boundary: `GOOGLE_WORKSPACE_CLI_CONFIG_DIR`, `GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE`, `GOOGLE_WORKSPACE_CLI_TOKEN`, `GOOGLE_WORKSPACE_CLI_CLIENT_ID`, `GOOGLE_WORKSPACE_CLI_CLIENT_SECRET`, `GOOGLE_WORKSPACE_CLI_KEYRING_BACKEND`. Other `gws` variables such as logging and sanitisation settings are passed through.
- Account metadata is sanitised on read and escaped on JSON output, so a tampered `account` file cannot inject terminal escapes or break the JSON.
- `gw` never has credential material in memory and never prints it.

After authenticating, verify the identity with a normal API call rather than by inspecting credential files.

## Agent Skill

`skills/google-workspace/SKILL.md` is a generic, model-agnostic Agent Skill. It contains no personal addresses and no machine-specific profile names.

```bash
bunx skills add atqamz/gw -g -a opencode -a claude-code -a codex --skill '*'
```

Which profile means what for a given user belongs in local agent configuration, not in the shipped skill.

## Development

```bash
nix develop
cargo test
cargo clippy --all-targets -- -D warnings
nix fmt
nix flake check
```

`nix flake check` builds the package, runs the test suite, and asserts that the built `gw` resolves the pinned `gws` store path.

The test suite uses a fake `gws` shell script, so isolation and pass-through are verified without Google credentials.

## Credits

All Google Workspace functionality comes from [`googleworkspace/cli`](https://github.com/googleworkspace/cli), Apache-2.0.

Related work: [`openclaw/gogcli`](https://github.com/openclaw/gogcli) offers native multi-account support and stronger built-in agent safety controls, and may be the better fit if you want broad Google access with multiple accounts today.

## License

MIT
