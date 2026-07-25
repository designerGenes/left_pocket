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

/// Retained backup trees written *inside* a registry root.
///
/// `upgrade-backups` is produced by
/// `upgrade-installation --clean-literal-root-artifacts`;
/// `real-world-test-backups` is produced by `corner tests -i`. Both hold
/// verbatim copies of live corner content, so every scanner that walks a
/// registry root must treat them as opaque: they are not corners, they must not
/// be text-rewritten by migrations, and artifact audits must not re-report the
/// copies they contain. A backup that a later command silently edits is not a
/// backup.
pub const UPGRADE_BACKUPS_DIR: &str = "upgrade-backups";
pub const REAL_WORLD_TEST_BACKUPS_DIR: &str = "real-world-test-backups";
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
    // Caches written before the pocket->corner rename used the "pockets" key;
    // accept it so an existing registry_cache.json still parses.
    #[serde(default, alias = "pockets")]
    pub corners: Vec<RegistryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Stable corner id, derived from the directory name under `~/.safe_pocket`.
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
            corners: Vec::new(),
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
    fs::create_dir_all(&dir).context("Failed to create corner registry directory")?;
    ensure_registry_git_state_from(&dir)?;
    Ok(dir)
}

pub fn current_registry_dir() -> Result<PathBuf> {
    let dir = crate::branding::current_registry_root()?;
    fs::create_dir_all(&dir).context("Failed to create primary corner registry directory")?;
    ensure_registry_git_state_from(&dir)?;
    Ok(dir)
}

pub fn rebuild_current_cache() -> Result<RegistryCache> {
    let root = current_registry_dir()?;
    rebuild_cache_from(&root)
}

pub fn sync_registry_git_state() -> Result<usize> {
    let root = registry_dir()?;
    ensure_registry_git_state_from(&root)?;
    sync_registry_snapshot_from(&root)
}

