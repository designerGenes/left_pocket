use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub const PRODUCT_NAME: &str = "left_pocket";
#[allow(dead_code)]
pub const CLI_ALIAS_NAME: &str = "locket";
pub const PRIMARY_BINARY_NAME: &str = "left_pocket";
#[allow(dead_code)]
pub const LEGACY_BINARY_NAMES: &[&str] = &["locket", "corner", "safe_pocket", "spocket"];
pub const REPOSITORY_URL: &str = "https://github.com/designerGenes/left_pocket";

// Embedded factory-default logo, used only when the user-editable resource file
// has not been installed yet. The runtime copy lives at
// `$POCKET_CONFIG_ROOT/logo.md` and can be edited (or replaced) at any time.
const EMBEDDED_LOGO: &str = include_str!("templates/values/logo.md");

/// Return the path to the user-editable logo resource, preferring the primary
/// config root but falling back to legacy config roots if a logo was placed
/// there before the rename.
fn logo_file_path() -> Option<PathBuf> {
    known_config_roots()
        .ok()?
        .into_iter()
        .map(|root| root.join("logo.md"))
        .find(|path| path.exists())
}

/// Read the logo template from the installed resource file, or fall back to the
/// embedded factory default if none exists.
fn logo_template() -> String {
    if let Some(path) = logo_file_path() {
        if let Ok(content) = fs::read_to_string(&path) {
            return content;
        }
    }
    embedded_logo_template()
}

/// Strip the `#POCKET_INSTALL_DESTINATION` directive (and legacy equivalents)
/// from the embedded factory-default logo so it matches the installed resource.
fn embedded_logo_template() -> String {
    let mut kept = Vec::new();
    let mut removed = false;
    for line in EMBEDDED_LOGO.lines() {
        if is_install_destination_line(line) {
            removed = true;
            continue;
        }
        kept.push(line);
    }
    let mut body = kept.join("\n");
    if EMBEDDED_LOGO.ends_with('\n') && !body.is_empty() {
        body.push('\n');
    }
    if removed {
        while body.starts_with('\n') {
            body.remove(0);
        }
    }
    body
}

fn is_install_destination_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("#POCKET_INSTALL_DESTINATION")
        || trimmed.starts_with("#LEFT_POCKET_INSTALL_DESTINATION")
        || trimmed.starts_with("#CORNER_INSTALL_DESTINATION")
        || trimmed.starts_with("#SPOCKET_INSTALL_DESTINATION")
}

/// Render the pocket logo, substituting `{pocket_id}` with the given value.
/// Pass an empty string for the no-id logo shown in `--help` and `-v`.
pub fn logo(pocket_id: &str) -> String {
    let template = logo_template();
    render_logo(&template, pocket_id)
}

fn render_logo(template: &str, pocket_id: &str) -> String {
    template
        .replace("{pocket_id}", pocket_id)
        .trim_end()
        .to_string()
}

/// Print the logo to stdout surrounded by blank lines.
pub fn print_logo() {
    println!();
    println!("{}", logo(""));
    println!();
}

/// Print the pocket logo with `pocket_id` placed below it, surrounded by
/// blank lines. Used whenever left_pocket opens a project so the terminal
/// shows exactly which pocket was resolved.
pub fn print_pocket_logo(pocket_id: &str) {
    println!();
    println!("{}", logo(pocket_id));
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

/// If `path` lives inside a known left_pocket registry root, return that root
/// in its raw (HOME-based) form together with `path`'s relative path beneath
/// it. Matching is performed against both the raw root and its symlink-
/// resolved form, so a registry reached through a symlinked HOME (macOS
/// `/var` → `/private/var`, a symlinked `$HOME`, …) is still recognised.
/// Callers that compare against canonicalized paths (`std::env::current_dir`,
/// `Config::resolve_path`) must use this instead of a plain `starts_with`
/// against `known_registry_roots()`, otherwise a pocket directory is mistaken
/// for a regular project path.
pub fn registry_root_containing(path: &Path) -> Result<Option<(PathBuf, PathBuf)>> {
    for root in known_registry_roots()? {
        let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
        for candidate in [root.clone(), canonical] {
            if let Ok(relative) = path.strip_prefix(&candidate) {
                return Ok(Some((root, relative.to_path_buf())));
            }
        }
    }
    Ok(None)
}

/// Convenience predicate: does `path` live inside a known left_pocket
/// registry root (symlink-aware)? See [`registry_root_containing`].
pub fn is_registry_internal_path(path: &Path) -> bool {
    registry_root_containing(path).ok().flatten().is_some()
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
    fn logo_contains_product_name_and_shape() {
        let rendered = render_logo(&embedded_logo_template(), "");
        assert!(rendered.contains("left_pocket"));
        assert!(rendered.contains('╭'));
        assert!(!rendered.contains("{pocket_id}"));
    }

    #[test]
    fn logo_with_pocket_id_renders_id_outside_walls() {
        let rendered = render_logo(&embedded_logo_template(), "3fcff0d934c6");
        assert!(rendered.contains("3fcff0d934c6"));
        for line in rendered.lines() {
            if line.contains("3fcff0d934c6") {
                assert!(
                    !line.starts_with('│'),
                    "pocket id should be outside the logo walls, got: {line}"
                );
            }
        }
    }

    #[test]
    fn logo_with_pocket_id_handles_long_ids() {
        let long_id = "0123456789abcdefghijkl";
        let rendered = render_logo(&embedded_logo_template(), long_id);
        assert!(rendered.contains(long_id));
    }

    #[test]
    fn embedded_logo_template_strips_install_directive() {
        let template = embedded_logo_template();
        assert!(!template.contains("#POCKET_INSTALL_DESTINATION"));
        assert!(!template.contains("#LEFT_POCKET_INSTALL_DESTINATION"));
        assert!(template.contains("left_pocket"));
    }
}
