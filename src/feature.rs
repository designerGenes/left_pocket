//! Feature file helpers: daily feature file resolution/creation, dated filename
//! parsing, and `place_automatically` feature-tag injection.
//!
//! This module centralises the logic used by the `daily-feature` subcommand so
//! that the VS Code extension can delegate file creation to the binary (keeping
//! the behaviour testable in Rust).

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Default subfolder (under FEATURES) where new daily feature files are created.
pub const DEFAULT_DAILY_SUBPATH: &str = "dailies";

/// Return today's date key in `YYYY-MM-DD` form using the local timezone.
pub fn today_date_key() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Zero-pad a number to at least two digits.
fn pad2(value: u32) -> String {
    format!("{:02}", value)
}

/// Map a month name (full or common abbreviation) to its 1-based number.
fn month_name_to_number(name: &str) -> Option<u32> {
    match name.to_lowercase().as_str() {
        "january" | "jan" => Some(1),
        "february" | "feb" => Some(2),
        "march" | "mar" => Some(3),
        "april" | "apr" => Some(4),
        "may" => Some(5),
        "june" | "jun" => Some(6),
        "july" | "jul" => Some(7),
        "august" | "aug" => Some(8),
        "september" | "sep" | "sept" => Some(9),
        "october" | "oct" => Some(10),
        "november" | "nov" => Some(11),
        "december" | "dec" => Some(12),
        _ => None,
    }
}

/// Split a filename stem into its separator-delimited parts.
///
/// Treats `-` and `_` as separators. Empty parts are removed so that numbered
/// suffixes like `2026_05_31_1` split cleanly.
fn split_parts(stem: &str) -> Vec<String> {
    stem.split(|c| c == '-' || c == '_')
        .filter(|p| !p.is_empty())
        .map(|p| p.to_string())
        .collect()
}

/// Parse a markdown filename into a `YYYY-MM-DD` date key if it "looks like" a
/// date. Returns `None` when no date can be recognised.
///
/// Recognised forms (optionally followed by extra `-`/`_` separated suffixes,
/// such as a numbered daily file `2026-05-31-1.md`):
/// - `YYYY-MM-DD`
/// - `MM-DD-YY`
/// - `Month-DD[-YYYY]` (month name first)
/// - `DD-Month[-YYYY]` (month name second)
pub fn parse_dated_filename(file_name: &str) -> Option<String> {
    let stem = strip_md_extension(file_name);
    let parts = split_parts(&stem);
    if parts.len() < 2 {
        return None;
    }

    let current_year = chrono::Local::now().format("%Y").to_string();

    // YYYY-MM-DD (4-digit year first, then numeric month/day)
    if parts.len() >= 3 {
        if let (Ok(y), Ok(m), Ok(d)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) {
            if parts[0].len() == 4 && (1..=12).contains(&m) && (1..=31).contains(&d) {
                return Some(format!("{:04}-{}-{}", y, pad2(m), pad2(d)));
            }
        }
    }

    // MM-DD-YY (numeric month/day, 2-digit year last)
    if parts.len() >= 3 {
        if let (Ok(m), Ok(d), Ok(y)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) {
            if parts[2].len() == 2 && (1..=12).contains(&m) && (1..=31).contains(&d) {
                return Some(format!("20{:02}-{}-{}", y, pad2(m), pad2(d)));
            }
        }
    }

    // Month-DD[-YYYY] (month name first)
    if let Some(m) = month_name_to_number(&parts[0]) {
        if let Ok(d) = parts[1].parse::<u32>() {
            if (1..=31).contains(&d) {
                let year = parts
                    .get(2)
                    .filter(|p| p.len() == 4 && p.parse::<u32>().is_ok())
                    .cloned()
                    .unwrap_or_else(|| current_year.clone());
                return Some(format!("{}-{}-{}", year, pad2(m), pad2(d)));
            }
        }
    }

    // DD-Month[-YYYY] (month name second)
    if let Some(m) = month_name_to_number(&parts[1]) {
        if let Ok(d) = parts[0].parse::<u32>() {
            if (1..=31).contains(&d) {
                let year = parts
                    .get(2)
                    .filter(|p| p.len() == 4 && p.parse::<u32>().is_ok())
                    .cloned()
                    .unwrap_or_else(|| current_year.clone());
                return Some(format!("{}-{}-{}", year, pad2(m), pad2(d)));
            }
        }
    }

    None
}