pub fn temporary_registry_dir() -> Result<PathBuf> {
    let dir = registry_dir()?.join(TEMPORARY_DIR);
    fs::create_dir_all(&dir).context("Failed to create temporary corner directory")?;
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

pub fn is_registry_corner_dir(corner_dir: &Path) -> Result<bool> {
    let parent = match corner_dir.parent() {
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
    let preferred_root = crate::branding::preferred_registry_root()?;
    let mut merged = RegistryCache::default();
    let mut seen_paths = HashSet::new();
    let mut any_existing_root = false;

    for root in crate::branding::known_registry_roots()? {
        if !root.exists() {
            continue;
        }

        any_existing_root = true;
        let cache = load_cache_or_rebuild_from(&root)?;
        for entry in cache.corners {
            if seen_paths.insert(entry.path.clone()) {
                merged.corners.push(entry);
            }
        }
    }

    if !any_existing_root {
        return load_cache_or_rebuild_from(&write_root);
    }

    // Split-brain protection: when the same corner hash (directory name) exists
    // in multiple registry roots (e.g. ~/.corner AND ~/.safe_pocket), pick the
    // canonical one instead of letting both entries survive. Without this, a
    // stale entry in the preferred root can shadow a fresh entry in a legacy
    // root and break `corner locate` / `corner -i` for the project.
    merged.corners = dedupe_corners(merged.corners, &preferred_root);

    Ok(merged)
}

/// Pick a single canonical entry per corner hash (directory name).
///
/// When two entries share a hash but live in different roots, refresh each
/// from disk and choose the survivor in this priority order:
///   1. The entry whose on-disk `birth_hash` matches the directory name
///      (i.e. the corner that "owns" this hash).
///   2. The entry whose cached `manifest_hash` still matches the on-disk
///      manifest's `hash` (i.e. the cache is fresh for this entry).
///   3. The entry that lives in the preferred registry root.
///   4. The first entry.
///
/// Entries that cannot be refreshed from disk (manifest missing) are dropped
/// in favour of any sibling that still has a manifest. If every sibling is
/// broken, the first entry is kept as-is so the caller can still see it.
fn dedupe_corners(corners: Vec<RegistryEntry>, preferred_root: &Path) -> Vec<RegistryEntry> {
    let mut by_hash: std::collections::HashMap<String, Vec<RegistryEntry>> =
        std::collections::HashMap::new();
    for entry in corners {
        by_hash.entry(entry.hash.clone()).or_default().push(entry);
    }

    let mut result = Vec::new();
    for (_, group) in by_hash {
        if group.len() == 1 {
            result.extend(group);
            continue;
        }

        let refreshed: Vec<Option<RegistryEntry>> =
            group.iter().map(refresh_entry_from_disk).collect();

        let survivor = pick_dedupe_survivor(&group, &refreshed, preferred_root);
        if let Some(survivor) = survivor {
            result.push(survivor);
        }
    }

    sort_entries(&mut result);
    result
}

fn pick_dedupe_survivor(
    group: &[RegistryEntry],
    refreshed: &[Option<RegistryEntry>],
    preferred_root: &Path,
) -> Option<RegistryEntry> {
    // 1. Most user content. When the same corner hash lives in multiple roots,
    //    the corner with more real user content (FEATURES, observations, tools,
    //    etc.) is almost certainly the "real" working corner. A freshly created
    //    accidental duplicate will have near-empty content directories. This is
    //    the primary signal — birth_hash matching was tried and rejected because
    //    it picks the freshly created corner over the migrated one with history.
    let mut best_content: Option<(usize, RegistryEntry)> = None;
    for fresh in refreshed.iter().flatten() {
        let count = count_corner_content(&fresh.path);
        match &best_content {
            None => best_content = Some((count, fresh.clone())),
            Some((prev_count, _)) => {
                if count > *prev_count {
                    best_content = Some((count, fresh.clone()));
                }
            }
        }
    }
    if let Some((_, survivor)) = best_content {
        return Some(survivor);
    }

    // 2. Lives in the preferred root.
    for fresh in refreshed.iter().flatten() {
        if fresh.path.starts_with(preferred_root) {
            return Some(fresh.clone());
        }
    }

    // 3. Any refreshable entry, then the raw cache entry.
    if let Some(fresh) = refreshed.iter().flatten().next() {
        return Some(fresh.clone());
    }
    group.first().cloned()
}

/// Count user-content files in a corner directory.
///
/// Walks the corner recursively but EXCLUDES:
///   - `.git` directories (version control internals, not user content)
///   - `.opencode` directories (generated agent/plugin files, not user content)
///   - Directories that contain a `manifest.json` (nested corner copies)
///   - `.DS_Store` files (macOS filesystem metadata)
///
/// This is the split-brain tiebreaker: the corner with more user content
/// (FEATURES, observations, tools, graphify-out, etc.) is the "real" working
/// corner, while an accidentally created duplicate will have near-empty
/// content directories.
///
/// Capped at `CONTENT_COUNT_CAP` files to avoid pathological cases; once the
/// cap is reached the exact count doesn't matter because both corners are
/// "large" and the preferred-root / refreshable fallbacks take over.
const CONTENT_COUNT_CAP: usize = 10_000;
fn count_corner_content(corner_dir: &Path) -> usize {
    let mut count = 0usize;
    count_corner_content_recursive(corner_dir, &mut count);
    count
}

fn count_corner_content_recursive(dir: &Path, count: &mut usize) {
    if *count >= CONTENT_COUNT_CAP {
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        if *count >= CONTENT_COUNT_CAP {
            return;
        }

        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip non-user-content directories.
            if name == ".git" || name == ".opencode" {
                continue;
            }
            // Skip nested corner copies: a directory that contains a
            // manifest.json is itself a corner, not user content.
            if path.join("manifest.json").exists() {
                continue;
            }
            count_corner_content_recursive(&path, count);
        } else if file_type.is_file() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == ".DS_Store" {
                continue;
            }
            *count += 1;
        }
        // Symlinks are ignored — they don't represent local content.
    }
}

/// Refresh a registry entry by reloading its manifest from disk.
///
/// Returns `None` when the manifest is missing or unreadable. The returned
/// entry always reflects the on-disk truth (current `hash`, `core_paths`,
/// `birth_hash`, etc.) rather than the possibly-stale cache fields.
pub fn refresh_entry_from_disk(entry: &RegistryEntry) -> Option<RegistryEntry> {
    let manifest = Manifest::load_without_registry_update(&entry.path).ok()??;
    Some(entry_from_manifest(&entry.path, &manifest))
}

