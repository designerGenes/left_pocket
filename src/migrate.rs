//! `corner upgrade-installation` — one-way text migration that rewrites legacy
//! Spocket/Safe_pocket references to their Corner equivalents across the known
//! config and registry roots.
//!
//! This is **not** a template sync. It performs in-place text rewrites on files
//! that already live on disk (placed templates inside corners, config assets,
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
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

/// Token rewrites applied to every visited file. Order matters: longer/more
/// specific tokens are listed first so they win when one is a prefix of another.
const TOKEN_REPLACEMENTS: &[(&str, &str)] = &[
    ("#SPOCKET_RUNTIME_CONTENT_START", "#CORNER_RUNTIME_CONTENT_START"),
    ("#SPOCKET_RUNTIME_CONTENT_END", "#CORNER_RUNTIME_CONTENT_END"),
    ("#SPOCKET_TEMPLATE_DESTINATION", "#CORNER_TEMPLATE_DESTINATION"),
    ("#SPOCKET_QUIET_MERGE", "#CORNER_QUIET_MERGE"),
    ("#SPOCKET_MERGE_AT_RUNTIME", "#CORNER_MERGE_AT_RUNTIME"),
    ("#SPOCKET_INSTALL_DESTINATION", "#CORNER_INSTALL_DESTINATION"),
    (
        "<!-- BEGIN SPOCKET TASK INTEGRATION -->",
        "<!-- BEGIN CORNER TASK INTEGRATION -->",
    ),
    (
        "<!-- END SPOCKET TASK INTEGRATION -->",
        "<!-- END CORNER TASK INTEGRATION -->",
    ),
];

/// Directory names that are never descended into during a scan.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "unhoused",
    ".session-tools",
    "graphify-out",
];

/// Maximum file size (in bytes) we are willing to read and rewrite. Larger
/// files are almost certainly binary or generated and are skipped.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

pub struct UpgradeOptions {
    pub dry_run: bool,
    pub yes: bool,
    pub extra_roots: Vec<PathBuf>,
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
        println!("{}", "No Corner/Safe_pocket roots found to scan.".dimmed());
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
    for root in &roots {
        collect_rewrites(root, &mut plan)?;
    }

    if plan.is_empty() {
        println!(
            "{}",
            "No legacy references found — nothing to upgrade.".bright_green()
        );
        return Ok(());
    }

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

    if options.dry_run {
        println!(
            "{}",
            "Dry run only — no files were modified.".dimmed()
        );
        return Ok(());
    }

    if !options.yes {
        use std::io::{self, Write as IoWrite};
        print!(
            "{} ",
            "Apply these rewrites? [y/N]:".bright_white()
        );
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
    println!(
        "{}",
        "Run `corner -u <project>` to re-place templates from the upgraded config."
            .dimmed()
    );
    Ok(())
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
    fn test_apply_replacements_rewrites_all_structural_tokens() {
        let input = "#SPOCKET_TEMPLATE_DESTINATION: x.md\n\
                     #SPOCKET_QUIET_MERGE\n\
                     #SPOCKET_MERGE_AT_RUNTIME\n\
                     #SPOCKET_INSTALL_DESTINATION: y.yaml\n\
                     #SPOCKET_RUNTIME_CONTENT_START\nbody\n#SPOCKET_RUNTIME_CONTENT_END\n\
                     <!-- BEGIN SPOCKET TASK INTEGRATION -->\n\
                     <!-- END SPOCKET TASK INTEGRATION -->\n";
        let out = apply_replacements(input);
        assert!(!out.contains("SPOCKET"));
        assert!(out.contains("#CORNER_TEMPLATE_DESTINATION"));
        assert!(out.contains("#CORNER_QUIET_MERGE"));
        assert!(out.contains("#CORNER_MERGE_AT_RUNTIME"));
        assert!(out.contains("#CORNER_INSTALL_DESTINATION"));
        assert!(out.contains("#CORNER_RUNTIME_CONTENT_START"));
        assert!(out.contains("#CORNER_RUNTIME_CONTENT_END"));
        assert!(out.contains("<!-- BEGIN CORNER TASK INTEGRATION -->"));
        assert!(out.contains("<!-- END CORNER TASK INTEGRATION -->"));
    }

    #[test]
    fn test_apply_replacements_leaves_feature_tag_names_alone() {
        let input = "#SPOCKET_MUST_INSTALL\nSPOCKET_LOG_EVERYTHING:\n";
        let out = apply_replacements(input);
        assert_eq!(out, input);
    }

    #[test]
    fn test_apply_replacements_idempotent() {
        let input = "#CORNER_TEMPLATE_DESTINATION: x.md\n";
        assert_eq!(apply_replacements(input), input);
    }

    #[test]
    fn test_count_replacements_counts_all_occurrences() {
        let input = "#SPOCKET_RUNTIME_CONTENT_START\n#SPOCKET_RUNTIME_CONTENT_START\n";
        assert_eq!(count_replacements(input), 2);
    }

    #[test]
    fn test_collect_rewrites_skips_skip_dirs_and_binary() {
        let base = std::env::temp_dir().join("corner_test_upgrade_collect");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".git")).unwrap();
        fs::create_dir_all(base.join("target")).unwrap();
        fs::create_dir_all(base.join("subdir")).unwrap();
        fs::write(
            base.join(".git/secret.md"),
            "#SPOCKET_RUNTIME_CONTENT_START\n",
        )
        .unwrap();
        fs::write(base.join("target/build.md"), "#SPOCKET_QUIET_MERGE\n").unwrap();
        fs::write(
            base.join("subdir/agents.md"),
            "#SPOCKET_TEMPLATE_DESTINATION: x\n",
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
}
