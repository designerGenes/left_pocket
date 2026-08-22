//! `left_pocket upgrade-installation` — one-way text migration that rewrites legacy
//! Spocket/Safe_pocket references to their left_pocket equivalents across the known
//! config and registry roots.
//!
//! This is **not** a template sync. It performs in-place text rewrites on files
//! that already live on disk (placed templates inside left_pockets, config assets,
//! AGENTS.md, copilot-instructions.md, etc.). It never copies content from a
//! project back into the config templates directory — it only rewrites tokens
//! inside whatever files it visits.
//!
//! The structural tokens it rewrites are the directive/marker prefixes. User-
//! facing feature-tag names (e.g. `SPOCKET_MUST_INSTALL`) are intentionally left
//! alone: those are defined in `feature_tags.yaml` and referenced inside feature
//! files, so renaming them in one place without the other would break existing
//! feature files. A future `--include-feature-tags` flag may handle that.

use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

/// Token rewrites applied to every visited file. Order matters: longer/more
/// specific tokens are listed first so they win when one is a prefix of another.
/// Both the Corner-era (`#CORNER_*`) and Spocket-era (`#SPOCKET_*`) forms are
/// rewritten to the current left_pocket forms.
const TOKEN_REPLACEMENTS: &[(&str, &str)] = &[
    (
        "#LEFT_POCKET_RUNTIME_CONTENT_START",
        "#LEFT_POCKET_RUNTIME_CONTENT_START",
    ),
    ("#LEFT_POCKET_RUNTIME_CONTENT_END", "#LEFT_POCKET_RUNTIME_CONTENT_END"),
    (
        "#LEFT_POCKET_TEMPLATE_DESTINATION",
        "#LEFT_POCKET_TEMPLATE_DESTINATION",
    ),
    ("#LEFT_POCKET_QUIET_MERGE", "#LEFT_POCKET_QUIET_MERGE"),
    ("#LEFT_POCKET_MERGE_AT_RUNTIME", "#LEFT_POCKET_MERGE_AT_RUNTIME"),
    ("#LEFT_POCKET_INSTALL_DESTINATION", "#LEFT_POCKET_INSTALL_DESTINATION"),
    (
        "<!-- BEGIN LEFT_POCKET TASK INTEGRATION -->",
        "<!-- BEGIN LEFT_POCKET TASK INTEGRATION -->",
    ),
    (
        "<!-- END LEFT_POCKET TASK INTEGRATION -->",
        "<!-- END LEFT_POCKET TASK INTEGRATION -->",
    ),
    (
        "#LEFT_POCKET_RUNTIME_CONTENT_START",
        "#LEFT_POCKET_RUNTIME_CONTENT_START",
    ),
    (
        "#LEFT_POCKET_RUNTIME_CONTENT_END",
        "#LEFT_POCKET_RUNTIME_CONTENT_END",
    ),
    (
        "#LEFT_POCKET_TEMPLATE_DESTINATION",
        "#LEFT_POCKET_TEMPLATE_DESTINATION",
    ),
    ("#LEFT_POCKET_QUIET_MERGE", "#LEFT_POCKET_QUIET_MERGE"),
    ("#LEFT_POCKET_MERGE_AT_RUNTIME", "#LEFT_POCKET_MERGE_AT_RUNTIME"),
    (
        "#LEFT_POCKET_INSTALL_DESTINATION",
        "#LEFT_POCKET_INSTALL_DESTINATION",
    ),
    (
        "<!-- BEGIN LEFT_POCKET TASK INTEGRATION -->",
        "<!-- BEGIN LEFT_POCKET TASK INTEGRATION -->",
    ),
    (
        "<!-- END LEFT_POCKET TASK INTEGRATION -->",
        "<!-- END LEFT_POCKET TASK INTEGRATION -->",
    ),
];

/// Directory names that are never descended into during a scan.
///
/// The two backup trees matter as much as the archive trees. `upgrade-backups`
/// is where this command quarantines artifacts and `real-world-test-backups` is
/// where `left_pocket tests -i` retains a complete pre-test copy of a left_pocket.
/// Descending into either would let an upgrade rewrite tokens *inside a backup*,
/// so restoring from it would no longer restore the original state, and would
/// make the artifact audit re-report every copied artifact — inflating the
/// reported count on each run and offering backup copies up for cleanup.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "unhoused",
    "snapshots",
    ".session-tools",
    "graphify-out",
    crate::registry::UPGRADE_BACKUPS_DIR,
    crate::registry::REAL_WORLD_TEST_BACKUPS_DIR,
];