/// Scan every known registry root on disk and build a list of fresh entries.
///
/// Unlike `load_cache_or_rebuild`, this ignores the on-disk cache files and
/// reads each corner's manifest directly. Used as a fallback when the cache
/// is stale and a lookup misses.
pub fn scan_all_roots_for_entries() -> Result<Vec<RegistryEntry>> {
    let mut entries = Vec::new();
    let preferred_root = crate::branding::preferred_registry_root()?;

    for root in crate::branding::known_registry_roots()? {
        if !root.exists() {
            continue;
        }
        collect_fresh_entries(&root, &mut entries)?;
        let temporary_root = root.join(TEMPORARY_DIR);
        if temporary_root.exists() {
            collect_fresh_entries(&temporary_root, &mut entries)?;
        }
    }

    entries = dedupe_corners(entries, &preferred_root);
    Ok(entries)
}

fn collect_fresh_entries(root: &Path, entries: &mut Vec<RegistryEntry>) -> Result<()> {
    for entry in fs::read_dir(root).context("Failed to read corner registry directory")? {
        let entry = entry?;
        let corner_dir = entry.path();

        if !corner_dir.is_dir() || is_reserved_registry_dir(&corner_dir) {
            continue;
        }

        let dir_name = match corner_dir.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        let manifest = match Manifest::load_without_registry_update(&corner_dir)? {
            Some(manifest) => manifest,
            None if has_workspace_file(&corner_dir, &dir_name) => {
                Manifest::backfill_without_registry_update(&corner_dir, &dir_name)?
            }
            None => continue,
        };

        entries.push(entry_from_manifest(&corner_dir, &manifest));
    }

    Ok(())
}

/// Force-rebuild the registry cache in every known registry root.
///
/// Used by `corner sync-registry` to recover from stale or split-brain cache
/// state. Returns the total number of corners written across all roots.
pub fn rebuild_all_caches() -> Result<usize> {
    let mut total = 0;
    let known_roots = crate::branding::known_registry_roots()?;

    for root in &known_roots {
        if !root.exists() {
            continue;
        }
        let mut cache = RegistryCache::default();
        collect_corners_from_dir(root, &mut cache)?;

        let temporary_root = root.join(TEMPORARY_DIR);
        if temporary_root.exists() {
            collect_corners_from_dir(&temporary_root, &mut cache)?;
        }

        // Drop split-brain duplicates so the rebuilt cache only carries one
        // entry per corner hash.
        let preferred_root = crate::branding::preferred_registry_root()?;
        cache.corners = dedupe_corners(cache.corners, &preferred_root);
        total += cache.corners.len();

        sort_entries(&mut cache.corners);
        cache.generated_at = Utc::now();
        write_cache_to(root, &cache)?;
    }

    Ok(total)
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

    let before = cache.corners.len();
    cache.corners.retain(|entry| entry.path.is_dir());
    sort_entries(&mut cache.corners);

    // A whole-root rename (for example ~/.safe_pocket -> ~/.corner) can leave a
    // cache file in place whose entries all point at paths that no longer
    // exist. If the root still contains corner directories, rebuild instead of
    // persisting an empty cache.
    if cache.corners.is_empty() && before > 0 && root_has_corner_dirs(root)? {
        return rebuild_cache_from(root);
    }

    if cache.corners.len() != before {
        cache.generated_at = Utc::now();
        write_cache_to(root, &cache)?;
    }

    Ok(cache)
}

fn rebuild_cache_from(root: &Path) -> Result<RegistryCache> {
    fs::create_dir_all(root).context("Failed to create corner registry directory")?;

    let mut cache = RegistryCache::default();
    collect_corners_from_dir(root, &mut cache)?;

    let temporary_root = root.join(TEMPORARY_DIR);
    if temporary_root.exists() {
        collect_corners_from_dir(&temporary_root, &mut cache)?;
    }

    sort_entries(&mut cache.corners);
    write_cache_to(root, &cache)?;
    Ok(cache)
}

