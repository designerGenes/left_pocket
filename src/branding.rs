use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const PRODUCT_NAME: &str = "left_pocket";
#[allow(dead_code)]
pub const CLI_ALIAS_NAME: &str = "locket";
pub const PRIMARY_BINARY_NAME: &str = "left_pocket";
#[allow(dead_code)]
pub const LEGACY_BINARY_NAMES: &[&str] = &["locket", "corner", "safe_pocket", "spocket"];
pub const REPOSITORY_URL: &str = "https://github.com/designerGenes/left_pocket";

/// ASCII-art pocket logo shown before `--help` and `-v` output.
///
/// The shape is a jeans-style pocket: a flap at the top carrying the product
/// name, a square body, and rounded lower corners. When left_pocket is invoked
/// from a project directory, [`logo_with_pocket_id`] renders the same shape
/// with the current pocket's ID centred inside the body instead.
pub const LOGO: &str = "\
+-------------------+
| left_pocket       |
+-------------------+
|                   |
|                   |
|                   |
|                   |
 \\                 /
  `---------------'";

/// Width of the pocket body interior, in characters. The ID line rendered by
/// [`logo_with_pocket_id`] is padded to exactly this width.
const LOGO_INTERIOR_WIDTH: usize = 19;

/// Centre `label` in a field of `width` characters (padding biased left, so a
/// 12-char pocket ID in the 19-char interior gets 3 leading and 4 trailing
/// spaces). Labels at least as wide as the field are returned unpadded.
fn center_in(label: &str, width: usize) -> String {
    let len = label.chars().count();
    if len >= width {
        return label.to_string();
    }
    let left = (width - len) / 2;
    let right = width - len - left;
    format!("{}{}{}", " ".repeat(left), label, " ".repeat(right))
}

/// Render the pocket logo with `pocket_id` centred inside the pocket body.
pub fn logo_with_pocket_id(pocket_id: &str) -> String {
    let id_line = center_in(pocket_id, LOGO_INTERIOR_WIDTH);
    format!(
        "\
+-------------------+
| left_pocket       |
+-------------------+
|                   |
|                   |
|{id_line}|
|                   |
 \\                 /
  `---------------'"
    )
}

/// Print the logo to stdout followed by a blank line.
pub fn print_logo() {
    println!("{LOGO}");
    println!();
}

/// Print the pocket logo with `pocket_id` centred inside it, followed by a
/// blank line. Used whenever left_pocket opens a project so the terminal
/// shows exactly which pocket was resolved.
pub fn print_pocket_logo(pocket_id: &str) {
    println!("{}", logo_with_pocket_id(pocket_id));
    println!();
}

pub const PRIMARY_ROOT_ENV_KEY: &str = "POCKET_ROOT";
pub const LEGACY_ROOT_ENV_KEYS: &[&str] = &[
    "LEFT_POCKET_ROOT",
    "LOCKET_ROOT",
    "CORNER_ROOT",
    "SPOCKET_ROOT",
];

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_with_pocket_id_centres_the_id() {
        let logo = logo_with_pocket_id("3fcff0d934c6");
        assert!(logo.contains("|   3fcff0d934c6    |"));
        // The frame matches the static logo except for the centred ID line.
        let without_id = logo.replace("|   3fcff0d934c6    |", "|                   |");
        assert_eq!(without_id, LOGO);
    }

    #[test]
    fn logo_with_pocket_id_handles_long_ids() {
        let long_id = "0123456789abcdefghijkl";
        let logo = logo_with_pocket_id(long_id);
        assert!(logo.contains(&format!("|{long_id}|")));
    }

    #[test]
    fn static_logo_has_no_pocket_id() {
        assert!(!LOGO.contains("hash"));
        assert!(LOGO.contains("left_pocket"));
    }
}
