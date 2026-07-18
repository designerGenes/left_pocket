use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::manifest::Manifest;

const ALIASES_FILE: &str = "aliases";
const CACHE_FILE: &str = "registry_cache.json";
const CACHE_TMP: &str = "registry_cache.json.tmp";
const CACHE_VERSION: u32 = 1;
const LEGACY_CONFIG_OBSERVATIONS: &str = ".config/safe_pocket/observations";
const SNAPSHOTS_DIR: &str = "snapshots";
const TEMPORARY_DIR: &str = "temporary";
const SNAPSHOT_CHUNK_SIZE: usize = 45 * 1024 * 1024;
const REGISTRY_GITIGNORE: &str =
    ".DS_Store\n/*/\n!/observations/\n!/snapshots/\n!/snapshots/**\n/temporary/\n";
const PRE_COMMIT_HOOK: &str =
    "#!/bin/sh\nset -eu\ncorner sync-registry-git >/dev/null\ngit add -A .\n";

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
    crate::branding::preferred_registry_root()
}

pub fn registry_dir() -> Result<PathBuf> {
    let dir = registry_root()?;
    fs::create_dir_all(&dir).context("Failed to create safe pocket registry directory")?;
    ensure_registry_git_state_from(&dir)?;
    Ok(dir)
}

pub fn sync_registry_git_state() -> Result<usize> {
    let root = registry_dir()?;
    ensure_registry_git_state_from(&root)?;
    sync_registry_snapshot_from(&root)
}

pub fn temporary_registry_dir() -> Result<PathBuf> {
    let dir = registry_dir()?.join(TEMPORARY_DIR);
    fs::create_dir_all(&dir).context("Failed to create temporary pocket directory")?;
    Ok(dir)
}

pub fn aliases_path() -> Result<PathBuf> {
    crate::branding::resolve_registry_relative_path(Path::new(ALIASES_FILE))
}

pub fn global_observations_dir() -> Result<PathBuf> {
    let dir = crate::branding::resolve_registry_relative_path(Path::new("observations"))?;
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

    for root in crate::branding::known_registry_roots()? {
        if parent == root.as_path() || parent == root.join(TEMPORARY_DIR).as_path() {
            return Ok(true);
        }
    }

    Ok(false)
}

pub fn load_cache_or_rebuild() -> Result<RegistryCache> {
    let write_root = registry_dir()?;
    let mut merged = RegistryCache::default();
    let mut seen_paths = HashSet::new();
    let mut any_existing_root = false;

    for root in crate::branding::known_registry_roots()? {
        if !root.exists() {
            continue;
        }

        any_existing_root = true;
        let cache = load_cache_or_rebuild_from(&root)?;
        for entry in cache.pockets {
            if seen_paths.insert(entry.path.clone()) {
                merged.pockets.push(entry);
            }
        }
    }

    if !any_existing_root {
        return load_cache_or_rebuild_from(&write_root);
    }

    Ok(merged)
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
    fs::create_dir_all(root).context("Failed to create pocket registry directory")?;

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
    ensure_registry_git_state_from(&root)?;
    let mut cache = load_cache_or_rebuild_from(&root)?;
    let entry = entry_from_manifest(pocket_dir, manifest);

    cache
        .pockets
        .retain(|existing| existing.path != entry.path && existing.hash != entry.hash);
    cache.pockets.push(entry);
    sort_entries(&mut cache.pockets);
    cache.generated_at = Utc::now();

    write_cache_to(&root, &cache)?;
    let _ = sync_registry_snapshot_from(&root);
    Ok(())
}

pub fn remove_pocket(pocket_dir: &Path) -> Result<()> {
    if !is_registry_pocket_dir(pocket_dir)? {
        return Ok(());
    }

    let root = registry_dir()?;
    ensure_registry_git_state_from(&root)?;
    let mut cache = load_cache_or_rebuild_from(&root)?;
    let before = cache.pockets.len();
    cache.pockets.retain(|entry| entry.path != pocket_dir);

    if cache.pockets.len() != before {
        cache.generated_at = Utc::now();
        write_cache_to(&root, &cache)?;
    }

    let _ = sync_registry_snapshot_from(&root);
    Ok(())
}

