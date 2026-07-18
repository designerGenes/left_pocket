use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const PRODUCT_NAME: &str = "Corner";
pub const PRIMARY_BINARY_NAME: &str = "corner";
#[allow(dead_code)]
pub const LEGACY_BINARY_NAMES: &[&str] = &["safe_pocket", "spocket"];

pub const PRIMARY_REGISTRY_DIRNAME: &str = ".corner";
pub const LEGACY_REGISTRY_DIRNAMES: &[&str] = &[".safe_pocket", ".spocket"];

pub const PRIMARY_CONFIG_DIRNAME: &str = "corner";
pub const LEGACY_CONFIG_DIRNAMES: &[&str] = &["safe_pocket", "spocket"];

pub const PRIMARY_BACKUP_REPO_DIRNAME: &str = ".corner_backup_repo";
pub const LEGACY_BACKUP_REPO_DIRNAMES: &[&str] =
    &[".safe_pocket_backup_repo", ".spocket_backup_repo"];

fn home_dir() -> Result<PathBuf> {
    dirs::home_dir().context("Failed to get home directory")
}

fn config_base_dir() -> Result<PathBuf> {
    Ok(home_dir()?.join(".config"))
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
