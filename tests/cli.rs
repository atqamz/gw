use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

const FAKE_GWS: &str = r#"#!/bin/sh
printf 'config_dir=%s\n' "${GOOGLE_WORKSPACE_CLI_CONFIG_DIR-unset}"
printf 'credentials_file=%s\n' "${GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE-unset}"
printf 'token=%s\n' "${GOOGLE_WORKSPACE_CLI_TOKEN-unset}"
printf 'argc=%s\n' "$#"
for arg in "$@"; do
  printf 'arg=%s\n' "$arg"
done
printf 'fake gws diagnostics\n' >&2
if [ "$1" = "stream" ]; then
  printf 'chunk-1\nchunk-2\nchunk-3\n'
  exit 0
fi
if [ "$1" = "cat" ]; then
  while IFS= read -r line; do printf '%s\n' "$line"; done
  exit 0
fi
exit 42
"#;

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = format!(
            "gw-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let root = std::env::temp_dir().join(unique);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("bin")).unwrap();
        fs::create_dir_all(root.join("config")).unwrap();

        let fake = root.join("bin").join("gws");
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&fake)
            .unwrap();
        file.write_all(FAKE_GWS.as_bytes()).unwrap();
        drop(file);

        Sandbox { root }
    }

    fn config_home(&self) -> PathBuf {
        self.root.join("config")
    }

    fn gw_root(&self) -> PathBuf {
        self.config_home().join("gw")
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_gw"));
        command
            .env_clear()
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.config_home())
            .env("PATH", self.root.join("bin"));
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn add(&self, name: &str, account: &str) -> Output {
        self.run(&["profile", "add", name, "--account", account])
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn missing_profile_fails_closed() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&[]);
    assert!(!output.status.success());
    assert_eq!(
        stderr(&output),
        "error: profile required\n\nusage: gw <profile> <gws arguments...>\n"
    );
    assert_eq!(stdout(&output), "");
}