fn strip_md_extension(file_name: &str) -> String {
    if let Some(stripped) = file_name
        .strip_suffix(".md")
        .or_else(|| file_name.strip_suffix(".MD"))
        .or_else(|| file_name.strip_suffix(".Md"))
    {
        stripped.to_string()
    } else {
        file_name.to_string()
    }
}

/// A markdown feature file whose name parses to a date key.
#[derive(Debug, Clone)]
pub struct DatedFeatureFile {
    pub path: PathBuf,
    pub date_key: String,
    pub mtime: std::time::SystemTime,
}

/// Recursively collect every markdown file under `features_dir` whose name
/// parses to a date key.
pub fn find_dated_feature_files(features_dir: &Path) -> Vec<DatedFeatureFile> {
    let mut results = Vec::new();
    let mut stack = vec![features_dir.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            if !file_type.is_file() {
                continue;
            }

            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };

            if !name.to_lowercase().ends_with(".md") {
                continue;
            }

            if let Some(date_key) = parse_dated_filename(name) {
                let mtime = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                results.push(DatedFeatureFile {
                    path,
                    date_key,
                    mtime,
                });
            }
        }
    }

    results
}

/// Find the most-recently-modified existing feature file matching `date_key`.
pub fn find_existing_feature_for_date(
    features_dir: &Path,
    date_key: &str,
) -> Option<DatedFeatureFile> {
    let mut matches: Vec<DatedFeatureFile> = find_dated_feature_files(features_dir)
        .into_iter()
        .filter(|f| f.date_key == date_key)
        .collect();

    matches.sort_by(|a, b| {
        b.mtime
            .cmp(&a.mtime)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });

    matches.into_iter().next()
}

/// Read the feature tags YAML file and return the names of tags that have
/// `place_automatically: true`.
///
/// Top-level keys (no leading indentation, ending in `:`) are treated as tag
/// names. A tag is auto-placed when, within its block, a line matches
/// `place_automatically: true`.
pub fn auto_place_tags(feature_tags_yaml: &Path) -> Vec<String> {
    let content = match fs::read_to_string(feature_tags_yaml) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_auto_place_tags(&content)
}

/// Parse YAML content for tags marked `place_automatically: true`.
pub fn parse_auto_place_tags(content: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current_tag: Option<String> = None;
    let mut current_auto = false;

    let flush = |result: &mut Vec<String>, tag: &Option<String>, auto: bool| {
        if auto {
            if let Some(name) = tag {
                if !result.contains(name) {
                    result.push(name.clone());
                }
            }
        }
    };

    for raw_line in content.lines() {
        // Skip comments and blank lines.
        let without_comment = match raw_line.find('#') {
            Some(idx) => &raw_line[..idx],
            None => raw_line,
        };
        if without_comment.trim().is_empty() {
            continue;
        }

        let is_indented = raw_line
            .chars()
            .next()
            .map(|c| c == ' ' || c == '\t')
            .unwrap_or(false);

        if !is_indented {
            // New top-level key: flush the previous tag's state first.
            flush(&mut result, &current_tag, current_auto);

            let trimmed = without_comment.trim_end();
            if let Some(colon) = trimmed.find(':') {
                let key = trimmed[..colon].trim();
                if is_valid_tag_name(key) {
                    current_tag = Some(key.to_string());
                    current_auto = false;
                    continue;
                }
            }
            // Not a tag definition line.
            current_tag = None;
            current_auto = false;
        } else if current_tag.is_some() {
            // Indented line within a tag block.
            let trimmed = without_comment.trim();
            if let Some(value) = trimmed
                .strip_prefix("place_automatically:")
                .map(|v| v.trim())
            {
                current_auto = matches!(value.to_lowercase().as_str(), "true" | "yes" | "on");
            }
        }
    }

    flush(&mut result, &current_tag, current_auto);
    result
}