pub fn upsert_corner(corner_dir: &Path, manifest: &Manifest) -> Result<()> {
    if !is_registry_corner_dir(corner_dir)? {
        return Ok(());
    }

    let root = registry_dir()?;
    ensure_registry_git_state_from(&root)?;
    let mut cache = load_cache_or_rebuild_from(&root)?;
    let entry = entry_from_manifest(corner_dir, manifest);
    let entry_hash = entry.hash.clone();

    cache
        .corners
        .retain(|existing| existing.path != entry.path && existing.hash != entry.hash);
    cache.corners.push(entry);
    sort_entries(&mut cache.corners);
    cache.generated_at = Utc::now();

    write_cache_to(&root, &cache)?;

    // Split-brain protection: when this corner's hash (directory name) also
    // appears in another registry root's cache, remove the stale sibling so a
    // future `load_cache_or_rebuild` cannot pick the wrong entry. This is the
    // root cause of the "lost connection" bug after a rename or augment: the
    // preferred root's cache was updated, but a legacy root still carried the
    // old entry, and lookups fell back to it.
    prune_duplicate_entries_from_other_roots(&root, &entry_hash)?;
    let _ = sync_registry_snapshot_from(&root);
    Ok(())
}

/// Remove entries with `hash` from every registry root's cache except `keep_root`.
///
/// Best-effort: if a root's cache cannot be read or written, the error is
/// swallowed so a single broken root cannot block the primary upsert.
fn prune_duplicate_entries_from_other_roots(keep_root: &Path, hash: &str) -> Result<()> {
    for root in crate::branding::known_registry_roots()? {
        if root == keep_root || !root.exists() {
            continue;
        }

        let cache_path = cache_path_for(&root);
        if !cache_path.exists() {
            continue;
        }

        let mut cache = match read_cache_from(&root) {
            Ok(cache) => cache,
            Err(_) => continue,
        };

        let before = cache.corners.len();
        cache.corners.retain(|entry| entry.hash != hash);
        if cache.corners.len() == before {
            continue;
        }

        cache.generated_at = Utc::now();
        let _ = write_cache_to(&root, &cache);
    }

    Ok(())
}

