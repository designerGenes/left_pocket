use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::manifest::Manifest;

const ALIASES_FILE: &str = "aliases";
const CACHE_FILE: &str = "registry_cache.json";
const CACHE_TMP: &str = "registry_cache.json.tmp";
const CACHE_VERSION: u32 = 1;
const LEGACY_CONFIG_OBSERVATIONS: &str = ".config/safe_pocket/observations";
const TEMPORARY_DIR: &str = "temporary";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryCache {
    #[serde(default = "default_cache_version")]
    pub version: u32,
    #[serde(default = "now")]
    pub generated_at: DateTime<Utc>,
    #[serde(default)]
    pub pockets: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Stable pocket id, derived from the directory name under `~/.safe_pocket`.
    pub hash: String,
    /// Current manifest hash. This can differ after sync/augment updates paths in place.
    pub manifest_hash: String,
    pub path: PathBuf,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub temporary: bool,
    #[serde(default)]
    pub core_paths: Vec<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub worktrees: Vec<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub augmented_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birth_hash: Option<String>,
    #[serde(default = "default_manifest_version")]
    pub manifest_version: u32,
}

impl RegistryEntry {
    pub fn all_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.core_paths.iter().chain(self.worktrees.iter())
    }
}

impl Default for RegistryCache {
    fn default() -> Self {
        Self {
            version: CACHE_VERSION,
            generated_at: Utc::now(),
            pockets: Vec::new(),
        }
    }
}

fn default_cache_version() -> u32 {
    CACHE_VERSION
}

fn default_manifest_version() -> u32 {
    1
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

pub fn registry_root() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".safe_pocket"))
}

pub fn registry_dir() -> Result<PathBuf> {
    let dir = registry_root()?;
    fs::create_dir_all(&dir).context("Failed to create safe pocket registry directory")?;
    Ok(dir)
}

pub fn temporary_registry_dir() -> Result<PathBuf> {
    let dir = registry_dir()?.join(TEMPORARY_DIR);
    fs::create_dir_all(&dir).context("Failed to create temporary safe pocket directory")?;
    Ok(dir)
}

pub fn aliases_path() -> Result<PathBuf> {
    Ok(registry_dir()?.join(ALIASES_FILE))
}

pub fn global_observations_dir() -> Result<PathBuf> {
    let dir = registry_dir()?.join("observations");
    migrate_legacy_observations(&dir)?;
    fs::create_dir_all(&dir).context("Failed to create global observations directory")?;
    Ok(dir)
}

fn migrate_legacy_observations(new_dir: &Path) -> Result<()> {
    if new_dir.exists() {
        return Ok(());
    }

    let legacy_dir = match dirs::home_dir() {
        Some(home) => home.join(LEGACY_CONFIG_OBSERVATIONS),
        None => return Ok(()),
    };

    if !legacy_dir.exists() {
        return Ok(());
    }

    fs::rename(&legacy_dir, new_dir).or_else(|_| copy_dir_all(&legacy_dir, new_dir))
}

fn cache_path_for(root: &Path) -> PathBuf {
    root.join(CACHE_FILE)
}

fn cache_tmp_path_for(root: &Path) -> PathBuf {
    root.join(CACHE_TMP)
}

pub fn is_registry_pocket_dir(pocket_dir: &Path) -> Result<bool> {
    let parent = match pocket_dir.parent() {
        Some(parent) => parent,
        None => return Ok(false),
    };

    Ok(parent == registry_root()?.as_path() || parent == temporary_registry_dir()?.as_path())
}

pub fn load_cache_or_rebuild() -> Result<RegistryCache> {
    let root = registry_dir()?;
    load_cache_or_rebuild_from(&root)
}