fn is_valid_tag_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Build the initial content for a freshly created feature file, prepending any
/// auto-placed tags above the date header.
pub fn build_feature_content(date_key: &str, auto_tags: &[String]) -> String {
    let mut out = String::new();
    for tag in auto_tags {
        out.push('#');
        out.push_str(tag);
        out.push('\n');
    }
    if !auto_tags.is_empty() {
        out.push('\n');
    }
    out.push_str(&format!("# {}\n\n", date_key));
    out
}

/// Sanitise a user-provided subpath so it stays within FEATURES.
fn sanitize_subpath(subpath: &str) -> PathBuf {
    let mut clean = PathBuf::new();
    for part in subpath.split(|c| c == '/' || c == '\\') {
        if part.is_empty() || part == "." || part == ".." {
            continue;
        }
        clean.push(part);
    }
    clean
}

/// Compute the next available numbered daily file path for `date_key` in
/// `target_dir` (e.g. `2026_05_31_1.md`, `2026_05_31_2.md`, ...).
pub fn next_numbered_path(target_dir: &Path, date_key: &str) -> PathBuf {
    let base = date_key.replace('-', "_");
    let mut n = 1;
    loop {
        let candidate = target_dir.join(format!("{}_{}.md", base, n));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Outcome of resolving a daily feature file.
#[derive(Debug)]
pub struct DailyFeatureOutcome {
    pub path: PathBuf,
    pub created: bool,
}

/// Resolve (and create if necessary) the daily feature file for today.
///
/// - When `force_new` is false: reuse any existing feature file dated today; if
///   none exists, create `FEATURES/<subpath>/<YYYY_MM_DD>.md`.
/// - When `force_new` is true: always create the next numbered file
///   `FEATURES/<subpath>/<YYYY_MM_DD>_<n>.md`.
///
/// Newly created files are seeded with `place_automatically` tags read from
/// `feature_tags_yaml`.
pub fn resolve_daily_feature(
    pocket_dir: &Path,
    subpath: &str,
    force_new: bool,
    feature_tags_yaml: &Path,
) -> Result<DailyFeatureOutcome> {
    let features_dir = pocket_dir.join("FEATURES");
    let date_key = today_date_key();

    if !force_new {
        if let Some(existing) = find_existing_feature_for_date(&features_dir, &date_key) {
            return Ok(DailyFeatureOutcome {
                path: existing.path,
                created: false,
            });
        }
    }

    let target_dir = features_dir.join(sanitize_subpath(subpath));
    fs::create_dir_all(&target_dir).with_context(|| {
        format!(
            "Failed to create daily feature directory: {}",
            target_dir.display()
        )
    })?;

    let target_path = if force_new {
        next_numbered_path(&target_dir, &date_key)
    } else {
        target_dir.join(format!("{}.md", date_key.replace('-', "_")))
    };

    let auto_tags = auto_place_tags(feature_tags_yaml);
    let content = build_feature_content(&date_key, &auto_tags);

    fs::write(&target_path, content)
        .with_context(|| format!("Failed to write feature file: {}", target_path.display()))?;

    Ok(DailyFeatureOutcome {
        path: target_path,
        created: true,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_iso_date() {
        assert_eq!(
            parse_dated_filename("2026-05-31.md"),
            Some("2026-05-31".to_string())
        );
        assert_eq!(
            parse_dated_filename("2026_05_31.md"),
            Some("2026-05-31".to_string())
        );
    }

    #[test]
    fn parses_iso_date_with_numbered_suffix() {
        assert_eq!(
            parse_dated_filename("2026-05-31-1.md"),
            Some("2026-05-31".to_string())
        );
        assert_eq!(
            parse_dated_filename("2026_05_31_2.md"),
            Some("2026-05-31".to_string())
        );
    }

    #[test]
    fn parses_mdy_two_digit_year() {
        assert_eq!(
            parse_dated_filename("05-31-26.md"),
            Some("2026-05-31".to_string())
        );
    }

    #[test]
    fn parses_month_name_forms() {
        assert_eq!(
            parse_dated_filename("May-31-2026.md"),
            Some("2026-05-31".to_string())
        );
        assert_eq!(
            parse_dated_filename("31-May-2026.md"),
            Some("2026-05-31".to_string())
        );
    }

    #[test]
    fn rejects_non_date_names() {
        assert_eq!(parse_dated_filename("00.md"), None);
        assert_eq!(parse_dated_filename("notes.md"), None);
        assert_eq!(parse_dated_filename("feature-auth.md"), None);
    }

    #[test]
    fn auto_place_tags_detects_true() {
        let yaml = "\
SPOCKET_MUST_TALK_LIKE_A_CAT:
  description: \"talk like a cat\"
  place_automatically: true
  type: \"during hook\"
SPOCKET_OTHER:
  description: \"no auto\"
  type: \"done hook\"
SPOCKET_ALSO_AUTO:
  place_automatically: yes
";
        let tags = parse_auto_place_tags(yaml);
        assert_eq!(
            tags,
            vec![
                "SPOCKET_MUST_TALK_LIKE_A_CAT".to_string(),
                "SPOCKET_ALSO_AUTO".to_string()
            ]
        );
    }

    #[test]
    fn auto_place_tags_ignores_false_and_missing() {
        let yaml = "\
TAG_A:
  place_automatically: false
TAG_B:
  description: \"x\"
";
        assert!(parse_auto_place_tags(yaml).is_empty());
    }

    #[test]
    fn auto_place_tags_handles_comments() {
        let yaml = "\
# a comment
TAG_A:
  # nested comment
  place_automatically: true # inline
";
        assert_eq!(parse_auto_place_tags(yaml), vec!["TAG_A".to_string()]);
    }

    #[test]
    fn build_content_prepends_tags() {
        let content =
            build_feature_content("2026-05-31", &["TAG_A".to_string(), "TAG_B".to_string()]);
        assert_eq!(content, "#TAG_A\n#TAG_B\n\n# 2026-05-31\n\n");
    }

    #[test]
    fn build_content_without_tags() {
        let content = build_feature_content("2026-05-31", &[]);
        assert_eq!(content, "# 2026-05-31\n\n");
    }

    #[test]
    fn resolve_creates_then_reuses() {
        let tmp = std::env::temp_dir().join(format!("spocket_feat_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        let tags_yaml = tmp.join("feature_tags.yaml");
        fs::write(&tags_yaml, "AUTO_TAG:\n  place_automatically: true\n").unwrap();

        // First call creates the base file with the auto tag.
        let first = resolve_daily_feature(&tmp, "dailies", false, &tags_yaml).unwrap();
        assert!(first.created);
        assert!(first.path.exists());
        assert!(first.path.starts_with(tmp.join("FEATURES").join("dailies")));
        let body = fs::read_to_string(&first.path).unwrap();
        assert!(body.starts_with("#AUTO_TAG\n"));

        // Second non-new call reuses the existing file.
        let second = resolve_daily_feature(&tmp, "dailies", false, &tags_yaml).unwrap();
        assert!(!second.created);
        assert_eq!(second.path, first.path);

        // force_new creates a numbered sibling.
        let third = resolve_daily_feature(&tmp, "dailies", true, &tags_yaml).unwrap();
        assert!(third.created);
        assert_ne!(third.path, first.path);
        let name = third
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(name.ends_with("_1.md"), "unexpected name: {name}");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_reuses_user_named_today_file() {
        let tmp = std::env::temp_dir().join(format!("spocket_feat_user_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let features = tmp.join("FEATURES");
        fs::create_dir_all(&features).unwrap();

        let tags_yaml = tmp.join("feature_tags.yaml");
        fs::write(&tags_yaml, "").unwrap();

        // A user-created file whose name parses to today's date.
        let today = today_date_key();
        let user_file = features.join(format!("{}-my-notes.md", today));
        fs::write(&user_file, "# existing\n").unwrap();

        let outcome = resolve_daily_feature(&tmp, "dailies", false, &tags_yaml).unwrap();
        assert!(!outcome.created);
        assert_eq!(outcome.path, user_file);

        let _ = fs::remove_dir_all(&tmp);
    }
}
