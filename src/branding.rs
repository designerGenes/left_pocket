use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const PRODUCT_NAME: &str = "left_pocket";
#[allow(dead_code)]
pub const CLI_ALIAS_NAME: &str = "locket";
pub const PRIMARY_BINARY_NAME: &str = "left_pocket";
#[allow(dead_code)]
pub const LEGACY_BINARY_NAMES: &[&str] = &["locket", "corner", "safe_pocket", "spocket"];
pub const REPOSITORY_URL: &str = "https://github.com/designerGenes/left_pocket";

/// ASCII-art logo shown before `--help` and `-v` output.
///
/// The outer rectangle is 10 columns wide (right edge at column 9). A smaller
/// inset rectangle (columns 6-9) houses the filled `▓▓` block on line 4, and
/// the product name `left_pocket` sits on line 3 after three spaces.
pub const LOGO: &str = "\
╭────────╮
│        │
│     ╭──┤   left_pocket
│     │▓▓│
╰─────┴──╯";

/// Print the logo to stdout followed by a blank line.
pub fn print_logo() {
    println!("{LOGO}");
    println!();
}

pub const PRIMARY_ROOT_ENV_KEY: &str = "LEFT_POCKET_ROOT";
pub const LEGACY_ROOT_ENV_KEYS: &[&str] = &["LOCKET_ROOT", "CORNER_ROOT", "SPOCKET_ROOT"];

pub const PRIMARY_REGISTRY_DIRNAME: &str = ".left_pocket";
pub const LEGACY_REGISTRY_DIRNAMES: &[&str] = &[".corner", ".safe_pocket", ".spocket"];

pub const PRIMARY_CONFIG_DIRNAME: &str = "left_pocket";
pub const LEGACY_CONFIG_DIRNAMES: &[&str] = &["corner", "safe_pocket", "spocket"];

pub const PRIMARY_BACKUP_REPO_DIRNAME: &str = ".left_pocket_backup_repo";
pub const LEGACY_BACKUP_REPO_DIRNAMES: &[&str] = &[
    ".corner_backup_repo",
    ".safe_pocket_backup_repo",
    ".spocket_backup_repo",
];

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("Failed to get home directory")
}

fn config_base_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config"))
}

pub fn current_registry_root() -> Result<PathBuf> {
    Ok(home_dir()?.join(PRIMARY_REGISTRY_DIRNAME))
}

pub fn current_config_root() -> Result<PathBuf> {
    Ok(config_base_dir()?.join(PRIMARY_CONFIG_DIRNAME))
}

fn preferred_existing_root(candidates: &[PathBuf]) -> PathBuf {
    if let Some(primary) = candidates.first().filter(|path| path.exists()) {
        return primary.clone();
    }

    for candidate in candidates.iter().skip(1) {
        if candidate.exists() {
            return candidate.clone();
        }
    }

    candidates
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn resolve_relative_path(candidates: &[PathBuf], relative: &Path) -> PathBuf {
    for root in candidates {
        let candidate = root.join(relative);
        if candidate.exists() {
            return candidate;
        }
    }

    preferred_existing_root(candidates).join(relative)
}

pub fn known_registry_roots() -> Result<Vec<PathBuf>> {
    let home = home_dir()?;
    let mut roots = vec![home.join(PRIMARY_REGISTRY_DIRNAME)];
    roots.extend(LEGACY_REGISTRY_DIRNAMES.iter().map(|name| home.join(name)));
    Ok(roots)
}

pub fn preferred_registry_root() -> Result<PathBuf> {
    Ok(preferred_existing_root(&known_registry_roots()?))
}

pub fn resolve_registry_relative_path(relative: &Path) -> Result<PathBuf> {
    Ok(resolve_relative_path(&known_registry_roots()?, relative))
}

pub fn known_config_roots() -> Result<Vec<PathBuf>> {
    let base = config_base_dir()?;
    let mut roots = vec![base.join(PRIMARY_CONFIG_DIRNAME)];
    roots.extend(LEGACY_CONFIG_DIRNAMES.iter().map(|name| base.join(name)));
    Ok(roots)
}

pub fn preferred_config_root() -> Result<PathBuf> {
    Ok(preferred_existing_root(&known_config_roots()?))
}

pub fn resolve_config_relative_path(relative: &Path) -> Result<PathBuf> {
    Ok(resolve_relative_path(&known_config_roots()?, relative))
}

pub fn known_backup_repo_paths() -> Result<Vec<PathBuf>> {
    let home = home_dir()?;
    let mut roots = vec![home.join(PRIMARY_BACKUP_REPO_DIRNAME)];
    roots.extend(
        LEGACY_BACKUP_REPO_DIRNAMES
            .iter()
            .map(|name| home.join(name)),
    );
    Ok(roots)
}

pub fn preferred_backup_repo_path() -> Result<PathBuf> {
    Ok(preferred_existing_root(&known_backup_repo_paths()?))
}

pub fn workspace_folder_name(hash: &str) -> String {
    format!("[{PRODUCT_NAME}] {hash}")
}

pub fn root_env_keys() -> Vec<&'static str> {
    let mut keys = vec![PRIMARY_ROOT_ENV_KEY];
    keys.extend(LEGACY_ROOT_ENV_KEYS.iter().copied());
    keys
}
