mod json;
mod profile;

use std::env;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitCode};

use profile::Profile;

const USAGE: &str = "usage: gw <profile> <gws arguments...>";

const HELP: &str = "gw - profile-aware Google Workspace CLI

usage:
  gw <profile> <gws arguments...>
  gw profile add <name> --account <email>
  gw profile list [--json]
  gw profile show <name> [--json]

options:
  -h, --help     print this help
  -V, --version  print the gw version and the gws backend in use

Every Google Workspace operation requires an explicit profile.
There is no default profile and no environment fallback.
Arguments after the profile are passed to gws unchanged.
";

const PROFILE_USAGE: &str = "usage:
  gw profile add <name> --account <email>
  gw profile list [--json]
  gw profile show <name> [--json]";

const ISOLATED_ENV: [&str; 6] = [
    "GOOGLE_WORKSPACE_CLI_CONFIG_DIR",
    "GOOGLE_WORKSPACE_CLI_CREDENTIALS_FILE",
    "GOOGLE_WORKSPACE_CLI_TOKEN",
    "GOOGLE_WORKSPACE_CLI_CLIENT_ID",
    "GOOGLE_WORKSPACE_CLI_CLIENT_SECRET",
    "GOOGLE_WORKSPACE_CLI_KEYRING_BACKEND",
];

fn gws_bin() -> &'static str {
    option_env!("GW_GWS_BIN").unwrap_or("gws")
}

fn fail(message: &str) -> ! {
    eprintln!("error: {message}");
    std::process::exit(1)
}

fn fail_with(message: &str, usage: &str) -> ! {
    eprintln!("error: {message}\n\n{usage}");
    std::process::exit(1)
}

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(first) = args.next() else {
        fail_with("profile required", USAGE)
    };
    let Some(first) = first.to_str() else {
        fail("invalid profile name: not valid UTF-8")
    };

    match first {
        "-h" | "--help" => print!("{HELP}"),
        "-V" | "--version" => println!("gw {}\ngws {}", env!("CARGO_PKG_VERSION"), gws_bin()),
        profile::MANAGEMENT_COMMAND => return profile_command(args),
        _ if first.starts_with('-') => fail_with(&format!("unrecognized option: {first}"), USAGE),
        _ => exec_gws(first, args),
    }
    ExitCode::SUCCESS
}

fn exec_gws(name: &str, args: impl Iterator<Item = OsString>) -> ! {
    let profile = profile::get(name).unwrap_or_else(|err| fail(&err));
    profile::ensure_gws_dir(&profile).unwrap_or_else(|err| fail(&err));

    let mut command = Command::new(gws_bin());
    command.args(args);
    for key in ISOLATED_ENV {
        command.env_remove(key);
    }
    command.env("GOOGLE_WORKSPACE_CLI_CONFIG_DIR", &profile.gws_config_dir);

    let err = command.exec();
    eprintln!("error: cannot execute {}: {err}", gws_bin());
    std::process::exit(127)
}

fn profile_command(mut args: impl Iterator<Item = OsString>) -> ExitCode {
    let Some(sub) = args.next() else {
        fail_with("profile subcommand required", PROFILE_USAGE)
    };
    let sub = sub.to_string_lossy().into_owned();
    let rest: Vec<String> = args.map(|a| a.to_string_lossy().into_owned()).collect();

    match sub.as_str() {
        "add" => profile_add(&rest),
        "list" => profile_list(&rest),
        "show" => profile_show(&rest),
        "-h" | "--help" => println!("{PROFILE_USAGE}"),
        _ => fail_with(&format!("unknown profile subcommand: {sub}"), PROFILE_USAGE),
    }
    ExitCode::SUCCESS
}

fn profile_add(args: &[String]) {
    let mut name = None;
    let mut account = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--account=") {
            account = Some(value.to_string());
        } else if arg == "--account" {
            account = Some(
                iter.next()
                    .unwrap_or_else(|| fail("--account requires a value"))
                    .clone(),
            );
        } else if arg.starts_with('-') {
            fail_with(&format!("unrecognized option: {arg}"), PROFILE_USAGE);
        } else if name.is_none() {
            name = Some(arg.clone());
        } else {
            fail_with(&format!("unexpected argument: {arg}"), PROFILE_USAGE);
        }
    }

    let Some(name) = name else {
        fail_with("profile name required", PROFILE_USAGE)
    };
    let Some(account) = account else {
        fail_with("--account required", PROFILE_USAGE)
    };

    let profile = profile::add(&name, &account).unwrap_or_else(|err| fail(&err));
    println!("added profile {} ({})", profile.name, profile.account);
    println!(
        "run: gw {} auth login   # to authenticate this profile",
        profile.name
    );
}

fn profile_list(args: &[String]) {
    let json = take_json(args, "list");
    if let Some(extra) = args.iter().find(|a| !a.starts_with('-')) {
        fail_with(&format!("unexpected argument: {extra}"), PROFILE_USAGE);
    }
    let profiles = profile::list().unwrap_or_else(|err| fail(&err));

    if json {
        let items: Vec<String> = profiles.iter().map(as_json).collect();
        println!("{{\"profiles\":[{}]}}", items.join(","));
        return;
    }

    let width = profiles.iter().map(|p| p.name.len()).max().unwrap_or(0);
    for profile in &profiles {
        println!("{:width$}  {}", profile.name, profile.account);
    }
}

fn profile_show(args: &[String]) {
    let positional: Vec<&str> = args
        .iter()
        .filter(|a| !a.starts_with('-'))
        .map(String::as_str)
        .collect();
    let [name] = positional[..] else {
        fail_with("profile name required", PROFILE_USAGE)
    };
    let json = take_json(args, "show");
    let profile = profile::get(name).unwrap_or_else(|err| fail(&err));

    if json {
        println!("{}", as_json(&profile));
    } else {
        println!("name     {}", profile.name);
        println!("account  {}", profile.account);
        println!("gws-dir  {}", profile.gws_config_dir.display());
    }
}

fn take_json(args: &[String], subcommand: &str) -> bool {
    let mut json = false;
    for arg in args.iter().filter(|a| a.starts_with('-')) {
        match arg.as_str() {
            "--json" => json = true,
            _ => fail_with(
                &format!("unrecognized option for profile {subcommand}: {arg}"),
                PROFILE_USAGE,
            ),
        }
    }
    json
}

fn as_json(profile: &Profile) -> String {
    format!(
        "{{\"name\":{},\"account\":{},\"gws_config_dir\":{}}}",
        json::string(&profile.name),
        json::string(&profile.account),
        json::string(&profile.gws_config_dir.to_string_lossy())
    )
}