/// Maximum file size (in bytes) we are willing to read and rewrite. Larger
/// files are almost certainly binary or generated and are skipped.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

pub struct UpgradeOptions {
    pub dry_run: bool,
    pub yes: bool,
    pub extra_roots: Vec<PathBuf>,
    pub clean_literal_root_artifacts: bool,
}

pub fn run(options: UpgradeOptions) -> Result<()> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for root in crate::branding::known_registry_roots()? {
        if root.is_dir() {
            roots.push(root);
        }
    }
    for root in crate::branding::known_config_roots()? {
        if root.is_dir() {
            roots.push(root);
        }
    }
    for root in &options.extra_roots {
        if root.is_dir() {
            roots.push(root.clone());
        }
    }
    roots.sort();
    roots.dedup();

    if roots.is_empty() {
        println!("{}", "No left_pocket/Safe_pocket roots found to scan.".dimmed());
        return Ok(());
    }

    println!(
        "{}",
        "Scanning for legacy Spocket/Safe_pocket references...".bright_white()
    );
    for root in &roots {
        println!("  {}", root.display().to_string().bright_blue());
    }
    println!();

    let mut plan: Vec<(PathBuf, usize)> = Vec::new();
    let mut artifacts: Vec<PathBuf> = Vec::new();
    for root in &roots {
        collect_rewrites(root, &mut plan)?;
        collect_literal_root_artifacts(root, &mut artifacts)?;
    }
    artifacts.sort();
    artifacts.dedup();

    if !artifacts.is_empty() {
        println!(
            "{} {} active literal config-root artifact director{} found (archives and retained backups excluded):",
            "Warning:".bright_yellow(),
            artifacts.len().to_string().bright_yellow(),
            if artifacts.len() == 1 { "y" } else { "ies" }
        );
        for artifact in &artifacts {
            println!("  {}", artifact.display().to_string().bright_blue());
        }
        if !options.clean_literal_root_artifacts {
            println!(
                "  {}",
                "Report only. Re-run with --clean-literal-root-artifacts to back up and remove safe artifacts."
                    .dimmed()
            );
        }
        println!();
    }

    if plan.is_empty() && !options.clean_literal_root_artifacts {
        println!(
            "{}",
            "No legacy references found — nothing to upgrade.".bright_green()
        );
        return Ok(());
    }

    if !plan.is_empty() {
        let total_replacements: usize = plan.iter().map(|(_, n)| *n).sum();
        println!(
            "{} {} replacement(s) across {} file(s):",
            "Found".bright_yellow(),
            total_replacements.to_string().bright_yellow(),
            plan.len().to_string().bright_yellow()
        );
        for (path, n) in &plan {
            println!(
                "  {} ({} replacement{})",
                path.display().to_string().bright_blue(),
                n.to_string().bright_yellow(),
                if *n == 1 { "" } else { "s" }
            );
        }
        println!();
    }

    if options.dry_run {
        println!("{}", "Dry run only — no files were modified.".dimmed());
        return Ok(());
    }

    if !plan.is_empty() && !options.yes {
        use std::io::{self, Write as IoWrite};
        print!("{} ", "Apply these rewrites? [y/N]:".bright_white());
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        if input != "y" && input != "yes" {
            println!("{}", "Aborted.".dimmed());
            return Ok(());
        }
    }

    let mut applied = 0usize;
    for (path, _) in &plan {
        let original = fs::read_to_string(path)
            .with_context(|| format!("Failed to read: {}", path.display()))?;
        let rewritten = apply_replacements(&original);
        if rewritten == original {
            continue;
        }
        fs::write(path, &rewritten)
            .with_context(|| format!("Failed to write: {}", path.display()))?;
        applied += 1;
    }

    println!(
        "{} {} file(s) upgraded.",
        "Upgrade complete:".bright_green(),
        applied.to_string().bright_yellow()
    );

    if options.clean_literal_root_artifacts {
        clean_literal_root_artifacts(&artifacts, options.yes)?;
    }
    println!(
        "{}",
        "Run `left_pocket -u <project>` to re-place templates from the upgraded config.".dimmed()
    );
    Ok(())
}

