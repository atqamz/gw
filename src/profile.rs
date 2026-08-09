use std::env;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub const MANAGEMENT_COMMAND: &str = "profile";

const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;
const MAX_NAME_LEN: usize = 64;
const MAX_ACCOUNT_LEN: usize = 254;

pub struct Profile {
    pub name: String,
    pub account: String,
    pub gws_config_dir: PathBuf,
}

pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("profile name must not be empty".to_string());
    }
    if name.len() > MAX_NAME_LEN {
        return Err(format!(
            "profile name must be at most {MAX_NAME_LEN} characters"
        ));
    }
    if name.starts_with('-') || name.starts_with('.') {
        return Err(format!("invalid profile name: {name:?}"));
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        return Err(format!("invalid profile name: {name:?}"));
    }
    Ok(())
}

fn validate_account(account: &str) -> Result<(), String> {
    if account.is_empty() {
        return Err("account must not be empty".to_string());
    }
    if account.len() > MAX_ACCOUNT_LEN {
        return Err(format!("account must be at most {MAX_ACCOUNT_LEN} bytes"));
    }
    if account.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(format!("invalid account: {account:?}"));
    }
    if !account.contains('@') {
        return Err(format!("account must be an email address: {account:?}"));
    }
    Ok(())
}

pub fn config_root() -> Result<PathBuf, String> {
    if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return Ok(dir.join("gw"));
        }
    }
    let home = env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "cannot locate config root: HOME is not set".to_string())?;
    if !home.is_absolute() {
        return Err("cannot locate config root: HOME is not absolute".to_string());
    }
    Ok(home.join(".config").join("gw"))
}

pub fn profiles_dir() -> Result<PathBuf, String> {
    Ok(config_root()?.join("profiles"))
}

pub fn profile_dir(name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let profiles = profiles_dir()?;
    let dir = profiles.join(name);
    if dir.parent() != Some(profiles.as_path()) {
        return Err(format!("invalid profile name: {name:?}"));
    }
    Ok(dir)
}

fn read_account(dir: &Path) -> String {
    fs::read_to_string(dir.join("account"))
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .collect()
}

fn load(name: &str, dir: PathBuf) -> Profile {
    Profile {
        name: name.to_string(),
        account: read_account(&dir),
        gws_config_dir: dir.join("gws"),
    }
}

pub fn get(name: &str) -> Result<Profile, String> {
    let dir = profile_dir(name)?;
    if !dir.is_dir() {
        return Err(format!("unknown profile: {name:?}"));
    }
    Ok(load(name, dir))
}

pub fn list() -> Result<Vec<Profile>, String> {
    let profiles = profiles_dir()?;
    let entries = match fs::read_dir(&profiles) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("cannot read {}: {err}", profiles.display())),
    };

    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| format!("cannot read {}: {err}", profiles.display()))?;
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if validate_name(&name).is_err() {
            continue;
        }
        found.push(load(&name, entry.path()));
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

pub fn ensure_gws_dir(profile: &Profile) -> Result<(), String> {
    make_dir(&profile.gws_config_dir)
}

fn make_dir(path: &Path) -> Result<(), String> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(path)
        .map_err(|err| format!("cannot create {}: {err}", path.display()))
}

pub fn add(name: &str, account: &str) -> Result<Profile, String> {
    if name == MANAGEMENT_COMMAND {
        return Err(format!(
            "profile name {MANAGEMENT_COMMAND:?} is reserved for profile management"
        ));
    }
    validate_account(account)?;
    let dir = profile_dir(name)?;
    if dir.exists() {
        return Err(format!("profile already exists: {name:?}"));
    }

    make_dir(&dir.join("gws"))?;

    let account_path = dir.join("account");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(FILE_MODE)
        .open(&account_path)
        .map_err(|err| format!("cannot create {}: {err}", account_path.display()))?;
    writeln!(file, "{account}")
        .map_err(|err| format!("cannot write {}: {err}", account_path.display()))?;

    Ok(load(name, dir))
}

#[cfg(test)]
mod tests {
    use super::{validate_account, validate_name};

    #[test]
    fn accepts_conservative_names() {
        for name in ["personal", "work", "a", "team-1", "team_1", "v0.1", "A9"] {
            assert!(validate_name(name).is_ok(), "{name} should be accepted");
        }
    }

    #[test]
    fn rejects_unsafe_names() {
        for name in [
            "",
            ".",
            "..",
            "../escape",
            "a/b",
            "a\\b",
            "/etc/passwd",
            "-flag",
            ".hidden",
            "with space",
            "new\nline",
            "nul\0byte",
            "esc\u{1b}[0m",
            "café",
            "prof:ile",
        ] {
            assert!(validate_name(name).is_err(), "{name:?} should be rejected");
        }
        assert!(validate_name(&"a".repeat(65)).is_err());
    }

    #[test]
    fn rejects_unsafe_accounts() {
        for account in ["", "no-at-sign", "a b@example.com", "a@example.com\nx"] {
            assert!(
                validate_account(account).is_err(),
                "{account:?} should be rejected"
            );
        }
        assert!(validate_account("user@example.com").is_ok());
    }
}