pub fn move_to_unhoused(path: &Path, operation: &str) -> Result<Option<PathBuf>> {
    let root = crate::branding::known_registry_roots()?
        .into_iter()
        .find(|root| path.starts_with(root))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Refusing to move path outside known pocket roots: {}",
                path.display()
            )
        })?;
    if !path.exists() {
        return Ok(None);
    }

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("item");
    let mut target = root.join("unhoused").join(&timestamp).join(name);
    let mut suffix = 1;
    while target.exists() {
        let file_name = target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("item")
            .to_string();
        target.set_file_name(format!("{file_name}.{suffix}"));
        suffix += 1;
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).context("Failed to create registry unhoused directory")?;
    }
    fs::rename(path, &target).with_context(|| {
        format!(
            "Failed to move safe pocket content to unhoused: {} -> {}",
            path.display(),
            target.display()
        )
    })?;

    let log_path = root.join("unhoused.log");
    let entry = format!(
        "{}\t{}\t{}\t{}\n",
        Utc::now().to_rfc3339(),
        operation,
        path.display(),
        target.display()
    );
    use std::io::Write as IoWrite;
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .context("Failed to open registry unhoused log")?;
    log.write_all(entry.as_bytes())
        .context("Failed to write registry unhoused log")?;
    let _ = crate::event::append_registry_event(
        "content.unhoused",
        serde_json::json!({
            "operation": operation,
            "original_path": path,
            "unhoused_path": target,
        }),
    );

    let _ = sync_registry_git_state();

    Ok(Some(target))
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
        Some("observations" | "registry" | "unhoused" | SNAPSHOTS_DIR | TEMPORARY_DIR)
    )
}

fn ensure_registry_git_state_from(root: &Path) -> Result<()> {
    fs::create_dir_all(root).context("Failed to create pocket registry root")?;

    if !root.join(".git").exists() {
        let output = Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .with_context(|| {
                format!("Failed to initialize git repository in {}", root.display())
            })?;
        if !output.status.success() {
            bail!(
                "Failed to initialize git repository in {}: {}",
                root.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    ensure_registry_gitignore(root)?;
    ensure_registry_pre_commit_hook(root)?;
    Ok(())
}

fn ensure_registry_gitignore(root: &Path) -> Result<()> {
    let path = root.join(".gitignore");
    let mut content = if path.exists() {
        fs::read_to_string(&path)
            .with_context(|| format!("Failed to read registry gitignore: {}", path.display()))?
    } else {
        String::new()
    };

    for line in REGISTRY_GITIGNORE.lines() {
        if !content.lines().any(|existing| existing == line) {
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(line);
            content.push('\n');
        }
    }

    fs::write(&path, content)
        .with_context(|| format!("Failed to write registry gitignore: {}", path.display()))?;
    Ok(())
}

fn ensure_registry_pre_commit_hook(root: &Path) -> Result<()> {
    let hook_path = root.join(".git").join("hooks").join("pre-commit");
    if let Some(parent) = hook_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create hook directory: {}", parent.display()))?;
    }

    fs::write(&hook_path, PRE_COMMIT_HOOK)
        .with_context(|| format!("Failed to write pre-commit hook: {}", hook_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&hook_path)
            .with_context(|| format!("Failed to stat hook: {}", hook_path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hook_path, permissions).with_context(|| {
            format!(
                "Failed to set executable permissions on hook: {}",
                hook_path.display()
            )
        })?;
    }

    Ok(())
}

fn sync_registry_snapshot_from(root: &Path) -> Result<usize> {
    let snapshots_root = root.join(SNAPSHOTS_DIR);
    let pockets_root = snapshots_root.join("pockets");
    let temporary_root = snapshots_root.join(TEMPORARY_DIR);
    fs::create_dir_all(&pockets_root).with_context(|| {
        format!(
            "Failed to create snapshots directory: {}",
            pockets_root.display()
        )
    })?;
    fs::create_dir_all(&temporary_root).with_context(|| {
        format!(
            "Failed to create temporary snapshots directory: {}",
            temporary_root.display()
        )
    })?;

    let mut expected = Vec::new();
    let mut count = 0;

    for entry in fs::read_dir(root).context("Failed to read safe pocket registry root")? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir()
            || is_reserved_registry_dir(&path)
            || path.file_name().and_then(|n| n.to_str()) == Some(".git")
        {
            continue;
        }

        let name = match path.file_name().and_then(|name| name.to_str()) {
            Some(name) => name,
            None => continue,
        };
        let target = pockets_root.join(name);
        mirror_snapshot_dir(&path, &target)?;
        expected.push(target);
        count += 1;
    }

    let temporary_source = root.join(TEMPORARY_DIR);
    if temporary_source.exists() {
        for entry in fs::read_dir(&temporary_source).with_context(|| {
            format!(
                "Failed to read temporary pockets: {}",
                temporary_source.display()
            )
        })? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let name = match path.file_name().and_then(|name| name.to_str()) {
                Some(name) => name,
                None => continue,
            };
            let target = temporary_root.join(name);
            mirror_snapshot_dir(&path, &target)?;
            expected.push(target);
            count += 1;
        }
    }

    prune_stale_snapshot_children(&pockets_root, &expected)?;
    prune_stale_snapshot_children(&temporary_root, &expected)?;

    Ok(count)
}

fn mirror_snapshot_dir(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        fs::remove_dir_all(dst).with_context(|| {
            format!("Failed to remove old snapshot directory: {}", dst.display())
        })?;
    }
    copy_dir_all_without_git(src, dst)
}