#[test]
fn unknown_profile_fails_closed() {
    let sandbox = Sandbox::new();
    sandbox.add("personal", "user@example.com");
    let output = sandbox.run(&["work", "gmail", "list"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("unknown profile"));
    assert_eq!(stdout(&output), "");
}

#[test]
fn traversal_profile_names_fail() {
    let sandbox = Sandbox::new();
    for name in [
        "..",
        ".",
        "../../etc",
        "/etc/passwd",
        "personal/../work",
        "personal\\work",
        ".hidden",
    ] {
        let output = sandbox.run(&[name, "gmail", "list"]);
        assert!(!output.status.success(), "{name:?} should be rejected");
        assert!(
            stderr(&output).contains("invalid profile name"),
            "{name:?} produced: {}",
            stderr(&output)
        );
    }
}

#[test]
fn reserved_management_name_is_rejected() {
    let sandbox = Sandbox::new();
    let output = sandbox.add("profile", "user@example.com");
    assert!(!output.status.success());
    assert!(stderr(&output).contains("reserved"));
    assert!(!sandbox.gw_root().join("profiles").join("profile").exists());
}

#[test]
fn add_creates_private_layout() {
    let sandbox = Sandbox::new();
    let output = sandbox.add("personal", "user@example.com");
    assert!(output.status.success(), "{}", stderr(&output));

    let dir = sandbox.gw_root().join("profiles").join("personal");
    assert_eq!(mode(&sandbox.gw_root()), 0o700);
    assert_eq!(mode(&sandbox.gw_root().join("profiles")), 0o700);
    assert_eq!(mode(&dir), 0o700);
    assert_eq!(mode(&dir.join("gws")), 0o700);
    assert_eq!(mode(&dir.join("account")), 0o600);
    assert_eq!(
        fs::read_to_string(dir.join("account")).unwrap(),
        "user@example.com\n"
    );

    let again = sandbox.add("personal", "user@example.com");
    assert!(!again.status.success());
    assert!(stderr(&again).contains("already exists"));
}

#[test]
fn add_rejects_unsafe_input() {
    let sandbox = Sandbox::new();
    assert!(!sandbox
        .add("../escape", "user@example.com")
        .status
        .success());
    assert!(!sandbox.add("personal", "not-an-email").status.success());
    assert!(!sandbox
        .run(&["profile", "add", "personal"])
        .status
        .success());
}

#[test]
fn list_and_show_expose_only_metadata() {
    let sandbox = Sandbox::new();
    sandbox.add("work", "work@example.com");
    sandbox.add("personal", "user@example.com");

    let secret = sandbox
        .gw_root()
        .join("profiles")
        .join("personal")
        .join("gws")
        .join("credentials.json");
    fs::write(&secret, "{\"refresh_token\":\"super-secret-token\"}").unwrap();

    let plain = sandbox.run(&["profile", "list"]);
    assert!(plain.status.success(), "{}", stderr(&plain));
    assert!(stdout(&plain).starts_with("personal"));
    assert!(stdout(&plain).contains("work@example.com"));

    let listed = sandbox.run(&["profile", "list", "--json"]);
    assert!(listed.status.success(), "{}", stderr(&listed));
    let body = stdout(&listed);
    assert!(
        body.starts_with("{\"profiles\":[{\"name\":\"personal\""),
        "{body}"
    );
    assert!(body.contains("\"account\":\"user@example.com\""));
    assert!(body.contains("\"gws_config_dir\":"));
    for forbidden in [
        "super-secret-token",
        "refresh_token",
        "credentials",
        "client_secret",
        "keyring",
    ] {
        assert!(!body.contains(forbidden), "{forbidden} leaked into {body}");
    }

    let shown = sandbox.run(&["profile", "show", "personal", "--json"]);
    assert!(shown.status.success());
    assert_eq!(
        stdout(&shown).trim(),
        format!(
            "{{\"name\":\"personal\",\"account\":\"user@example.com\",\"gws_config_dir\":\"{}\"}}",
            sandbox
                .gw_root()
                .join("profiles")
                .join("personal")
                .join("gws")
                .display()
        )
    );

    assert!(!sandbox
        .run(&["profile", "show", "missing"])
        .status
        .success());
}

#[test]
fn json_output_escapes_hostile_metadata() {
    let sandbox = Sandbox::new();
    sandbox.add("personal", "user@example.com");
    let account = sandbox
        .gw_root()
        .join("profiles")
        .join("personal")
        .join("account");
    fs::write(&account, "a\"b\\c\u{1b}[0m@example.com\nsecond line\n").unwrap();

    let output = sandbox.run(&["profile", "show", "personal", "--json"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let body = stdout(&output);
    assert!(
        body.contains("\"account\":\"a\\\"b\\\\c[0m@example.com\""),
        "{body}"
    );
    assert!(!body.contains("second line"));
    assert!(!body.contains('\u{1b}'));
}

#[test]
fn valid_profile_reaches_child_with_isolated_config_dir() {
    let sandbox = Sandbox::new();
    sandbox.add("personal", "user@example.com");

    let output = sandbox.run(&["personal", "gmail", "+triage", "--query", "is:unread"]);
    let body = stdout(&output);
    let expected = sandbox
        .gw_root()
        .join("profiles")
        .join("personal")
        .join("gws");

    assert_eq!(output.status.code(), Some(42));
    assert!(
        body.contains(&format!("config_dir={}\n", expected.display())),
        "{body}"
    );
    assert_eq!(
        body,
        format!(
            "config_dir={}\ncredentials_file=unset\ntoken=unset\nargc=4\narg=gmail\narg=+triage\narg=--query\narg=is:unread\n",
            expected.display()
        )
    );
    assert_eq!(stderr(&output), "fake gws diagnostics\n");
}

#[test]
fn two_profiles_resolve_different_config_dirs() {
    let sandbox = Sandbox::new();
    sandbox.add("personal", "user@example.com");
    sandbox.add("work", "work@example.com");

    let personal = stdout(&sandbox.run(&["personal", "whoami"]));
    let work = stdout(&sandbox.run(&["work", "whoami"]));

    assert!(personal.contains("profiles/personal/gws"));
    assert!(work.contains("profiles/work/gws"));
    assert_ne!(personal, work);
}

#[test]
fn identity_bearing_environment_is_stripped_from_child() {
    let sandbox = Sandbox::new();
    sandbox.add("personal", "user@example.com");

    let output = sandbox
        .command()
        .env("GOOGLE_WORKSPACE_CLI_CONFIG_DIR", "/somewhere/else")
        .env("GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE", "/other/creds.json")
        .env("GOOGLE_WORKSPACE_CLI_TOKEN", "ambient-token")
        .args(["personal", "gmail", "list"])
        .output()
        .unwrap();

    let body = stdout(&output);
    assert!(body.contains("credentials_file=unset\n"), "{body}");
    assert!(body.contains("token=unset\n"), "{body}");
    assert!(!body.contains("/somewhere/else"), "{body}");
    assert!(!body.contains("ambient-token"), "{body}");
}

#[test]
fn arguments_are_preserved_verbatim() {
    let sandbox = Sandbox::new();
    sandbox.add("personal", "user@example.com");

    let args = [
        "--help",
        "-x",
        "",
        "a b",
        "quote\"d",
        "--flag=value with spaces",
        "+triage",
    ];
    let mut command = sandbox.command();
    command.arg("personal").args(args);
    let output = command.output().unwrap();
    let body = stdout(&output);

    assert!(body.contains(&format!("argc={}\n", args.len())), "{body}");
    for arg in args {
        assert!(body.contains(&format!("arg={arg}\n")), "{arg:?} in {body}");
    }
}

#[test]
fn authentication_commands_pass_through() {
    let sandbox = Sandbox::new();
    sandbox.add("personal", "user@example.com");
    let body = stdout(&sandbox.run(&["personal", "auth", "login", "--no-browser"]));
    assert!(
        body.contains("arg=auth\narg=login\narg=--no-browser\n"),
        "{body}"
    );
}

#[test]
fn stdin_and_streaming_output_pass_through() {
    let sandbox = Sandbox::new();
    sandbox.add("personal", "user@example.com");

    let streamed = sandbox.run(&["personal", "stream"]);
    assert_eq!(streamed.status.code(), Some(0));
    assert!(stdout(&streamed).ends_with("chunk-1\nchunk-2\nchunk-3\n"));

    let mut child = sandbox
        .command()
        .args(["personal", "cat"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"piped payload\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).ends_with("piped payload\n"));
}

#[test]
fn help_and_version_do_not_require_a_profile() {
    let sandbox = Sandbox::new();
    let help = sandbox.run(&["--help"]);
    assert!(help.status.success());
    assert!(stdout(&help).contains("gw <profile> <gws arguments...>"));
    assert!(stdout(&help).contains("There is no default profile"));

    let version = sandbox.run(&["--version"]);
    assert!(version.status.success());
    assert!(stdout(&version).starts_with(&format!("gw {}\n", env!("CARGO_PKG_VERSION"))));
    assert!(stdout(&version).contains("gws "));
}

#[test]
fn empty_profile_list_is_quiet() {
    let sandbox = Sandbox::new();
    let plain = sandbox.run(&["profile", "list"]);
    assert!(plain.status.success());
    assert_eq!(stdout(&plain), "");

    let json = sandbox.run(&["profile", "list", "--json"]);
    assert!(json.status.success());
    assert_eq!(stdout(&json), "{\"profiles\":[]}\n");
}