fn collect_literal_root_artifacts(root: &Path, artifacts: &mut Vec<PathBuf>) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            // Never follow symlinks while discovering cleanup candidates. A
            // symlink named {{SPOCKET_CONFIG_ROOT}} could otherwise point at an
            // unrelated tree outside the registry.
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "{{SPOCKET_CONFIG_ROOT}}"
                || name == "{{CORNER_CONFIG_ROOT}}"
                || name == "{{LEFT_POCKET_CONFIG_ROOT}}"
            {
                artifacts.push(path);
                continue;
            }
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            stack.push(path);
        }
    }
    Ok(())
}

fn clean_literal_root_artifacts(artifacts: &[PathBuf], yes: bool) -> Result<()> {
    let mut safe = Vec::new();
    for artifact in artifacts {
        if safe_literal_artifact(artifact)? {
            safe.push(artifact.clone());
        } else {
            println!(
                "  {} {} (unexpected contents; left untouched)",
                "Skipped:".bright_yellow(),
                artifact.display().to_string().bright_blue()
            );
        }
    }
    if safe.is_empty() {
        println!("{}", "No safe literal-root artifacts to clean.".dimmed());
        return Ok(());
    }

    println!(
        "{}",
        "The following directories contain only feature_tags.yaml and are eligible for cleanup:"
            .bright_white()
    );
    for artifact in &safe {
        println!("  {}", artifact.display().to_string().bright_blue());
    }
    if !yes {
        use std::io::{self, Write as IoWrite};
        print!(
            "{} ",
            "Type REMOVE to back them up and remove them:".bright_white()
        );
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        if input.trim() != "REMOVE" {
            println!("{}", "Artifact cleanup aborted.".dimmed());
            return Ok(());
        }
    }

    let registry = crate::branding::current_registry_root()?;
    let backup = registry.join("upgrade-backups").join(format!(
        "literal-root-artifacts-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%S%.9fZ"),
        std::process::id()
    ));
    fs::create_dir_all(&backup)?;
    let mut manifest = String::new();
    // Atomically quarantine the entire directory. This is both the backup and
    // the removal from the active left_pocket: even if feature_tags.yaml changes
    // after validation, the current directory and all current contents move
    // together. No copy-then-delete race and no remove_dir_all.
    for (index, artifact) in safe.iter().enumerate() {
        if !safe_literal_artifact(artifact)? {
            println!(
                "  {} {} (contents changed before quarantine; left untouched)",
                "Skipped:".bright_yellow(),
                artifact.display().to_string().bright_blue()
            );
            continue;
        }
        let destination = backup.join(format!("{index:04}-artifact"));
        fs::rename(artifact, &destination).with_context(|| {
            format!(
                "Failed to atomically quarantine {} to {} (nothing was removed)",
                artifact.display(),
                destination.display()
            )
        })?;
        manifest.push_str(&format!(
            "{}\t{}\n",
            artifact.display(),
            destination.display()
        ));
        println!(
            "  {} {}",
            "Quarantined:".bright_green(),
            artifact.display().to_string().bright_blue()
        );
    }
    fs::write(backup.join("MANIFEST.tsv"), manifest)?;
    println!("Backup retained at {}", backup.display());
    Ok(())
}

fn safe_literal_artifact(path: &Path) -> Result<bool> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(false),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(false);
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return Ok(false),
    };
    if entries.len() != 1 {
        return Ok(false);
    }
    let entry = &entries[0];
    let file_type = entry.file_type()?;
    Ok(entry.file_name() == "feature_tags.yaml" && file_type.is_file() && !file_type.is_symlink())
}

fn collect_rewrites(root: &Path, plan: &mut Vec<(PathBuf, usize)>) -> Result<()> {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let ft = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            if ft.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if SKIP_DIRS.contains(&name) {
                        continue;
                    }
                }
                stack.push(path);
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let size = entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
            if size > MAX_FILE_BYTES {
                continue;
            }
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let n = count_replacements(&text);
            if n > 0 {
                plan.push((path, n));
            }
        }
    }
    Ok(())
}

/// Rewrite legacy directive tokens (`#CORNER_*`, `#SPOCKET_*` and the legacy
/// task-integration markers) in every text file under `roots`, skipping the
/// standard `SKIP_DIRS` trees. Used by `left_pocket -u` so an upgraded left_pocket and
/// its project drop outdated directives in the same pass. Returns the number
/// of files modified.
pub fn rewrite_legacy_tokens_in_roots(roots: &[PathBuf]) -> Result<usize> {
    let mut plan: Vec<(PathBuf, usize)> = Vec::new();
    for root in roots {
        if root.is_dir() {
            collect_rewrites(root, &mut plan)?;
        }
    }
    plan.sort();
    plan.dedup();

    let mut applied = 0usize;
    for (path, n) in &plan {
        let Ok(original) = fs::read_to_string(path) else {
            continue;
        };
        let rewritten = apply_replacements(&original);
        if rewritten == original {
            continue;
        }
        fs::write(path, &rewritten)
            .with_context(|| format!("Failed to write: {}", path.display()))?;
        applied += 1;
        println!(
            "  {} ({} replacement{})",
            path.display().to_string().bright_blue(),
            n.to_string().bright_yellow(),
            if *n == 1 { "" } else { "s" }
        );
    }
    Ok(applied)
}