fn copy_dir_all_without_git(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)
        .with_context(|| format!("Failed to create snapshot directory: {}", dst.display()))?;

    for entry in fs::read_dir(src)
        .with_context(|| format!("Failed to read source directory: {}", src.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
                continue;
            }
            copy_dir_all_without_git(&path, &target)?;
        } else {
            copy_file_for_snapshot(&path, &target)?;
        }
    }

    Ok(())
}

fn copy_file_for_snapshot(src: &Path, dst: &Path) -> Result<()> {
    let metadata = fs::metadata(src).with_context(|| {
        format!(
            "Failed to read file metadata for snapshot: {}",
            src.display()
        )
    })?;

    if metadata.len() as usize <= SNAPSHOT_CHUNK_SIZE {
        fs::copy(src, dst).with_context(|| {
            format!(
                "Failed to copy safe pocket content into snapshot: {} -> {}",
                src.display(),
                dst.display()
            )
        })?;
        return Ok(());
    }

    let bytes = fs::read(src)
        .with_context(|| format!("Failed to read large file for snapshot: {}", src.display()))?;
    let file_name = dst
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file")
        .to_string();
    let chunks = bytes.len().div_ceil(SNAPSHOT_CHUNK_SIZE);

    for (index, chunk) in bytes.chunks(SNAPSHOT_CHUNK_SIZE).enumerate() {
        let chunk_path = dst.with_file_name(format!("{file_name}.part{:04}", index + 1));
        fs::write(&chunk_path, chunk).with_context(|| {
            format!(
                "Failed to write snapshot chunk: {} -> {}",
                src.display(),
                chunk_path.display()
            )
        })?;
    }

    let manifest_path = dst.with_file_name(format!("{file_name}.snapshot.json"));
    let manifest = serde_json::json!({
        "original_name": file_name,
        "original_size": bytes.len(),
        "chunk_size": SNAPSHOT_CHUNK_SIZE,
        "chunks": chunks,
        "note": "Large file stored as snapshot chunks to stay below GitHub's per-file size limit.",
    });
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?).with_context(|| {
        format!(
            "Failed to write snapshot manifest: {}",
            manifest_path.display()
        )
    })?;

    Ok(())
}

fn prune_stale_snapshot_children(root: &Path, expected: &[PathBuf]) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(root)
        .with_context(|| format!("Failed to read snapshot directory: {}", root.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !expected.iter().any(|expected_path| expected_path == &path) {
            fs::remove_dir_all(&path).with_context(|| {
                format!(
                    "Failed to remove stale snapshot directory: {}",
                    path.display()
                )
            })?;
        }
    }

    Ok(())
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

    #[test]
    fn test_rebuild_cache_skips_unhoused_directory() {
        let root = std::env::temp_dir().join("spocket_registry_unhoused_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("unhoused").join("moved123")).unwrap();

        let manifest = Manifest::new("manifest_hash".to_string(), vec![PathBuf::from("/tmp/p")]);
        manifest
            .save(&root.join("unhoused").join("moved123"))
            .unwrap();

        let cache = rebuild_cache_from(&root).unwrap();
        assert!(cache.pockets.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_sync_registry_snapshot_creates_git_safe_copies() {
        let root = std::env::temp_dir().join("spocket_registry_snapshot_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let pocket_dir = root.join("abc123");
        fs::create_dir_all(pocket_dir.join(".git")).unwrap();
        fs::write(pocket_dir.join("file.txt"), "hello").unwrap();

        let count = sync_registry_snapshot_from(&root).unwrap();
        assert_eq!(count, 1);

        let snapshot = root.join("snapshots").join("pockets").join("abc123");
        assert!(snapshot.join("file.txt").is_file());
        assert!(!snapshot.join(".git").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_snapshot_large_file_is_chunked() {
        let root = std::env::temp_dir().join("spocket_registry_snapshot_large_file_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let pocket_dir = root.join("abc123");
        fs::create_dir_all(&pocket_dir).unwrap();
        let target = pocket_dir.join("large.bin");
        let bytes = vec![7u8; SNAPSHOT_CHUNK_SIZE + 32];
        fs::write(&target, bytes).unwrap();

        let count = sync_registry_snapshot_from(&root).unwrap();
        assert_eq!(count, 1);

        let snapshot_dir = root.join("snapshots").join("pockets").join("abc123");
        assert!(!snapshot_dir.join("large.bin").exists());
        assert!(snapshot_dir.join("large.bin.part0001").is_file());
        assert!(snapshot_dir.join("large.bin.part0002").is_file());
        assert!(snapshot_dir.join("large.bin.snapshot.json").is_file());

        let _ = fs::remove_dir_all(&root);
    }
}