fn load_cache_or_rebuild_from(root: &Path) -> Result<RegistryCache> {
    let cache_path = cache_path_for(root);

    if !cache_path.exists() {
        return rebuild_cache_from(root);
    }

    let mut cache = match read_cache_from(root) {
        Ok(cache) => cache,
        Err(_) => return rebuild_cache_from(root),
    };

    let before = cache.pockets.len();
    cache.pockets.retain(|entry| entry.path.is_dir());
    sort_entries(&mut cache.pockets);

    if cache.pockets.len() != before {
        cache.generated_at = Utc::now();
        write_cache_to(root, &cache)?;
    }

    Ok(cache)
}

fn rebuild_cache_from(root: &Path) -> Result<RegistryCache> {
    fs::create_dir_all(root).context("Failed to create safe pocket registry directory")?;

    let mut cache = RegistryCache::default();
    collect_pockets_from_dir(root, &mut cache)?;

    let temporary_root = root.join(TEMPORARY_DIR);
    if temporary_root.exists() {
        collect_pockets_from_dir(&temporary_root, &mut cache)?;
    }

    sort_entries(&mut cache.pockets);
    write_cache_to(root, &cache)?;
    Ok(cache)
}

pub fn upsert_pocket(pocket_dir: &Path, manifest: &Manifest) -> Result<()> {
    if !is_registry_pocket_dir(pocket_dir)? {
        return Ok(());
    }

    let root = registry_dir()?;
    let mut cache = load_cache_or_rebuild_from(&root)?;
    let entry = entry_from_manifest(pocket_dir, manifest);

    cache
        .pockets
        .retain(|existing| existing.path != entry.path && existing.hash != entry.hash);
    cache.pockets.push(entry);
    sort_entries(&mut cache.pockets);
    cache.generated_at = Utc::now();

    write_cache_to(&root, &cache)
}

pub fn remove_pocket(pocket_dir: &Path) -> Result<()> {
    if !is_registry_pocket_dir(pocket_dir)? {
        return Ok(());
    }

    let root = registry_dir()?;
    let mut cache = load_cache_or_rebuild_from(&root)?;
    let before = cache.pockets.len();
    cache.pockets.retain(|entry| entry.path != pocket_dir);

    if cache.pockets.len() != before {
        cache.generated_at = Utc::now();
        write_cache_to(&root, &cache)?;
    }

    Ok(())
}

fn read_cache_from(root: &Path) -> Result<RegistryCache> {
    let content =
        fs::read_to_string(cache_path_for(root)).context("Failed to read registry cache")?;
    serde_json::from_str(&content).context("Failed to parse registry cache")
}

fn write_cache_to(root: &Path, cache: &RegistryCache) -> Result<()> {
    fs::create_dir_all(root).context("Failed to create safe pocket registry directory")?;
    let content =
        serde_json::to_string_pretty(cache).context("Failed to serialize registry cache")?;
    let tmp_path = cache_tmp_path_for(root);
    fs::write(&tmp_path, content).context("Failed to write registry cache tmp file")?;
    fs::rename(&tmp_path, cache_path_for(root)).context("Failed to rename registry cache")?;
    Ok(())
}

fn entry_from_manifest(pocket_dir: &Path, manifest: &Manifest) -> RegistryEntry {
    let hash = pocket_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&manifest.hash)
        .to_string();

    RegistryEntry {
        hash,
        manifest_hash: manifest.hash.clone(),
        path: pocket_dir.to_path_buf(),
        created_at: manifest.created_at,
        temporary: manifest.temporary,
        core_paths: manifest.core_paths.clone(),
        worktrees: manifest.worktrees.clone(),
        parent_hash: manifest.parent_hash.clone(),
        children: manifest.children.clone(),
        augmented_from: manifest.augmented_from.clone(),
        birth_hash: manifest.birth_hash.clone(),
        manifest_version: manifest.version,
    }
}

fn sort_entries(entries: &mut [RegistryEntry]) {
    entries.sort_by(|a, b| a.hash.cmp(&b.hash));
}