fn count_replacements(text: &str) -> usize {
    let mut total = 0;
    for (old, _) in TOKEN_REPLACEMENTS {
        total += text.matches(old).count();
    }
    total
}

fn apply_replacements(text: &str) -> String {
    let mut out = text.to_string();
    for (old, new) in TOKEN_REPLACEMENTS {
        if out.contains(old) {
            out = out.replace(old, new);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rewrite_legacy_tokens_in_roots_rewrites_corner_and_spocket_directives() {
        let base = std::env::temp_dir().join("left_pocket_test_rewrite_roots");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("left_pocket_dir/.git")).unwrap();
        fs::create_dir_all(base.join("project_dir")).unwrap();

        fs::write(
            base.join("left_pocket_dir/AGENTS.md"),
            "#LEFT_POCKET_RUNTIME_CONTENT_START\nold\n#LEFT_POCKET_RUNTIME_CONTENT_END\n",
        )
        .unwrap();
        fs::write(
            base.join("project_dir/copilot-instructions.md"),
            "#LEFT_POCKET_TEMPLATE_DESTINATION: x.md\n<!-- BEGIN LEFT_POCKET TASK INTEGRATION -->\n",
        )
        .unwrap();
        fs::write(base.join("project_dir/legacy.md"), "#LEFT_POCKET_QUIET_MERGE\n").unwrap();
        // Must be skipped: inside a SKIP_DIRS tree.
        fs::write(base.join("left_pocket_dir/.git/config.md"), "#LEFT_POCKET_QUIET_MERGE\n").unwrap();

        let applied = rewrite_legacy_tokens_in_roots(&[
            base.join("left_pocket_dir"),
            base.join("project_dir"),
        ])
        .unwrap();

        assert_eq!(applied, 3);
        assert_eq!(
            fs::read_to_string(base.join("left_pocket_dir/AGENTS.md")).unwrap(),
            "#LEFT_POCKET_RUNTIME_CONTENT_START\nold\n#LEFT_POCKET_RUNTIME_CONTENT_END\n"
        );
        assert!(fs::read_to_string(base.join("project_dir/copilot-instructions.md"))
            .unwrap()
            .contains("<!-- BEGIN LEFT_POCKET TASK INTEGRATION -->"));
        assert_eq!(
            fs::read_to_string(base.join("project_dir/legacy.md")).unwrap(),
            "#LEFT_POCKET_QUIET_MERGE\n"
        );
        assert_eq!(
            fs::read_to_string(base.join("left_pocket_dir/.git/config.md")).unwrap(),
            "#LEFT_POCKET_QUIET_MERGE\n"
        );

        // Idempotent: a second pass finds nothing to change.
        assert_eq!(
            rewrite_legacy_tokens_in_roots(&[base.join("left_pocket_dir"), base.join("project_dir")])
                .unwrap(),
            0
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_apply_replacements_rewrites_all_structural_tokens() {
        let input = "#LEFT_POCKET_TEMPLATE_DESTINATION: x.md\n\
                     #LEFT_POCKET_QUIET_MERGE\n\
                     #LEFT_POCKET_MERGE_AT_RUNTIME\n\
                     #LEFT_POCKET_INSTALL_DESTINATION: y.yaml\n\
                     #LEFT_POCKET_RUNTIME_CONTENT_START\nbody\n#LEFT_POCKET_RUNTIME_CONTENT_END\n\
                     <!-- BEGIN LEFT_POCKET TASK INTEGRATION -->\n\
                     <!-- END LEFT_POCKET TASK INTEGRATION -->\n";
        let out = apply_replacements(input);
        assert!(!out.contains("SPOCKET"));
        assert!(out.contains("#LEFT_POCKET_TEMPLATE_DESTINATION"));
        assert!(out.contains("#LEFT_POCKET_QUIET_MERGE"));
        assert!(out.contains("#LEFT_POCKET_MERGE_AT_RUNTIME"));
        assert!(out.contains("#LEFT_POCKET_INSTALL_DESTINATION"));
        assert!(out.contains("#LEFT_POCKET_RUNTIME_CONTENT_START"));
        assert!(out.contains("#LEFT_POCKET_RUNTIME_CONTENT_END"));
        assert!(out.contains("<!-- BEGIN LEFT_POCKET TASK INTEGRATION -->"));
        assert!(out.contains("<!-- END LEFT_POCKET TASK INTEGRATION -->"));
    }

    #[test]
    fn test_apply_replacements_leaves_feature_tag_names_alone() {
        let input = "#SPOCKET_MUST_INSTALL\nSPOCKET_LOG_EVERYTHING:\n";
        let out = apply_replacements(input);
        assert_eq!(out, input);
    }

    #[test]
    fn test_apply_replacements_idempotent() {
        let input = "#LEFT_POCKET_TEMPLATE_DESTINATION: x.md\n";
        assert_eq!(apply_replacements(input), input);
    }

    #[test]
    fn test_count_replacements_counts_all_occurrences() {
        let input = "#LEFT_POCKET_RUNTIME_CONTENT_START\n#LEFT_POCKET_RUNTIME_CONTENT_START\n";
        assert_eq!(count_replacements(input), 2);
    }

    #[test]
    fn test_collect_rewrites_skips_skip_dirs_and_binary() {
        let base = std::env::temp_dir().join("left_pocket_test_upgrade_collect");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".git")).unwrap();
        fs::create_dir_all(base.join("target")).unwrap();
        fs::create_dir_all(base.join("subdir")).unwrap();
        fs::write(
            base.join(".git/secret.md"),
            "#LEFT_POCKET_RUNTIME_CONTENT_START\n",
        )
        .unwrap();
        fs::write(base.join("target/build.md"), "#LEFT_POCKET_QUIET_MERGE\n").unwrap();
        fs::write(
            base.join("subdir/agents.md"),
            "#LEFT_POCKET_TEMPLATE_DESTINATION: x\n",
        )
        .unwrap();
        fs::write(base.join("binary.bin"), "\x00\x01#SPOCKET\n").unwrap();

        let mut plan = Vec::new();
        collect_rewrites(&base, &mut plan).unwrap();

        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].0, base.join("subdir/agents.md"));
        assert_eq!(plan[0].1, 1);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn literal_artifact_scan_skips_archives_and_finds_active_left_pockets() {
        let base = std::env::temp_dir().join("left_pocket_test_literal_artifact_scan");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("abc123/{{SPOCKET_CONFIG_ROOT}}")).unwrap();
        fs::create_dir_all(base.join("snapshots/abc123/{{SPOCKET_CONFIG_ROOT}}")).unwrap();

        let mut artifacts = Vec::new();
        collect_literal_root_artifacts(&base, &mut artifacts).unwrap();

        assert_eq!(artifacts, vec![base.join("abc123/{{SPOCKET_CONFIG_ROOT}}")]);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn literal_artifact_is_safe_only_with_one_feature_tags_file() {
        let base = std::env::temp_dir().join("left_pocket_test_literal_artifact_safety");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("feature_tags.yaml"), "tags: {}\n").unwrap();
        assert!(safe_literal_artifact(&base).unwrap());

        fs::write(base.join("unexpected.txt"), "do not delete\n").unwrap();
        assert!(!safe_literal_artifact(&base).unwrap());

        let _ = fs::remove_dir_all(base);
    }

    #[cfg(unix)]
    #[test]
    fn literal_artifact_scan_and_safety_reject_symlinks() {
        use std::os::unix::fs::symlink;

        let base = std::env::temp_dir().join("left_pocket_test_literal_artifact_symlink");
        let outside = std::env::temp_dir().join("left_pocket_test_literal_artifact_outside");
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&outside);
        fs::create_dir_all(base.join("abc123")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("feature_tags.yaml"), "important: true\n").unwrap();
        let link = base.join("abc123/{{SPOCKET_CONFIG_ROOT}}");
        symlink(&outside, &link).unwrap();

        let mut artifacts = Vec::new();
        collect_literal_root_artifacts(&base, &mut artifacts).unwrap();
        assert!(artifacts.is_empty());
        assert!(!safe_literal_artifact(&link).unwrap());
        assert!(outside.join("feature_tags.yaml").is_file());

        let _ = fs::remove_dir_all(base);
        let _ = fs::remove_dir_all(outside);
    }
}