pub fn remove_corner(corner_dir: &Path) -> Result<()> {
    if !is_registry_corner_dir(corner_dir)? {
        return Ok(());
    }

    let root = registry_dir()?;
    ensure_registry_git_state_from(&root)?;
    let mut cache = load_cache_or_rebuild_from(&root)?;
    let before = cache.corners.len();
    cache.corners.retain(|entry| entry.path != corner_dir);

    if cache.corners.len() != before {
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
                "Refusing to move path outside known corner roots: {}",
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
            "Failed to move corner content to unhoused: {} -> {}",
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
    fs::create_dir_all(root).context("Failed to create corner registry directory")?;
    let content =
        serde_json::to_string_pretty(cache).context("Failed to serialize registry cache")?;
    let tmp_path = cache_tmp_path_for(root);
    fs::write(&tmp_path, content).context("Failed to write registry cache tmp file")?;
    fs::rename(&tmp_path, cache_path_for(root)).context("Failed to rename registry cache")?;
    Ok(())
}

fn entry_from_manifest(corner_dir: &Path, manifest: &Manifest) -> RegistryEntry {
    let hash = corner_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&manifest.hash)
        .to_string();

    RegistryEntry {
        hash,
        manifest_hash: manifest.hash.clone(),
        path: corner_dir.to_path_buf(),
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

fn collect_corners_from_dir(root: &Path, cache: &mut RegistryCache) -> Result<()> {
    for entry in fs::read_dir(root).context("Failed to read corner registry directory")? {
        let entry = entry?;
        let corner_dir = entry.path();

        if !corner_dir.is_dir() || is_reserved_registry_dir(&corner_dir) {
            continue;
        }

        let dir_name = match corner_dir.file_name().and_then(|n| n.to_str()) {
            Some(name) => name.to_string(),
            None => continue,
        };

        let manifest = match Manifest::load_without_registry_update(&corner_dir)? {
            Some(manifest) => manifest,
            None if has_workspace_file(&corner_dir, &dir_name) => {
                Manifest::backfill_without_registry_update(&corner_dir, &dir_name)?
            }
            None => continue,
        };

        cache
            .corners
            .push(entry_from_manifest(&corner_dir, &manifest));
    }

    Ok(())
}

/// Directory names inside a registry root that are never corners.
///
/// A registry root hosts more than corners: task storage (`global_data`), the
/// root git repository (`.git`), archived corners (`snapshots`, `unhoused`),
/// the temporary corner area, and the retained backup trees above. This is the
/// single authoritative list — `corner locate --read-only` and every other
/// registry walker share it, so adding an operational directory here cannot
/// leave one code path treating it as a corner.
pub const RESERVED_REGISTRY_DIRS: &[&str] = &[
    ".git",
    "global_data",
    "observations",
    "registry",
    "unhoused",
    SNAPSHOTS_DIR,
    TEMPORARY_DIR,
    UPGRADE_BACKUPS_DIR,
    REAL_WORLD_TEST_BACKUPS_DIR,
];

/// True when `name` is a reserved registry directory rather than a corner.
pub fn is_reserved_registry_name(name: &str) -> bool {
    RESERVED_REGISTRY_DIRS.contains(&name)
}

fn is_reserved_registry_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(is_reserved_registry_name)
        .unwrap_or(false)
}

fn root_has_corner_dirs(root: &Path) -> Result<bool> {
    for entry in fs::read_dir(root).context("Failed to read corner registry directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() && !is_reserved_registry_dir(&path) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ensure_registry_git_state_from(root: &Path) -> Result<()> {
    fs::create_dir_all(root).context("Failed to create corner registry root")?;

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
    let corners_root = snapshots_root.join("corners");
    let temporary_root = snapshots_root.join(TEMPORARY_DIR);
    // Snapshot trees written before the pocket->corner rename live under
    // snapshots/pockets; drop the stale tree so only corners/ remains.
    let legacy_snapshots = snapshots_root.join("pockets");
    if legacy_snapshots.exists() {
        let _ = fs::remove_dir_all(&legacy_snapshots);
    }
    fs::create_dir_all(&corners_root).with_context(|| {
        format!(
            "Failed to create snapshots directory: {}",
            corners_root.display()
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

    for entry in fs::read_dir(root).context("Failed to read corner registry root")? {
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
        let target = corners_root.join(name);
        mirror_snapshot_dir(&path, &target)?;
        expected.push(target);
        count += 1;
    }

    let temporary_source = root.join(TEMPORARY_DIR);
    if temporary_source.exists() {
        for entry in fs::read_dir(&temporary_source).with_context(|| {
            format!(
                "Failed to read temporary corners: {}",
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

    prune_stale_snapshot_children(&corners_root, &expected)?;
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
                "Failed to copy corner content into snapshot: {} -> {}",
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

fn has_workspace_file(corner_dir: &Path, hash: &str) -> bool {
    if corner_dir.join(format!("{}.code-workspace", hash)).exists() {
        return true;
    }

    fs::read_dir(corner_dir)
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

        let corner_dir = root.join("abc123");
        fs::create_dir_all(&corner_dir).unwrap();
        let mut manifest = Manifest::new("manifest_hash".to_string(), vec![PathBuf::from("/p")]);
        manifest.worktrees.push(PathBuf::from("/p-worktree"));
        manifest.save(&corner_dir).unwrap();

        let cache = rebuild_cache_from(&root).unwrap();

        assert_eq!(cache.corners.len(), 1);
        assert_eq!(cache.corners[0].hash, "abc123");
        assert_eq!(cache.corners[0].manifest_hash, "manifest_hash");
        assert_eq!(cache.corners[0].path, corner_dir);
        assert_eq!(cache.corners[0].core_paths, vec![PathBuf::from("/p")]);
        assert_eq!(
            cache.corners[0].worktrees,
            vec![PathBuf::from("/p-worktree")]
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_cache_load_prunes_deleted_corners() {
        let root = std::env::temp_dir().join("spocket_registry_prune_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let cache = RegistryCache {
            corners: vec![RegistryEntry {
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
        assert!(loaded.corners.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_rebuild_cache_includes_temporary_corners() {
        let root = std::env::temp_dir().join("spocket_registry_temporary_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("temporary")).unwrap();

        let corner_dir = root.join("temporary").join("temp123");
        fs::create_dir_all(&corner_dir).unwrap();
        let manifest = Manifest::new_with_options(
            "manifest_hash".to_string(),
            vec![PathBuf::from("/tmp/project")],
            true,
        );
        manifest.save(&corner_dir).unwrap();

        let cache = rebuild_cache_from(&root).unwrap();
        assert_eq!(cache.corners.len(), 1);
        assert!(cache.corners[0].temporary);
        assert_eq!(cache.corners[0].path, corner_dir);

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
        assert!(cache.corners.is_empty());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_sync_registry_snapshot_creates_git_safe_copies() {
        let root = std::env::temp_dir().join("spocket_registry_snapshot_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let corner_dir = root.join("abc123");
        fs::create_dir_all(corner_dir.join(".git")).unwrap();
        fs::write(corner_dir.join("file.txt"), "hello").unwrap();

        let count = sync_registry_snapshot_from(&root).unwrap();
        assert_eq!(count, 1);

        let snapshot = root.join("snapshots").join("corners").join("abc123");
        assert!(snapshot.join("file.txt").is_file());
        assert!(!snapshot.join(".git").exists());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn test_snapshot_large_file_is_chunked() {
        let root = std::env::temp_dir().join("spocket_registry_snapshot_large_file_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let corner_dir = root.join("abc123");
        fs::create_dir_all(&corner_dir).unwrap();
        let target = corner_dir.join("large.bin");
        let bytes = vec![7u8; SNAPSHOT_CHUNK_SIZE + 32];
        fs::write(&target, bytes).unwrap();

        let count = sync_registry_snapshot_from(&root).unwrap();
        assert_eq!(count, 1);

        let snapshot_dir = root.join("snapshots").join("corners").join("abc123");
        assert!(!snapshot_dir.join("large.bin").exists());
        assert!(snapshot_dir.join("large.bin.part0001").is_file());
        assert!(snapshot_dir.join("large.bin.part0002").is_file());
        assert!(snapshot_dir.join("large.bin.snapshot.json").is_file());

        let _ = fs::remove_dir_all(&root);
    }

    fn fresh_entry(hash: &str, path: PathBuf, manifest_hash: &str, birth_hash: Option<&str>) -> RegistryEntry {
        RegistryEntry {
            hash: hash.to_string(),
            manifest_hash: manifest_hash.to_string(),
            path,
            created_at: Utc::now(),
            temporary: false,
            core_paths: Vec::new(),
            worktrees: Vec::new(),
            parent_hash: None,
            children: Vec::new(),
            augmented_from: None,
            birth_hash: birth_hash.map(str::to_string),
            manifest_version: 1,
        }
    }

    fn write_manifest_at(corner_dir: &Path, hash: &str, birth_hash: Option<&str>) {
        fs::create_dir_all(corner_dir).unwrap();
        let mut json = serde_json::json!({
            "hash": hash,
            "core_paths": Vec::<String>::new(),
            "created_at": "2026-07-18T16:36:18.181290443Z",
            "temporary": false,
            "children": Vec::<String>::new(),
            "version": 1,
        });
        if let Some(bh) = birth_hash {
            json["birth_hash"] = serde_json::json!(bh);
        }
        fs::write(corner_dir.join("manifest.json"), serde_json::to_string_pretty(&json).unwrap())
            .unwrap();
    }

    #[test]
    fn dedupe_corners_keeps_single_entries_unchanged() {
        let root = std::env::temp_dir().join("spocket_dedupe_single_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let a = root.join("aaaa11111111");
        let b = root.join("bbbb22222222");
        write_manifest_at(&a, "aaaa11111111", None);
        write_manifest_at(&b, "bbbb22222222", None);

        let entries = vec![
            fresh_entry("aaaa11111111", a.clone(), "aaaa11111111", None),
            fresh_entry("bbbb22222222", b.clone(), "bbbb22222222", None),
        ];
        let result = dedupe_corners(entries, &root);
        assert_eq!(result.len(), 2);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn dedupe_corners_prefers_more_user_content() {
        // Two corners with the same dir-name hash in different roots.
        // The one with more user content (FEATURES, observations, etc.)
        // should win, even if the other one's birth_hash matches the dir
        // name. This is the real-world split-brain: an old migrated corner
        // with lots of history vs. a freshly created accidental duplicate.
        let primary_root = std::env::temp_dir().join("spocket_dedupe_content_primary");
        let legacy_root = std::env::temp_dir().join("spocket_dedupe_content_legacy");
        let _ = fs::remove_dir_all(&primary_root);
        let _ = fs::remove_dir_all(&legacy_root);
        fs::create_dir_all(&primary_root).unwrap();
        fs::create_dir_all(&legacy_root).unwrap();

        let hash = "sharedhash000";
        let primary_corner = primary_root.join(hash);
        let legacy_corner = legacy_root.join(hash);

        // Primary corner: lots of user content, birth_hash does NOT match.
        write_manifest_at(&primary_corner, "primaryhm000", Some("other00000000"));
        fs::create_dir_all(primary_corner.join("FEATURES/dailies")).unwrap();
        for i in 0..10 {
            fs::write(
                primary_corner.join("FEATURES/dailies").join(format!("day_{i}.md")),
                "content",
            )
            .unwrap();
        }
        fs::create_dir_all(primary_corner.join("observations")).unwrap();
        fs::write(primary_corner.join("observations").join("note.md"), "content").unwrap();

        // Legacy corner: near-empty, but birth_hash MATCHES the dir name.
        write_manifest_at(&legacy_corner, "legacyhash00", Some(hash));
        fs::create_dir_all(legacy_corner.join("FEATURES")).unwrap();
        fs::write(legacy_corner.join("FEATURES/00.md"), "minimal").unwrap();

        let entries = vec![
            fresh_entry(hash, primary_corner.clone(), "primaryhm000", Some("other00000000")),
            fresh_entry(hash, legacy_corner.clone(), "legacyhash00", Some(hash)),
        ];

        let result = dedupe_corners(entries, &primary_root);
        assert_eq!(result.len(), 1, "expected duplicate to collapse to one entry");
        assert_eq!(
            result[0].path, primary_corner,
            "expected corner with more user content to win, got {}",
            result[0].path.display()
        );

        let _ = fs::remove_dir_all(&primary_root);
        let _ = fs::remove_dir_all(&legacy_root);
    }

    #[test]
    fn dedupe_corners_excludes_opencode_and_nested_corners_from_content_count() {
        // .opencode/ has thousands of generated files in both corners, so it
        // must NOT count toward the content total. A nested corner directory
        // (a dir containing manifest.json) must also be excluded.
        let primary_root = std::env::temp_dir().join("spocket_dedupe_excl_primary");
        let legacy_root = std::env::temp_dir().join("spocket_dedupe_excl_legacy");
        let _ = fs::remove_dir_all(&primary_root);
        let _ = fs::remove_dir_all(&legacy_root);
        fs::create_dir_all(&primary_root).unwrap();
        fs::create_dir_all(&legacy_root).unwrap();

        let hash = "sharedhash001";
        let primary_corner = primary_root.join(hash);
        let legacy_corner = legacy_root.join(hash);

        // Primary corner: 2 real user files + 100 .opencode files.
        write_manifest_at(&primary_corner, "primaryhm001", Some(hash));
        fs::create_dir_all(primary_corner.join("FEATURES")).unwrap();
        fs::write(primary_corner.join("FEATURES/real.md"), "content").unwrap();
        fs::write(primary_corner.join("README.md"), "content").unwrap();
        fs::create_dir_all(primary_corner.join(".opencode")).unwrap();
        for i in 0..100 {
            fs::write(primary_corner.join(".opencode").join(format!("agent_{i}.md")), "x").unwrap();
        }

        // Legacy corner: 1 real user file + a nested corner copy with 50 files.
        write_manifest_at(&legacy_corner, "legacyhash01", Some("other00000001"));
        fs::create_dir_all(legacy_corner.join("FEATURES")).unwrap();
        fs::write(legacy_corner.join("FEATURES/real.md"), "content").unwrap();
        // Nested corner copy — must be excluded from the count.
        let nested = legacy_corner.join("nestedcorner");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("manifest.json"), "{}").unwrap();
        for i in 0..50 {
            fs::write(nested.join(format!("file_{i}.md")), "x").unwrap();
        }

        // Primary has 2 real files; legacy has 1 real file (the 50 nested
        // files don't count). Primary should win.
        let entries = vec![
            fresh_entry(hash, primary_corner.clone(), "primaryhm001", Some(hash)),
            fresh_entry(hash, legacy_corner.clone(), "legacyhash01", Some("other00000001")),
        ];

        let result = dedupe_corners(entries, &primary_root);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].path, primary_corner,
            "expected corner with more real user content (excluding .opencode and nested corners) to win"
        );

        let _ = fs::remove_dir_all(&primary_root);
        let _ = fs::remove_dir_all(&legacy_root);
    }

    #[test]
    fn dedupe_corners_falls_back_to_preferred_root_when_content_is_equal() {
        let primary_root = std::env::temp_dir().join("spocket_dedupe_pref_primary");
        let legacy_root = std::env::temp_dir().join("spocket_dedupe_pref_legacy");
        let _ = fs::remove_dir_all(&primary_root);
        let _ = fs::remove_dir_all(&legacy_root);
        fs::create_dir_all(&primary_root).unwrap();
        fs::create_dir_all(&legacy_root).unwrap();

        let hash = "sharedhash002";
        let primary_corner = primary_root.join(hash);
        let legacy_corner = legacy_root.join(hash);

        // Both corners have the same amount of content.
        write_manifest_at(&primary_corner, "primaryhm002", Some("other00000002"));
        write_manifest_at(&legacy_corner, "legacyhash02", Some("other00000003"));
        fs::create_dir_all(primary_corner.join("FEATURES")).unwrap();
        fs::write(primary_corner.join("FEATURES/00.md"), "x").unwrap();
        fs::create_dir_all(legacy_corner.join("FEATURES")).unwrap();
        fs::write(legacy_corner.join("FEATURES/00.md"), "x").unwrap();

        let entries = vec![
            fresh_entry(hash, primary_corner.clone(), "primaryhm002", Some("other00000002")),
            fresh_entry(hash, legacy_corner.clone(), "legacyhash02", Some("other00000003")),
        ];

        let result = dedupe_corners(entries, &primary_root);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].path, primary_corner, "expected preferred root to win when content is equal");

        let _ = fs::remove_dir_all(&primary_root);
        let _ = fs::remove_dir_all(&legacy_root);
    }

    #[test]
    fn refresh_entry_from_disk_returns_none_for_missing_manifest() {
        let root = std::env::temp_dir().join("spocket_refresh_missing_test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let entry = fresh_entry("abc", root.join("abc"), "abc", None);
        let refreshed = refresh_entry_from_disk(&entry);
        assert!(refreshed.is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refresh_entry_from_disk_returns_fresh_fields() {
        let root = std::env::temp_dir().join("spocket_refresh_fresh_test");
        let _ = fs::remove_dir_all(&root);
        let corner_dir = root.join("abc123");
        write_manifest_at(&corner_dir, "newhash000000", Some("abc123"));

        // Stale cache entry that doesn't match the on-disk manifest.
        let stale = fresh_entry("abc123", corner_dir.clone(), "oldhash000000", None);
        let refreshed = refresh_entry_from_disk(&stale).expect("manifest exists");
        assert_eq!(refreshed.hash, "abc123");
        assert_eq!(refreshed.manifest_hash, "newhash000000");
        assert_eq!(refreshed.birth_hash.as_deref(), Some("abc123"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn count_corner_content_counts_user_files() {
        let root = std::env::temp_dir().join("spocket_count_content_test");
        let _ = fs::remove_dir_all(&root);
        let corner = root.join("abc123");
        fs::create_dir_all(corner.join("FEATURES/dailies")).unwrap();
        fs::create_dir_all(corner.join("observations")).unwrap();
        fs::create_dir_all(corner.join(".opencode")).unwrap();
        fs::write(corner.join("FEATURES/dailies/day1.md"), "x").unwrap();
        fs::write(corner.join("FEATURES/dailies/day2.md"), "x").unwrap();
        fs::write(corner.join("observations/note.md"), "x").unwrap();
        fs::write(corner.join("README.md"), "x").unwrap();
        // .opencode files must NOT count.
        fs::write(corner.join(".opencode/agent.md"), "x").unwrap();
        // .DS_Store must NOT count.
        fs::write(corner.join(".DS_Store"), "x").unwrap();

        let count = count_corner_content(&corner);
        assert_eq!(count, 4, "expected 4 user files (2 dailies + 1 observation + 1 README)");

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn count_corner_content_excludes_nested_corner_dirs() {
        let root = std::env::temp_dir().join("spocket_count_nested_test");
        let _ = fs::remove_dir_all(&root);
        let corner = root.join("abc123");
        fs::create_dir_all(corner.join("FEATURES")).unwrap();
        fs::write(corner.join("FEATURES/real.md"), "x").unwrap();

        // Nested corner — must NOT count.
        let nested = corner.join("nestedcorner");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("manifest.json"), "{}").unwrap();
        fs::write(nested.join("file1.md"), "x").unwrap();
        fs::write(nested.join("file2.md"), "x").unwrap();

        let count = count_corner_content(&corner);
        assert_eq!(count, 1, "expected only 1 real file, nested corner excluded");

        let _ = fs::remove_dir_all(&root);
    }
}