fn collect_pockets_from_dir(root: &Path, cache: &mut RegistryCache) -> Result<()> {
    for entry in fs::read_dir(root).context("Failed to read safe pocket registry directory")? {
        let entry = entry?;
        let pocket_dir = entry.path();

        if !pocket_dir.is_dir() || is_reserved_registry_dir(&pocket_dir) {
            continue;
        }

        let dir_name = match pocket_dir.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        let manifest = match Manifest::load_without_registry_update(&pocket_dir)? {
            Some(manifest) => manifest,
            None if has_workspace_file(&pocket_dir, &dir_name) => {
                Manifest::backfill_without_registry_update(&pocket_dir, &dir_name)?
            }
            None => continue,
        };

        cache
            .pockets
            .push(entry_from_manifest(&pocket_dir, &manifest));
    }

    Ok(())
}

fn is_reserved_registry_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("observations" | "registry" | TEMPORARY_DIR)
    )
}

fn has_workspace_file(pocket_dir: &Path, hash: &str) -> bool {
    if pocket_dir.join(format!("{}.code-workspace", hash)).exists() {
        return true;
    }

    fs::read_dir(pocket_dir)
        .map(|entries| {
            entries.flatten().any(|entry| {
                entry.path().extension().and_then(|ext| ext.to_str()) == Some("code-workspace")
            })
        })
        .unwrap_or(false)
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).context("Failed to create global observations directory")?;

    for entry in fs::read_dir(src).context("Failed to read legacy observations directory")? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else if !dst_path.exists() {
            fs::copy(&src_path, &dst_path).context("Failed to copy legacy observation file")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rebuild_cache_includes_manifest_metadata() {
        let root = std::env::temp_dir().join("spocket_registry_cache_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let pocket_dir = root.join("abc123");
        fs::create_dir_all(&pocket_dir).unwrap();
        let mut manifest = Manifest::new("manifest_hash".to_string(), vec![PathBuf::from("/p")]);
        manifest.worktrees.push(PathBuf::from("/p-worktree"));
        manifest.save(&pocket_dir).unwrap();

        let cache = rebuild_cache_from(&root).unwrap();

        assert_eq!(cache.pockets.len(), 1);
        assert_eq!(cache.pockets[0].hash, "abc123");
        assert_eq!(cache.pockets[0].manifest_hash, "manifest_hash");
        assert_eq!(cache.pockets[0].path, pocket_dir);
        assert_eq!(cache.pockets[0].core_paths, vec![PathBuf::from("/p")]);
        assert_eq!(
            cache.pockets[0].worktrees,
            vec![PathBuf::from("/p-worktree")]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_cache_load_prunes_deleted_pockets() {
        let root = std::env::temp_dir().join("spocket_registry_prune_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let cache = RegistryCache {
            pockets: vec![RegistryEntry {
                hash: "missing".to_string(),
                manifest_hash: "missing".to_string(),
                path: root.join("missing"),
                created_at: Utc::now(),
                temporary: false,
                core_paths: Vec::new(),
                worktrees: Vec::new(),
                parent_hash: None,
                children: Vec::new(),
                augmented_from: None,
                birth_hash: None,
                manifest_version: 1,
            }],
            ..RegistryCache::default()
        };
        write_cache_to(&root, &cache).unwrap();

        let loaded = load_cache_or_rebuild_from(&root).unwrap();
        assert!(loaded.pockets.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_rebuild_cache_includes_temporary_pockets() {
        let root = std::env::temp_dir().join("spocket_registry_temporary_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("temporary")).unwrap();

        let pocket_dir = root.join("temporary").join("temp123");
        fs::create_dir_all(&pocket_dir).unwrap();
        let manifest = Manifest::new_with_options(
            "manifest_hash".to_string(),
            vec![PathBuf::from("/tmp/project")],
            true,
        );
        manifest.save(&pocket_dir).unwrap();

        let cache = rebuild_cache_from(&root).unwrap();
        assert_eq!(cache.pockets.len(), 1);
        assert!(cache.pockets[0].temporary);
        assert_eq!(cache.pockets[0].path, pocket_dir);

        let _ = fs::remove_dir_all(&root);
    }
}
