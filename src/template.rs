use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

// ── Default assets (embedded at compile time) ────────────────────────────────
// These are the "factory defaults" that ship with spocket. Rather than
// hand-maintaining a list of `include_str!` constants, `build.rs` walks the
// entire `src/templates` tree at compile time and generates the
// `EMBEDDED_TEMPLATES: &[(&str, &str)]` table below (relative_path,
// file_contents). Adding, moving, or renaming a template file therefore needs
// no Rust changes.
//
// On first run each embedded file is materialised into the user's config
// directory (never overwriting an existing file):
// - Files whose `#SPOCKET_TEMPLATE_DESTINATION` targets `{{SPOCKET_CONFIG_ROOT}}`
//   or `{{SPOCKET_REGISTRY_ROOT}}` are *interpreted* now — the directive is
//   stripped, the two install-known roots are expanded, and the result is
//   written to the resolved path (e.g. `directory_structure.md` →
//   `$HOME/.config/safe_pocket/directory_structure.yaml`).
// - All other files are *staged verbatim* under
//   `$HOME/.config/safe_pocket/templates/<relative_path>` so the runtime
//   template loader can parse and apply them to individual corners.
include!(concat!(env!("OUT_DIR"), "/embedded_templates.rs"));

// ── Template variables ───────────────────────────────────────────────────────

/// Context needed to expand template variables.
pub struct TemplateContext {
    /// Absolute path to the corner root (e.g. `~/.safe_pocket/19930adf3aaa`).
    pub spocket_root: PathBuf,
    /// Absolute path to the primary project directory.
    pub project_root: PathBuf,
    /// Short name of the corner (the directory basename / hash).
    pub spocket_name: String,
    /// Absolute path to the global observations directory (`~/.safe_pocket/observations`).
    pub global_observations_path: PathBuf,
    /// Absolute path to the corner config directory (`$HOME/.config/safe_pocket`).
    pub config_root: PathBuf,
}

/// Legacy markers for the old Beads integration block. Retained only so that
/// corners created before the built-in task tracker can have the stale block
/// stripped out on their next runtime merge. Nothing new is ever written with
/// these markers.
const LEGACY_BEADS_BEGIN_MARKER: &str = "<!-- BEGIN BEADS INTEGRATION -->";
const LEGACY_BEADS_END_MARKER: &str = "<!-- END BEADS INTEGRATION -->";

/// Agent-facing guidance injected into AGENTS.md at runtime, describing
/// Corner's built-in task tracker (`corner task`).
const TASK_RUNTIME_BLOCK: &str = r#"<!-- BEGIN SPOCKET TASK INTEGRATION -->
## Issue Tracking with `corner task`

**IMPORTANT**: This project tracks all work in Corner's built-in task
tracker. Do NOT use markdown TODO lists or external issue trackers — use
`corner task` so every agent shares one source of truth.

### Why `corner task`?

- Fast: backed by a local SQLite database, no network or daemon required.
- Shared: every agent on this corner sees the same task list.
- Scoped: tasks are grouped per project by a prefix derived automatically from
  the corner — just run the commands from inside the project directory.

### Quick reference

```bash
# What work is open (lowest priority number = highest urgency)?
corner task list
corner task list --priority 1      # only P0 and P1
corner task list --raw             # JSON, for programmatic use

# Create work
corner task create --named "Implement feature X" \
  --description "Why this matters and what to do" --priority 1

# Drive a task through its lifecycle (ID may be the full id or the suffix)
corner task <ID> assign --agent "Builder"
corner task <ID> start  --notes "Starting now"
corner task <ID> log    --notes "Progress / findings"
corner task <ID> close  --notes "Done and verified"
corner task <ID> discard
corner task <ID> describe          # full details + history
corner task <ID> describe --raw    # JSON
```

### Priorities

- `0` — Critical (security, data loss, broken builds)
- `1` — High (major features, important bugs)
- `2` — Medium (default)
- `3` — Low (polish, optimization)
- `4` — Backlog (future ideas)

### Workflow for AI Agents

1. **Check open work**: `corner task list` before asking what to do.
2. **Claim it**: `corner task <ID> assign --agent "<you>"` then
   `corner task <ID> start`.
3. **Record progress**: `corner task <ID> log --notes "…"` as you go.
4. **Discover new work?** `corner task create --named "…" --description "…"`.
5. **Finish**: `corner task <ID> close --notes "…"`.

### Rules

- ✅ Use `corner task` for ALL task tracking.
- ✅ Use `--raw` when you need structured (JSON) output.
- ❌ Do NOT create markdown TODO lists.
- ❌ Do NOT use external issue trackers.
<!-- END SPOCKET TASK INTEGRATION -->
"#;

/// Replace `{{CORNER_ROOT}}`/`{{SPOCKET_ROOT}}`, `{{PROJECT_ROOT}}`,
/// `{{CORNER_NAME}}`/`{{SPOCKET_NAME}}`, `{{GLOBAL_OBSERVATIONS_PATH}}`,
/// `{{CORNER_CONFIG_ROOT}}`/`{{SPOCKET_CONFIG_ROOT}}`, and
/// `{{CORNER_REGISTRY_ROOT}}`/`{{SPOCKET_REGISTRY_ROOT}}` in `text`.
pub fn expand_variables(text: &str, ctx: &TemplateContext) -> String {
    let registry_root = crate::registry::registry_root()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "{{CORNER_REGISTRY_ROOT}}".to_string());
    text.replace("{{CORNER_ROOT}}", &ctx.spocket_root.to_string_lossy())
        .replace("{{SPOCKET_ROOT}}", &ctx.spocket_root.to_string_lossy())
        .replace("{{PROJECT_ROOT}}", &ctx.project_root.to_string_lossy())
        .replace("{{CORNER_NAME}}", &ctx.spocket_name)
        .replace("{{SPOCKET_NAME}}", &ctx.spocket_name)
        .replace(
            "{{GLOBAL_OBSERVATIONS_PATH}}",
            &ctx.global_observations_path.to_string_lossy(),
        )
        .replace("{{CORNER_CONFIG_ROOT}}", &ctx.config_root.to_string_lossy())
        .replace(
            "{{SPOCKET_CONFIG_ROOT}}",
            &ctx.config_root.to_string_lossy(),
        )
        .replace("{{CORNER_REGISTRY_ROOT}}", &registry_root)
        .replace("{{SPOCKET_REGISTRY_ROOT}}", &registry_root)
}

/// Token marking a destination/content as resolvable at install time (before any
/// corner exists), using only the two install-known roots.
const CONFIG_ROOT_TOKEN: &str = "{{SPOCKET_CONFIG_ROOT}}";
const CORNER_CONFIG_ROOT_TOKEN: &str = "{{CORNER_CONFIG_ROOT}}";
const REGISTRY_ROOT_TOKEN: &str = "{{SPOCKET_REGISTRY_ROOT}}";
const CORNER_REGISTRY_ROOT_TOKEN: &str = "{{CORNER_REGISTRY_ROOT}}";

/// Expand only the two install-known roots (`{{SPOCKET_CONFIG_ROOT}}` and
/// `{{SPOCKET_REGISTRY_ROOT}}`). Corner-level variables such as
/// `{{SPOCKET_ROOT}}` are intentionally left untouched so that literal examples
/// embedded in interpreted files (e.g. feature-tag descriptions) survive.
fn expand_install_roots(text: &str, config_root: &Path, registry_root: &Path) -> String {
    text.replace(CORNER_CONFIG_ROOT_TOKEN, &config_root.to_string_lossy())
        .replace(CONFIG_ROOT_TOKEN, &config_root.to_string_lossy())
        .replace(CORNER_REGISTRY_ROOT_TOKEN, &registry_root.to_string_lossy())
        .replace(REGISTRY_ROOT_TOKEN, &registry_root.to_string_lossy())
}

fn filter_template_content(content: &str, _ctx: &TemplateContext) -> String {
    // Always strip any legacy `BEADS_DIR=` lines: the built-in task tracker
    // replaced beads, so this variable is never wanted anymore.
    let mut kept = Vec::new();
    for line in content.lines() {
        if line.trim_start().starts_with("BEADS_DIR=") {
            continue;
        }
        kept.push(line);
    }

    if content.ends_with('\n') && !kept.is_empty() {
        format!("{}\n", kept.join("\n"))
    } else {
        kept.join("\n")
    }
}

fn runtime_content_for_template(tmpl: &Template, ctx: &TemplateContext) -> String {
    let mut sections = Vec::new();
    let base = expand_variables(&tmpl.content, ctx).trim().to_string();
    if !base.is_empty() {
        sections.push(base);
    }

    if tmpl.destination == "{{SPOCKET_ROOT}}/AGENTS.md"
        || tmpl.destination == "{{CORNER_ROOT}}/AGENTS.md"
    {
        sections.push(TASK_RUNTIME_BLOCK.trim().to_string());
    }

    sections.join("\n\n")
}

fn expand_template_content(content: &str, ctx: &TemplateContext) -> String {
    expand_variables(content, ctx)
}

// ── Parsed template ──────────────────────────────────────────────────────────

/// A single parsed template file.
#[derive(Debug, Clone)]
pub struct Template {
    /// Relative destination path inside the corner (after variable expansion).
    pub destination: String,
    /// File content (everything after the metadata lines, with `#SPOCKET` lines stripped).
    pub content: String,
    /// If true, merge with existing file rather than overwriting.
    pub quiet_merge: bool,
    /// If true, an empty destination file is placed at creation and content is
    /// injected at runtime (VS Code open/close)
    /// wrapped in `#SPOCKET_RUNTIME_CONTENT_START` / `#SPOCKET_RUNTIME_CONTENT_END` markers.
    pub merge_at_runtime: bool,
    /// Original source file path (for diagnostics).
    #[allow(dead_code)]
    pub source_path: PathBuf,
}

/// Parse a template file into one or more [`Template`] blocks.
///
/// The first non-empty line **must** start with `#SPOCKET_TEMPLATE_DESTINATION`
/// followed by a colon (optional) and the destination path. A file may contain
/// **multiple** `#SPOCKET_TEMPLATE_DESTINATION` directives: each one begins a new
/// block, and all content beneath it applies to that destination until the next
/// `#SPOCKET_TEMPLATE_DESTINATION` directive (or end of file). Blocks that target
/// the same destination are concatenated.
///
/// All other lines that start with `#SPOCKET` are treated as metadata and
/// stripped. Recognised per-block metadata directives:
/// - `#SPOCKET_QUIET_MERGE` — merge with existing file instead of overwriting.
/// - `#SPOCKET_MERGE_AT_RUNTIME` — inject content at runtime.
pub fn parse_template(path: &Path) -> Result<Vec<Template>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read template file: {}", path.display()))?;
    parse_template_content(&raw, path)
}

/// Parse already-loaded template `raw` content into one or more [`Template`]
/// blocks. `source` is used only for diagnostics and `Template::source_path`.
/// See [`parse_template`] for the directive grammar.
pub fn parse_template_content(raw: &str, source: &Path) -> Result<Vec<Template>> {
    let path = source;
    let raw_ends_with_newline = raw.ends_with('\n');

    // Accumulator for the block currently being built.
    struct Block {
        destination: String,
        lines: Vec<String>,
        quiet_merge: bool,
        merge_at_runtime: bool,
    }

    let mut blocks: Vec<Block> = Vec::new();

    for line in raw.lines() {
        if let Some(dest) = parse_destination_directive(line) {
            // Start a new block targeting `dest`.
            blocks.push(Block {
                destination: dest,
                lines: Vec::new(),
                quiet_merge: false,
                merge_at_runtime: false,
            });
            continue;
        }

        // Content lines must belong to an already-opened block.
        let Some(current) = blocks.last_mut() else {
            // Allow leading blank lines before the first directive; anything else
            // (non-whitespace before the first destination) is an error.
            if line.trim().is_empty() {
                continue;
            }
            // Tolerate other `#SPOCKET_*` directive lines (e.g. a stray
            // `#SPOCKET_INSTALL_DESTINATION`) that may precede the first
            // template block — they are install-time metadata and are simply
            // stripped here.
            if line.trim_start().starts_with("#SPOCKET") {
                continue;
            }
            return Err(anyhow!(
                "Template file '{}' is missing #SPOCKET_TEMPLATE_DESTINATION before its content.\n\
                     Found: {}",
                path.display(),
                line
            ));
        };

        let trimmed = line.trim();
        if trimmed == "#SPOCKET_QUIET_MERGE" {
            current.quiet_merge = true;
        } else if trimmed == "#SPOCKET_MERGE_AT_RUNTIME" {
            current.merge_at_runtime = true;
        } else if line.starts_with("#SPOCKET") {
            // Strip unrecognised metadata directives.
        } else {
            current.lines.push(line.to_string());
        }
    }

    if blocks.is_empty() {
        return Err(anyhow!(
            "Template file '{}' is missing #SPOCKET_TEMPLATE_DESTINATION.",
            path.display()
        ));
    }

    let num_blocks = blocks.len();
    let mut templates = Vec::with_capacity(num_blocks);
    for (idx, block) in blocks.into_iter().enumerate() {
        let is_final = idx + 1 == num_blocks;
        let content = if block.lines.is_empty() {
            String::new()
        } else {
            let mut content = block.lines.join("\n");
            // Interior blocks are always terminated by a newline in the source
            // (the following directive sits on its own line), so a trailing blank
            // line in the source is preserved. The final block mirrors the file.
            let want_trailing = if is_final {
                raw_ends_with_newline
            } else {
                true
            };
            if want_trailing {
                content.push('\n');
            }
            // Trim a single leading newline (common when a blank line follows the
            // destination directive or stripped metadata directives).
            if let Some(stripped) = content.strip_prefix('\n') {
                content = stripped.to_string();
            }
            content
        };

        templates.push(Template {
            destination: block.destination,
            content,
            quiet_merge: block.quiet_merge,
            merge_at_runtime: block.merge_at_runtime,
            source_path: path.to_path_buf(),
        });
    }

    Ok(templates)
}

/// Parse the `#SPOCKET_TEMPLATE_DESTINATION` directive from a line.
/// Accepts both `#SPOCKET_TEMPLATE_DESTINATION: path` and
/// `#SPOCKET_TEMPLATE_DESTINATION path` (with or without colon).
fn parse_destination_directive(line: &str) -> Option<String> {
    let line = line.trim();
    let prefix = "#SPOCKET_TEMPLATE_DESTINATION";

    if !line.starts_with(prefix) {
        return None;
    }

    let rest = &line[prefix.len()..];
    // Strip optional colon and whitespace
    let rest = rest.trim_start_matches(':').trim();

    if rest.is_empty() {
        return None;
    }

    Some(rest.to_string())
}

// ── Directory structure template ─────────────────────────────────────────────

/// Parse a directory structure file into a list of relative directory paths.
///
/// Format:
/// ```text
/// .github
/// - prompts
/// - skills
/// FEATURES
/// - SomeFolder
/// - - SomeSubfolder
/// observations
/// ```
///
/// Indentation is expressed by leading `- ` prefixes, optionally preceded by
/// whitespace. Each `- ` adds one level of nesting under the most recent parent
/// at the preceding depth.
pub fn parse_directory_structure(content: &str) -> Result<Vec<PathBuf>> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    // Stack of (depth, path) representing the current nesting context.
    let mut stack: Vec<(usize, PathBuf)> = Vec::new();

    for (line_no, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim_end();
        if line.is_empty() {
            continue;
        }

        // Count leading "- " prefixes to determine depth
        let (depth, name) = parse_directory_line(line);

        if name.is_empty() {
            continue;
        }

        // Pop stack back to find the parent at depth-1
        while stack.last().map_or(false, |(d, _)| *d >= depth) {
            stack.pop();
        }

        let path = if let Some((_, parent)) = stack.last() {
            parent.join(&name)
        } else {
            if depth > 0 {
                return Err(anyhow!(
                    "directory structure line {}: indented entry '{}' has no parent",
                    line_no + 1,
                    name
                ));
            }
            PathBuf::from(&name)
        };

        dirs.push(path.clone());
        stack.push((depth, path));
    }

    Ok(dirs)
}

/// Parse a single line from the directory structure file.
/// Returns `(depth, directory_name)`.
fn parse_directory_line(line: &str) -> (usize, String) {
    let mut depth: usize = 0;
    let mut rest = line.trim_start();

    while rest.starts_with("- ") {
        depth += 1;
        rest = &rest[2..];
    }

    let name = rest.trim().to_string();
    (depth, name)
}

// ── Template directory helpers ───────────────────────────────────────────────

/// Returns the preferred config directory, falling back to legacy names when
/// only old locations exist.
pub fn safe_pocket_config_dir() -> Result<PathBuf> {
    crate::branding::preferred_config_root()
}

pub fn config_path(relative: &str) -> Result<PathBuf> {
    crate::branding::resolve_config_relative_path(Path::new(relative))
}

/// Returns the path to the templates directory, preferring Corner's config root
/// but falling back to legacy config directories when that specific directory is
/// still only present there.
pub fn templates_dir() -> Result<PathBuf> {
    config_path("templates")
}

/// Returns the path to the global observations directory
/// (`$HOME/.safe_pocket/observations`), creating it if necessary.
pub fn global_observations_dir() -> Result<PathBuf> {
    crate::registry::global_observations_dir()
}

/// Ensure the default assets exist in the user's config directory.
/// Only writes files that don't already exist (respects user customizations).
pub fn ensure_default_assets() -> Result<()> {
    let config_dir = safe_pocket_config_dir()?;
    let tmpl_dir = config_dir.join("templates");
    fs::create_dir_all(&tmpl_dir).context("Failed to create templates directory")?;

    // Ensure global observations directory exists in the registry, not config.
    let _ = crate::registry::global_observations_dir()?;

    let registry_root = crate::registry::registry_root()?;
    install_embedded_templates(&config_dir, &registry_root)
}

/// Install default assets into Corner's canonical roots regardless of whether
/// legacy safe_pocket directories already exist. This is used by the install
/// script so installation always seeds `~/.config/corner` and `~/.corner`.
pub fn install_default_assets_to_current_roots() -> Result<()> {
    let config_dir = crate::branding::current_config_root()?;
    let tmpl_dir = config_dir.join("templates");
    fs::create_dir_all(&tmpl_dir).context("Failed to create primary templates directory")?;

    let registry_root = crate::registry::current_registry_dir()?;
    fs::create_dir_all(registry_root.join("observations"))
        .context("Failed to create primary global observations directory")?;

    install_embedded_templates(&config_dir, &registry_root)
}

/// Materialise every embedded template into the user's config directory, never
/// overwriting an existing file.
///
/// Placement is driven entirely by directives inside each file (no hard-coded
/// file names):
/// - `#SPOCKET_INSTALL_DESTINATION: <path>` places the file at `<path>` (one or
///   more) at install time. These directives are **stripped** from the placed
///   file. Used for assets that live directly in the config root, e.g.
///   `directory_structure.yaml`, `feature_tags.yaml`, and the conversation
///   feature-tag definition.
/// - When a file has **no** `#SPOCKET_INSTALL_DESTINATION`, it is mirrored to
///   `<config_dir>/templates/<relative_path>` so the runtime loader can find it.
///
/// `#SPOCKET_TEMPLATE_DESTINATION` directives are always **left intact** in the
/// placed file — they are consumed later, at runtime, when a new corner is
/// created.
pub fn install_embedded_templates(config_dir: &Path, registry_root: &Path) -> Result<()> {
    for (rel, content) in EMBEDDED_TEMPLATES {
        install_one_embedded(rel, content, config_dir, registry_root)?;
    }
    Ok(())
}

fn install_one_embedded(
    rel: &str,
    content: &str,
    config_dir: &Path,
    registry_root: &Path,
) -> Result<()> {
    // Collect any explicit install destinations and produce the placed body
    // (the same content with every `#SPOCKET_INSTALL_DESTINATION` line removed).
    let install_dirs: Vec<String> = content
        .lines()
        .filter_map(parse_install_destination_directive)
        .collect();
    let placed = strip_install_directives(content);

    if install_dirs.is_empty() {
        // Default: mirror into the system-wide templates directory so the
        // runtime loader can pick it up (its TEMPLATE_DESTINATION is preserved).
        let staged = config_dir.join("templates").join(rel);
        return write_if_absent(&staged, &placed, "template");
    }

    // Explicit install destination(s): place directly into the config tree.
    for dir in &install_dirs {
        let dest = expand_install_roots(dir, config_dir, registry_root);
        write_if_absent(Path::new(&dest), &placed, "config asset")?;
    }
    Ok(())
}

/// Remove every `#SPOCKET_INSTALL_DESTINATION` line from `content`. If any were
/// removed, a leading run of blank lines is trimmed so the placed file starts at
/// its real content. `#SPOCKET_TEMPLATE_DESTINATION` and all other lines are
/// preserved verbatim.
fn strip_install_directives(content: &str) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut removed = false;
    for line in content.lines() {
        if parse_install_destination_directive(line).is_some() {
            removed = true;
            continue;
        }
        kept.push(line);
    }

    let mut body = kept.join("\n");
    if content.ends_with('\n') && !body.is_empty() {
        body.push('\n');
    }
    if removed {
        while body.starts_with('\n') {
            body.remove(0);
        }
    }
    body
}

/// Parse a `#SPOCKET_INSTALL_DESTINATION` directive from a line. Accepts both
/// `#SPOCKET_INSTALL_DESTINATION: path` and `#SPOCKET_INSTALL_DESTINATION path`.
fn parse_install_destination_directive(line: &str) -> Option<String> {
    let line = line.trim();
    let prefix = "#SPOCKET_INSTALL_DESTINATION";
    if !line.starts_with(prefix) {
        return None;
    }
    let rest = line[prefix.len()..].trim_start_matches(':').trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn write_if_absent(target: &Path, content: &str, label: &str) -> Result<()> {
    if target.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }
    fs::write(target, content)
        .with_context(|| format!("Failed to write {}: {}", label, target.display()))?;
    if crate::verbose() {
        println!(
            "{} {}",
            format!("Installed {}:", label).bright_green(),
            target.display().to_string().dimmed()
        );
    }
    Ok(())
}
pub fn load_templates() -> Result<Vec<Template>> {
    let tmpl_dir = templates_dir()?;

    if !tmpl_dir.exists() {
        return Ok(Vec::new());
    }

    let mut templates = Vec::new();
    let mut dirs_to_visit = vec![tmpl_dir.clone()];

    while let Some(dir) = dirs_to_visit.pop() {
        for entry in fs::read_dir(&dir)
            .with_context(|| format!("Failed to read templates directory: {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // The `agents/` subdirectory holds unified agent definitions
                // (synced via `spocket sync agents`), not corner templates.
                if path.file_name().map_or(false, |n| n == "agents") {
                    continue;
                }
                dirs_to_visit.push(path);
                continue;
            }

            if !path.is_file() {
                continue;
            }

            match parse_template(&path) {
                Ok(mut parsed) => templates.append(&mut parsed),
                Err(e) => {
                    if crate::verbose() {
                        eprintln!(
                            "{} skipping {}: {}",
                            "Warning:".bright_yellow(),
                            path.display(),
                            e
                        );
                    }
                }
            }
        }
    }

    Ok(merge_templates_by_destination(templates))
}

fn merge_templates_by_destination(templates: Vec<Template>) -> Vec<Template> {
    let mut grouped: BTreeMap<String, Template> = BTreeMap::new();

    for tmpl in templates {
        grouped
            .entry(tmpl.destination.clone())
            .and_modify(|existing| {
                if !existing.content.ends_with('\n') && !existing.content.is_empty() {
                    existing.content.push('\n');
                }
                existing.content.push_str(&tmpl.content);
                existing.quiet_merge |= tmpl.quiet_merge;
                existing.merge_at_runtime |= tmpl.merge_at_runtime;
            })
            .or_insert(tmpl);
    }

    grouped.into_values().collect()
}

/// Load the directory structure, respecting project-local override.
///
/// 1. If `project_dir` contains a `directory_template.md` or `directory_template.yaml`, use it.
/// 2. Otherwise, look in the preferred config roots for *exactly one* file matching
///    `directory_structure.md` or `directory_structure.yaml`. Error if more than one is found.
/// 3. If neither exists, return an empty list.
pub fn load_directory_structure(project_dir: Option<&Path>) -> Result<Vec<PathBuf>> {
    // Check project-local first
    if let Some(proj) = project_dir {
        for local_file_name in ["directory_template.md", "directory_template.yaml"] {
            let local_file = proj.join(local_file_name);
            if local_file.exists() {
                let content = fs::read_to_string(&local_file)
                    .with_context(|| format!("Failed to read project-local {}", local_file_name))?;
                return parse_directory_structure(&content);
            }
        }
    }

    for config_dir in crate::branding::known_config_roots()? {
        let mut structure_files: Vec<PathBuf> = Vec::new();
        if config_dir.exists() {
            for entry in fs::read_dir(&config_dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name == "directory_structure.md"
                            || name == "directory_structure.yaml"
                            || name == "directory_template.md"
                            || name == "directory_template.yaml"
                        {
                            structure_files.push(path);
                        }
                    }
                }
            }
        }

        match structure_files.len() {
            0 => continue,
            1 => {
                let content = fs::read_to_string(&structure_files[0])?;
                return parse_directory_structure(&content);
            }
            n => {
                return Err(anyhow!(
                    "Found {} directory structure files in {}, expected at most 1:\n{}",
                    n,
                    config_dir.display(),
                    structure_files
                        .iter()
                        .map(|p| format!("  - {}", p.display()))
                        .collect::<Vec<_>>()
                        .join("\n")
                ));
            }
        }
    }

    Ok(Vec::new())
}

// ── Runtime merge ────────────────────────────────────────────────────────────

pub const RUNTIME_START_MARKER: &str = "#SPOCKET_RUNTIME_CONTENT_START";
pub const RUNTIME_END_MARKER: &str = "#SPOCKET_RUNTIME_CONTENT_END";

#[derive(Clone, Copy, PartialEq, Eq)]
enum TemplateApplyMode {
    Create,
    Upgrade,
}

fn strip_markers(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut in_block = false;

    for line in content.split_inclusive('\n') {
        let line_content = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = line_content.trim();
        if trimmed == RUNTIME_START_MARKER {
            in_block = true;
            continue;
        }
        if trimmed == RUNTIME_END_MARKER {
            in_block = false;
            continue;
        }
        if !in_block {
            result.push_str(line);
        }
    }

    result
}

fn strip_managed_block(content: &str, start_marker: &str, end_marker: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut in_block = false;

    for line in content.split_inclusive('\n') {
        let line_content = line.strip_suffix('\n').unwrap_or(line);
        let trimmed = line_content.trim();
        if trimmed == start_marker {
            in_block = true;
            continue;
        }
        if trimmed == end_marker {
            in_block = false;
            continue;
        }
        if !in_block {
            result.push_str(line);
        }
    }

    result
}

fn expand_runtime_variables_in_content(content: &str, ctx: &TemplateContext) -> String {
    let mut result = String::with_capacity(content.len());

    for line in content.split_inclusive('\n') {
        let (line_content, newline) = match line.strip_suffix('\n') {
            Some(stripped) => (stripped, "\n"),
            None => (line, ""),
        };

        if line_content.starts_with("#SPOCKET") {
            result.push_str(line_content);
        } else {
            result.push_str(&expand_variables(line_content, ctx));
        }

        result.push_str(newline);
    }

    result
}

fn expand_runtime_variables_in_file(dest_path: &Path, ctx: &TemplateContext) -> Result<bool> {
    if !dest_path.exists() {
        return Ok(false);
    }

    let existing = fs::read_to_string(dest_path)
        .with_context(|| format!("Failed to read: {}", dest_path.display()))?;
    let expanded = expand_runtime_variables_in_content(&existing, ctx);

    if expanded == existing {
        return Ok(false);
    }

    fs::write(dest_path, &expanded)
        .with_context(|| format!("Failed to write: {}", dest_path.display()))?;

    Ok(true)
}

pub fn inject_runtime_content(dest_path: &Path, runtime_content: &str) -> Result<bool> {
    let existing = if dest_path.exists() {
        fs::read_to_string(dest_path)
            .with_context(|| format!("Failed to read: {}", dest_path.display()))?
    } else {
        String::new()
    };

    let base = strip_managed_block(
        &strip_markers(&existing),
        LEGACY_BEADS_BEGIN_MARKER,
        LEGACY_BEADS_END_MARKER,
    );

    let mut injected = base;
    if !injected.is_empty() && !injected.ends_with('\n') {
        injected.push('\n');
    }
    injected.push_str(RUNTIME_START_MARKER);
    injected.push('\n');
    injected.push_str(runtime_content.trim_end());
    injected.push('\n');
    injected.push_str(RUNTIME_END_MARKER);
    injected.push('\n');

    if injected == existing {
        return Ok(false);
    }

    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create parent directory: {}", parent.display()))?;
    }

    fs::write(dest_path, &injected)
        .with_context(|| format!("Failed to write: {}", dest_path.display()))?;

    Ok(true)
}

pub fn strip_runtime_content(dest_path: &Path) -> Result<bool> {
    if !dest_path.exists() {
        return Ok(false);
    }

    let existing = fs::read_to_string(dest_path)
        .with_context(|| format!("Failed to read: {}", dest_path.display()))?;

    if !existing.contains(RUNTIME_START_MARKER) {
        return Ok(false);
    }

    let stripped = strip_markers(&existing);

    if stripped == existing {
        return Ok(false);
    }

    fs::write(dest_path, &stripped)
        .with_context(|| format!("Failed to write: {}", dest_path.display()))?;

    Ok(true)
}

pub fn apply_merge_at_runtime(corner_dir: &Path, ctx: &TemplateContext) -> Result<usize> {
    let templates = load_templates()?;
    let mut count = 0;

    for tmpl in &templates {
        let dest_rel = expand_variables(&tmpl.destination, ctx);
        let dest_path = resolve_template_destination(&dest_rel, corner_dir, ctx);

        match expand_runtime_variables_in_file(&dest_path, ctx) {
            Ok(true) => {
                if crate::verbose() {
                    println!(
                        "  {} {}",
                        "Runtime variables expanded:".bright_green(),
                        dest_path.display().to_string().bright_blue()
                    );
                }
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!(
                    "{} runtime variable expansion failed for {}: {}",
                    "Warning:".bright_yellow(),
                    dest_path.display(),
                    e
                );
            }
        }
    }

    for tmpl in templates.iter().filter(|t| t.merge_at_runtime) {
        let dest_rel = expand_variables(&tmpl.destination, ctx);
        let content = runtime_content_for_template(tmpl, ctx);
        let dest_path = resolve_template_destination(&dest_rel, corner_dir, ctx);

        match inject_runtime_content(&dest_path, &content) {
            Ok(true) => {
                count += 1;
                if crate::verbose() {
                    println!(
                        "  {} {}",
                        "Runtime merged:".bright_green(),
                        dest_path.display().to_string().bright_blue()
                    );
                }
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!(
                    "{} runtime merge failed for {}: {}",
                    "Warning:".bright_yellow(),
                    dest_path.display(),
                    e
                );
            }
        }
    }

    Ok(count)
}

pub fn strip_merge_at_runtime(corner_dir: &Path, ctx: &TemplateContext) -> Result<usize> {
    let templates = load_templates()?;
    let mut count = 0;

    for tmpl in templates.iter().filter(|t| t.merge_at_runtime) {
        let dest_rel = expand_variables(&tmpl.destination, ctx);
        let dest_path = resolve_template_destination(&dest_rel, corner_dir, ctx);

        match strip_runtime_content(&dest_path) {
            Ok(true) => {
                count += 1;
                if crate::verbose() {
                    println!(
                        "  {} {}",
                        "Runtime stripped:".bright_green(),
                        dest_path.display().to_string().bright_blue()
                    );
                }
            }
            Ok(false) => {}
            Err(e) => {
                eprintln!(
                    "{} runtime strip failed for {}: {}",
                    "Warning:".bright_yellow(),
                    dest_path.display(),
                    e
                );
            }
        }
    }

    Ok(count)
}

// ── Apply templates to a corner ──────────────────────────────────────────────

/// Merge template content into existing file content.
///
/// For each line in `new_content` of the form `KEY=VALUE`, if `KEY` is not
/// already present in `existing`, append the line. Lines that are blank or
/// don't match `KEY=VALUE` are appended if not already present verbatim.
///
/// Returns the merged content.
pub fn merge_content(existing: &str, new_content: &str) -> String {
    let mut result = existing.to_string();

    // Ensure the existing content ends with a newline before appending
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }

    for line in new_content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            // Skip blank lines during merge
            continue;
        }

        // For KEY=VALUE lines, check if the key already exists
        if let Some(eq_pos) = trimmed.find('=') {
            let key = trimmed[..eq_pos].trim();
            // Check if any existing line starts with KEY= (case-sensitive)
            let key_prefix = format!("{}=", key);
            let already_exists = existing.lines().any(|l| l.trim().starts_with(&key_prefix));
            if already_exists {
                continue;
            }
        } else {
            // Non-KEY=VALUE line: skip if already present verbatim
            if existing.lines().any(|l| l.trim() == trimmed) {
                continue;
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    result
}

/// Apply all loaded templates to a corner directory.
///
/// - Creates directories from the directory structure.
/// - Expands template variables and writes files.
/// - Merge-at-runtime templates place only an empty destination file.
/// - If `interactive` is true, warns before overwriting existing non-runtime files.
///
/// Returns the number of files written.
pub fn apply_templates(
    corner_dir: &Path,
    ctx: &TemplateContext,
    project_dir: Option<&Path>,
    interactive: bool,
) -> Result<usize> {
    apply_templates_with_mode(
        corner_dir,
        ctx,
        project_dir,
        interactive,
        TemplateApplyMode::Create,
    )
}

fn apply_templates_with_mode(
    corner_dir: &Path,
    ctx: &TemplateContext,
    project_dir: Option<&Path>,
    interactive: bool,
    mode: TemplateApplyMode,
) -> Result<usize> {
    if mode == TemplateApplyMode::Create {
        ensure_default_assets()?;
    }

    // 1. Load directory structure and create directories
    let dirs = load_directory_structure(project_dir)?;
    for dir in &dirs {
        let full_path = corner_dir.join(dir);
        fs::create_dir_all(&full_path)
            .with_context(|| format!("Failed to create directory: {}", full_path.display()))?;
    }

    // 2. Load and apply templates
    let templates = load_templates()?;

    apply_template_set(&templates, corner_dir, ctx, interactive, mode)
}

fn apply_template_set(
    templates: &[Template],
    corner_dir: &Path,
    ctx: &TemplateContext,
    interactive: bool,
    mode: TemplateApplyMode,
) -> Result<usize> {
    let mut files_written = 0;

    for tmpl in templates {
        // Expand variables in the destination path
        let dest_rel = expand_variables(&tmpl.destination, ctx);
        // Runtime-merge templates place an empty file; normal templates keep
        // content variables for replacement when the corner is opened.
        let content = if tmpl.merge_at_runtime {
            String::new()
        } else {
            expand_template_content(&filter_template_content(&tmpl.content, ctx), ctx)
        };

        // Resolve the destination: if it starts with the spocket_root, make it
        // relative to the corner dir. Otherwise treat it as relative to corner dir.
        let dest_path = resolve_template_destination(&dest_rel, corner_dir, ctx);

        // Ensure parent directory exists
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create parent directory: {}", parent.display())
            })?;
        }

        if tmpl.merge_at_runtime && mode == TemplateApplyMode::Upgrade {
            continue;
        }

        // Check for existing file
        if dest_path.exists() {
            if tmpl.merge_at_runtime {
                continue;
            }

            let existing = fs::read_to_string(&dest_path).unwrap_or_default();

            if tmpl.quiet_merge && mode == TemplateApplyMode::Create {
                // Quiet merge: add new keys/lines to existing file without overwriting
                let merged = merge_content(&existing, &content);
                if merged == existing {
                    // Nothing new to add
                    continue;
                }
                fs::write(&dest_path, &merged).with_context(|| {
                    format!("Failed to merge template into: {}", dest_path.display())
                })?;
                files_written += 1;
                println!(
                    "  {} {}",
                    "Merged:".bright_green(),
                    dest_path.display().to_string().bright_blue()
                );
                continue;
            }

            if existing == content {
                // No changes needed
                continue;
            }

            if interactive {
                // Show diff and prompt
                if !prompt_overwrite(&dest_path, &existing, &content)? {
                    println!(
                        "  {} {}",
                        "Skipped:".dimmed(),
                        dest_path.display().to_string().bright_blue()
                    );
                    continue;
                }
            } else {
                if mode == TemplateApplyMode::Create {
                    // Non-interactive creation preserves user-provided files.
                    continue;
                }
            }
        }

        if mode == TemplateApplyMode::Upgrade && dest_path.exists() {
            move_existing_to_unhoused(corner_dir, &dest_path, "template upgrade")?;
        }

        fs::write(&dest_path, &content)
            .with_context(|| format!("Failed to write template to: {}", dest_path.display()))?;
        files_written += 1;

        println!(
            "  {} {}",
            "Wrote:".bright_green(),
            dest_path.display().to_string().bright_blue()
        );
    }

    Ok(files_written)
}

fn move_existing_to_unhoused(corner_dir: &Path, path: &Path, operation: &str) -> Result<()> {
    if !path.starts_with(corner_dir) || !path.exists() {
        return Ok(());
    }

    let relative = path.strip_prefix(corner_dir).unwrap_or(path);
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut target = corner_dir.join("unhoused").join(&timestamp).join(relative);
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
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create unhoused directory: {}", parent.display())
        })?;
    }

    fs::rename(path, &target).with_context(|| {
        format!(
            "Failed to move existing corner content to unhoused: {} -> {}",
            path.display(),
            target.display()
        )
    })?;

    let log_path = corner_dir.join("unhoused.log");
    let entry = format!(
        "{}\t{}\t{}\t{}\n",
        chrono::Utc::now().to_rfc3339(),
        operation,
        path.display(),
        target.display()
    );
    let mut log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("Failed to open unhoused log: {}", log_path.display()))?;
    use std::io::Write as IoWrite;
    log.write_all(entry.as_bytes())
        .with_context(|| format!("Failed to write unhoused log: {}", log_path.display()))?;
    let _ = crate::event::append_corner_event(
        corner_dir,
        "content.unhoused",
        serde_json::json!({
            "operation": operation,
            "original_path": path,
            "unhoused_path": target,
        }),
    );

    Ok(())
}

/// Resolve a template destination path to an absolute path.
///
/// After variable expansion, the destination might be:
/// - An absolute path (e.g. `/Users/.../corner/AGENTS.md`) → use as-is
/// - A relative path (e.g. `.github/copilot-instructions.md`) → relative to corner_dir
fn resolve_template_destination(dest: &str, corner_dir: &Path, _ctx: &TemplateContext) -> PathBuf {
    let path = PathBuf::from(dest);
    if path.is_absolute() {
        path
    } else {
        corner_dir.join(path)
    }
}

// ── Diff & prompt ────────────────────────────────────────────────────────────

/// Display a simple line-based diff between two strings and prompt the user
/// to confirm overwriting.
pub fn prompt_overwrite(path: &Path, existing: &str, proposed: &str) -> Result<bool> {
    println!();
    println!(
        "{}",
        format!(
            "File already exists with different content: {}",
            path.display()
        )
        .bright_yellow()
    );

    display_diff(existing, proposed);

    println!();
    print!("{} ", "Overwrite this file? [y/N]:".bright_white());
    use std::io::{self, Write as IoWrite};
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    Ok(input == "y" || input == "yes")
}

/// Display a simple unified-style diff between `old` and `new`.
pub fn display_diff(old: &str, new: &str) {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    // Simple line-by-line comparison (not a proper diff algorithm, but good
    // enough for showing meaningful changes to the user).
    let max_lines = old_lines.len().max(new_lines.len());
    let mut has_diff = false;

    for i in 0..max_lines {
        let old_line = old_lines.get(i).copied();
        let new_line = new_lines.get(i).copied();

        match (old_line, new_line) {
            (Some(o), Some(n)) if o == n => {
                // identical — skip unless near a change
            }
            (Some(o), Some(n)) => {
                if !has_diff {
                    println!("  {}", "--- existing".red());
                    println!("  {}", "+++ proposed".green());
                    has_diff = true;
                }
                println!("  {}", format!("- {}", o).red());
                println!("  {}", format!("+ {}", n).green());
            }
            (Some(o), None) => {
                if !has_diff {
                    println!("  {}", "--- existing".red());
                    println!("  {}", "+++ proposed".green());
                    has_diff = true;
                }
                println!("  {}", format!("- {}", o).red());
            }
            (None, Some(n)) => {
                if !has_diff {
                    println!("  {}", "--- existing".red());
                    println!("  {}", "+++ proposed".green());
                    has_diff = true;
                }
                println!("  {}", format!("+ {}", n).green());
            }
            (None, None) => break,
        }
    }

    if !has_diff {
        println!("  {}", "(no visible differences)".dimmed());
    }
}

// ── Upgrade ──────────────────────────────────────────────────────────────────

/// Upgrade an existing corner to match the current templates.
///
/// This is called by `spocket -u <path>`. It does NOT open the workspace;
/// it resets non-runtime template destinations to match the templates while
/// preserving merge-at-runtime destinations for runtime injection.
pub fn upgrade_corner(corner_dir: &Path) -> Result<()> {
    // Validate the corner directory exists and has a manifest
    if !corner_dir.exists() {
        return Err(anyhow!(
            "Corner directory does not exist: {}",
            corner_dir.display()
        ));
    }

    let manifest_path = corner_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(anyhow!(
            "No manifest.json found in {}. Is this a valid corner?",
            corner_dir.display()
        ));
    }

    // Load manifest to get core_paths
    let manifest = crate::manifest::Manifest::load(corner_dir)?
        .ok_or_else(|| anyhow!("Failed to load manifest from {}", corner_dir.display()))?;

    let spocket_name = corner_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let project_root = manifest
        .core_paths
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("<unknown>"));

    let global_obs = global_observations_dir().unwrap_or_else(|_| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(crate::branding::PRIMARY_REGISTRY_DIRNAME)
            .join("observations")
    });

    let config_root = safe_pocket_config_dir().unwrap_or_else(|_| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(crate::branding::PRIMARY_CONFIG_DIRNAME)
    });

    let ctx = TemplateContext {
        spocket_root: corner_dir.to_path_buf(),
        project_root: project_root.clone(),
        spocket_name,
        global_observations_path: global_obs,
        config_root,
    };

    println!(
        "{} {}",
        "Upgrading corner:".bright_white().bold(),
        corner_dir.display().to_string().bright_yellow()
    );

    // Upgrade force-resets non-runtime templates while preserving user-authored
    // content in merge-at-runtime destinations.
    let files_written = apply_templates_with_mode(
        corner_dir,
        &ctx,
        Some(&project_root),
        false,
        TemplateApplyMode::Upgrade,
    )?;

    let runtime_updated = apply_merge_at_runtime(corner_dir, &ctx)?;
    let total_updated = files_written + runtime_updated;

    if total_updated == 0 {
        println!(
            "{}",
            "Corner is already up to date with templates.".bright_green()
        );
    } else {
        println!(
            "{} {} file(s) written.",
            "Upgrade complete:".bright_green(),
            total_updated.to_string().bright_yellow()
        );
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // ── parse_destination_directive ──────────────────────────────────────

    #[test]
    fn test_parse_destination_with_colon() {
        let result = parse_destination_directive(
            "#SPOCKET_TEMPLATE_DESTINATION: .github/copilot-instructions.md",
        );
        assert_eq!(result, Some(".github/copilot-instructions.md".to_string()));
    }

    #[test]
    fn test_parse_destination_without_colon() {
        let result = parse_destination_directive(
            "#SPOCKET_TEMPLATE_DESTINATION .github/prompts/talkLikeACat.md",
        );
        assert_eq!(result, Some(".github/prompts/talkLikeACat.md".to_string()));
    }

    #[test]
    fn test_parse_destination_with_template_variable() {
        let result = parse_destination_directive(
            "#SPOCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/AGENTS.md",
        );
        assert_eq!(result, Some("{{SPOCKET_ROOT}}/AGENTS.md".to_string()));
    }

    #[test]
    fn test_parse_destination_empty_returns_none() {
        let result = parse_destination_directive("#SPOCKET_TEMPLATE_DESTINATION:");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_destination_no_prefix() {
        let result = parse_destination_directive("some random line");
        assert_eq!(result, None);
    }

    // ── expand_variables ────────────────────────────────────────────────

    fn make_ctx(spocket_root: &str, project_root: &str, name: &str) -> TemplateContext {
        TemplateContext {
            spocket_root: PathBuf::from(spocket_root),
            project_root: PathBuf::from(project_root),
            spocket_name: name.to_string(),
            global_observations_path: PathBuf::from("/global/observations"),
            config_root: PathBuf::from("/home/user/.config/safe_pocket"),
        }
    }

    #[test]
    fn test_expand_variables() {
        let ctx = make_ctx(
            "/home/user/.safe_pocket/abc123",
            "/home/user/project",
            "abc123",
        );

        let input = "Root: {{SPOCKET_ROOT}}\nProject: {{PROJECT_ROOT}}\nName: {{SPOCKET_NAME}}";
        let result = expand_variables(input, &ctx);

        assert_eq!(
            result,
            "Root: /home/user/.safe_pocket/abc123\nProject: /home/user/project\nName: abc123"
        );
    }

    #[test]
    fn test_expand_variables_supports_corner_aliases() {
        let ctx = make_ctx("/home/user/.corner/abc123", "/home/user/project", "abc123");
        let input = "Root: {{CORNER_ROOT}}\nName: {{CORNER_NAME}}\nCfg: {{CORNER_CONFIG_ROOT}}";

        let output = expand_variables(input, &ctx);

        assert_eq!(
            output,
            "Root: /home/user/.corner/abc123\nName: abc123\nCfg: /home/user/.config/safe_pocket"
        );
    }

    #[test]
    fn test_expand_variables_global_observations() {
        let ctx = make_ctx("/sp", "/pr", "name");

        let input = "Obs: {{GLOBAL_OBSERVATIONS_PATH}}";
        assert_eq!(expand_variables(input, &ctx), "Obs: /global/observations");
    }

    #[test]
    fn test_expand_variables_no_variables() {
        let ctx = make_ctx("/x", "/y", "z");

        let input = "No variables here";
        assert_eq!(expand_variables(input, &ctx), "No variables here");
    }

    #[test]
    fn test_expand_variables_multiple_occurrences() {
        let ctx = make_ctx("/sp", "/pr", "name");

        let input = "{{SPOCKET_ROOT}} and {{SPOCKET_ROOT}} again";
        assert_eq!(expand_variables(input, &ctx), "/sp and /sp again");
    }

    #[test]
    fn test_merge_templates_by_destination_combines_duplicates() {
        let templates = vec![
            Template {
                destination: "AGENTS.md".to_string(),
                content: "First".to_string(),
                quiet_merge: false,
                merge_at_runtime: false,
                source_path: PathBuf::from("first.md"),
            },
            Template {
                destination: "README.md".to_string(),
                content: "Read me\n".to_string(),
                quiet_merge: false,
                merge_at_runtime: false,
                source_path: PathBuf::from("readme.md"),
            },
            Template {
                destination: "AGENTS.md".to_string(),
                content: "Second\n".to_string(),
                quiet_merge: true,
                merge_at_runtime: true,
                source_path: PathBuf::from("second.md"),
            },
        ];

        let merged = merge_templates_by_destination(templates);
        assert_eq!(merged.len(), 2);

        let agents = merged
            .iter()
            .find(|tmpl| tmpl.destination == "AGENTS.md")
            .unwrap();
        assert_eq!(agents.content, "First\nSecond\n");
        assert!(agents.quiet_merge);
        assert!(agents.merge_at_runtime);
        assert_eq!(agents.source_path, PathBuf::from("first.md"));
    }

    // ── parse_template ──────────────────────────────────────────────────

    #[test]
    fn test_parse_template_basic() {
        let dir = std::env::temp_dir().join("spocket_test_parse_basic");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("test.md");
        fs::write(
            &file,
            "#SPOCKET_TEMPLATE_DESTINATION: .github/test.md\nHello world\nSecond line\n",
        )
        .unwrap();

        let tmpl = parse_template(&file).unwrap().remove(0);
        assert_eq!(tmpl.destination, ".github/test.md");
        assert_eq!(tmpl.content, "Hello world\nSecond line\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_template_strips_spocket_lines() {
        let dir = std::env::temp_dir().join("spocket_test_parse_strip");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("test.md");
        fs::write(
            &file,
            "#SPOCKET_TEMPLATE_DESTINATION: test.md\n\
             #SPOCKET_SOME_OTHER_DIRECTIVE: value\n\
             Content line 1\n\
             Content line 2\n",
        )
        .unwrap();

        let tmpl = parse_template(&file).unwrap().remove(0);
        assert_eq!(tmpl.destination, "test.md");
        assert_eq!(tmpl.content, "Content line 1\nContent line 2\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_template_empty_file_errors() {
        let dir = std::env::temp_dir().join("spocket_test_parse_empty");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("empty.md");
        fs::write(&file, "").unwrap();

        assert!(parse_template(&file).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_template_missing_directive_errors() {
        let dir = std::env::temp_dir().join("spocket_test_parse_no_directive");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("bad.md");
        fs::write(&file, "Just some content\nNo directive\n").unwrap();

        assert!(parse_template(&file).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_template_midfile_destinations() {
        let dir = std::env::temp_dir().join("spocket_test_parse_midfile");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("multi.md");
        fs::write(
            &file,
            "#SPOCKET_TEMPLATE_DESTINATION file1.md\n\
             bow wow wow\n\
             \n\
             more stuff\n\
             \n\
             #SPOCKET_TEMPLATE_DESTINATION file2.md\n\
             meow meow meow\n\
             \n\
             #SPOCKET_TEMPLATE_DESTINATION file1.md\n\
             oh wow this is more!\n",
        )
        .unwrap();

        let templates = parse_template(&file).unwrap();
        assert_eq!(templates.len(), 3);
        assert_eq!(templates[0].destination, "file1.md");
        assert_eq!(templates[1].destination, "file2.md");
        assert_eq!(templates[2].destination, "file1.md");

        // After merge, file1 concatenates both of its blocks; file2 stands alone.
        let merged = merge_templates_by_destination(templates);
        let by_dest: std::collections::HashMap<_, _> = merged
            .into_iter()
            .map(|t| (t.destination.clone(), t.content))
            .collect();
        assert_eq!(
            by_dest.get("file1.md").unwrap(),
            "bow wow wow\n\nmore stuff\n\noh wow this is more!\n"
        );
        assert_eq!(by_dest.get("file2.md").unwrap(), "meow meow meow\n\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_template_midfile_per_block_flags() {
        let dir = std::env::temp_dir().join("spocket_test_parse_midfile_flags");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("flags.md");
        fs::write(
            &file,
            "#SPOCKET_TEMPLATE_DESTINATION a.md\n\
             #SPOCKET_QUIET_MERGE\n\
             alpha\n\
             #SPOCKET_TEMPLATE_DESTINATION b.md\n\
             beta\n",
        )
        .unwrap();

        let templates = parse_template(&file).unwrap();
        assert_eq!(templates.len(), 2);
        assert!(templates[0].quiet_merge);
        assert_eq!(templates[0].content, "alpha\n");
        assert!(!templates[1].quiet_merge);
        assert_eq!(templates[1].content, "beta\n");

        let _ = fs::remove_dir_all(&dir);
    }

    // ── parse_directory_structure ────────────────────────────────────────

    #[test]
    fn test_parse_directory_structure_basic() {
        let content = ".github\n- prompts\n- skills\nFEATURES\nOBSERVATIONS\n";
        let dirs = parse_directory_structure(content).unwrap();

        assert_eq!(
            dirs,
            vec![
                PathBuf::from(".github"),
                PathBuf::from(".github/prompts"),
                PathBuf::from(".github/skills"),
                PathBuf::from("FEATURES"),
                PathBuf::from("OBSERVATIONS"),
            ]
        );
    }

    #[test]
    fn test_parse_directory_structure_nested() {
        let content = "FEATURES\n- SomeFolder\n- AnotherFolder\n- - SomeSubfolder\n";
        let dirs = parse_directory_structure(content).unwrap();

        assert_eq!(
            dirs,
            vec![
                PathBuf::from("FEATURES"),
                PathBuf::from("FEATURES/SomeFolder"),
                PathBuf::from("FEATURES/AnotherFolder"),
                PathBuf::from("FEATURES/AnotherFolder/SomeSubfolder"),
            ]
        );
    }

    #[test]
    fn test_parse_directory_structure_empty() {
        let dirs = parse_directory_structure("").unwrap();
        assert!(dirs.is_empty());
    }

    #[test]
    fn test_parse_directory_structure_orphan_child_errors() {
        let content = "- orphan_child\n";
        let result = parse_directory_structure(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_directory_structure_skips_blank_lines() {
        let content = ".github\n\n- prompts\n\nFEATURES\n";
        let dirs = parse_directory_structure(content).unwrap();

        assert_eq!(
            dirs,
            vec![
                PathBuf::from(".github"),
                PathBuf::from(".github/prompts"),
                PathBuf::from("FEATURES"),
            ]
        );
    }

    // ── resolve_template_destination ────────────────────────────────────

    #[test]
    fn test_resolve_destination_relative() {
        let ctx = make_ctx("/corner", "/project", "hash");
        let corner = PathBuf::from("/corner");

        let result = resolve_template_destination(".github/test.md", &corner, &ctx);
        assert_eq!(result, PathBuf::from("/corner/.github/test.md"));
    }

    #[test]
    fn test_resolve_destination_absolute() {
        let ctx = make_ctx("/corner", "/project", "hash");
        let corner = PathBuf::from("/corner");

        let result = resolve_template_destination("/corner/AGENTS.md", &corner, &ctx);
        assert_eq!(result, PathBuf::from("/corner/AGENTS.md"));
    }

    // ── apply_templates (integration) ───────────────────────────────────

    #[test]
    fn test_apply_templates_creates_dirs_and_files() {
        // This test creates a mini template setup in a temp dir and verifies
        // that apply_templates creates the expected structure.
        let base = std::env::temp_dir().join("spocket_test_apply");
        let _ = fs::remove_dir_all(&base);

        let corner_dir = base.join("corner");
        let config_dir = base.join("config");
        let tmpl_dir = config_dir.join("templates");
        fs::create_dir_all(&tmpl_dir).unwrap();
        fs::create_dir_all(&corner_dir).unwrap();

        // Write a template
        fs::write(
            tmpl_dir.join("test.md"),
            "#SPOCKET_TEMPLATE_DESTINATION: subdir/test.md\nHello {{SPOCKET_NAME}}\n",
        )
        .unwrap();

        // Write directory structure
        fs::write(config_dir.join("directory_structure.md"), "subdir\nother\n").unwrap();

        let ctx = make_ctx(
            &corner_dir.to_string_lossy(),
            &base.join("project").to_string_lossy(),
            "testhash",
        );

        // We can't easily test apply_templates directly because it reads from
        // the real config dir. This test verifies the building blocks work.
        // The integration is tested by the full build + manual testing.

        // Verify directory parsing
        let dirs = parse_directory_structure("subdir\nother\n").unwrap();
        assert_eq!(dirs.len(), 2);

        // Verify template parsing
        let tmpl = parse_template(&tmpl_dir.join("test.md")).unwrap().remove(0);
        assert_eq!(tmpl.destination, "subdir/test.md");

        // Verify variable expansion
        let expanded = expand_variables(&tmpl.content, &ctx);
        assert_eq!(expanded, "Hello testhash\n");

        let _ = fs::remove_dir_all(&base);
    }

    // ── display_diff (smoke test) ───────────────────────────────────────

    #[test]
    fn test_display_diff_identical() {
        // Should not panic
        display_diff("same\ncontent\n", "same\ncontent\n");
    }

    #[test]
    fn test_display_diff_different() {
        // Should not panic
        display_diff("old line\n", "new line\n");
    }

    // ── parse_directory_line ────────────────────────────────────────────

    #[test]
    fn test_parse_directory_line_no_indent() {
        let (depth, name) = parse_directory_line("FEATURES");
        assert_eq!(depth, 0);
        assert_eq!(name, "FEATURES");
    }

    #[test]
    fn test_parse_directory_line_single_indent() {
        let (depth, name) = parse_directory_line("- prompts");
        assert_eq!(depth, 1);
        assert_eq!(name, "prompts");
    }

    #[test]
    fn test_parse_directory_line_double_indent() {
        let (depth, name) = parse_directory_line("- - subfolder");
        assert_eq!(depth, 2);
        assert_eq!(name, "subfolder");
    }

    #[test]
    fn test_parse_directory_line_triple_indent() {
        let (depth, name) = parse_directory_line("- - - deep");
        assert_eq!(depth, 3);
        assert_eq!(name, "deep");
    }

    // ── #SPOCKET_QUIET_MERGE ────────────────────────────────────────────

    #[test]
    fn test_parse_template_quiet_merge_flag() {
        let dir = std::env::temp_dir().join("spocket_test_quiet_merge_flag");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("env.md");
        fs::write(
            &file,
            "#SPOCKET_TEMPLATE_DESTINATION: .env\n#SPOCKET_QUIET_MERGE\n\nMY_KEY=value\n",
        )
        .unwrap();

        let tmpl = parse_template(&file).unwrap().remove(0);
        assert!(tmpl.quiet_merge, "quiet_merge should be true");
        assert_eq!(tmpl.content, "MY_KEY=value\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_template_no_quiet_merge_flag() {
        let dir = std::env::temp_dir().join("spocket_test_no_quiet_merge");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("regular.md");
        fs::write(
            &file,
            "#SPOCKET_TEMPLATE_DESTINATION: regular.txt\nSome content\n",
        )
        .unwrap();

        let tmpl = parse_template(&file).unwrap().remove(0);
        assert!(!tmpl.quiet_merge, "quiet_merge should be false");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_parse_template_merge_at_runtime_flag() {
        let dir = std::env::temp_dir().join("spocket_test_merge_at_runtime_flag");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("agents.md");
        fs::write(
            &file,
            "#SPOCKET_TEMPLATE_DESTINATION: AGENTS.md\n\
             #SPOCKET_MERGE_AT_RUNTIME\n\nRuntime content\n",
        )
        .unwrap();

        let tmpl = parse_template(&file).unwrap().remove(0);
        assert!(tmpl.merge_at_runtime, "merge_at_runtime should be true");
        assert_eq!(tmpl.content, "Runtime content\n");

        let _ = fs::remove_dir_all(&dir);
    }

    // ── runtime merge ───────────────────────────────────────────────────

    #[test]
    fn test_runtime_injection_and_strip_preserves_user_content() {
        let dir = std::env::temp_dir().join("spocket_test_runtime_preserves_user_content");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("AGENTS.md");
        fs::write(&file, "Meow like a cat\n").unwrap();

        assert!(inject_runtime_content(&file, "Bark like a dog\n").unwrap());
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            "Meow like a cat\n#SPOCKET_RUNTIME_CONTENT_START\nBark like a dog\n#SPOCKET_RUNTIME_CONTENT_END\n"
        );

        assert!(strip_runtime_content(&file).unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), "Meow like a cat\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_runtime_variable_expansion_skips_spocket_directives() {
        let ctx = make_ctx("/corner", "/project", "hash");
        let input = "Path: {{SPOCKET_ROOT}}\n#SPOCKET_NOTE: {{SPOCKET_ROOT}}\n";

        assert_eq!(
            expand_runtime_variables_in_content(input, &ctx),
            "Path: /corner\n#SPOCKET_NOTE: {{SPOCKET_ROOT}}\n"
        );
    }

    #[test]
    fn test_filter_template_content_removes_beads_dir() {
        let ctx = make_ctx("/corner", "/project", "hash");
        let content = "SPOCKET_ROOT={{SPOCKET_ROOT}}\nBEADS_DIR={{SPOCKET_ROOT}}/.beads\n";

        assert_eq!(
            filter_template_content(content, &ctx),
            "SPOCKET_ROOT={{SPOCKET_ROOT}}\n"
        );
    }

    #[test]
    fn test_runtime_content_for_agents_includes_task_block() {
        let ctx = make_ctx("/corner", "/project", "hash");
        let tmpl = Template {
            destination: "{{SPOCKET_ROOT}}/AGENTS.md".to_string(),
            content: "Base runtime\n".to_string(),
            quiet_merge: false,
            merge_at_runtime: true,
            source_path: PathBuf::from("/tmp/AGENTS.md"),
        };

        let content = runtime_content_for_template(&tmpl, &ctx);
        assert!(content.contains("Base runtime"));
        assert!(content.contains("<!-- BEGIN SPOCKET TASK INTEGRATION -->"));
        assert!(content.contains("corner task"));
    }

    #[test]
    fn test_create_mode_places_empty_merge_at_runtime_destination() {
        let dir = std::env::temp_dir().join("spocket_test_create_places_empty_runtime");
        let _ = fs::remove_dir_all(&dir);
        let corner_dir = dir.join("corner");
        fs::create_dir_all(&corner_dir).unwrap();

        let templates = vec![Template {
            destination: "AGENTS.md".to_string(),
            content: "Runtime content\n".to_string(),
            quiet_merge: false,
            merge_at_runtime: true,
            source_path: dir.join("template.md"),
        }];
        let ctx = make_ctx(&corner_dir.to_string_lossy(), "/project", "hash");

        let written = apply_template_set(
            &templates,
            &corner_dir,
            &ctx,
            false,
            TemplateApplyMode::Create,
        )
        .unwrap();

        assert_eq!(written, 1);
        assert_eq!(
            fs::read_to_string(corner_dir.join("AGENTS.md")).unwrap(),
            ""
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_create_mode_does_not_overwrite_existing_runtime_destination() {
        let dir = std::env::temp_dir().join("spocket_test_create_keeps_runtime_destination");
        let _ = fs::remove_dir_all(&dir);
        let corner_dir = dir.join("corner");
        fs::create_dir_all(&corner_dir).unwrap();
        fs::write(corner_dir.join("AGENTS.md"), "User content\n").unwrap();

        let templates = vec![Template {
            destination: "AGENTS.md".to_string(),
            content: "Runtime content\n".to_string(),
            quiet_merge: false,
            merge_at_runtime: true,
            source_path: dir.join("template.md"),
        }];
        let ctx = make_ctx(&corner_dir.to_string_lossy(), "/project", "hash");

        let written = apply_template_set(
            &templates,
            &corner_dir,
            &ctx,
            false,
            TemplateApplyMode::Create,
        )
        .unwrap();

        assert_eq!(written, 0);
        assert_eq!(
            fs::read_to_string(corner_dir.join("AGENTS.md")).unwrap(),
            "User content\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_mode_overwrites_non_runtime_and_preserves_runtime() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_template_rules");
        let _ = fs::remove_dir_all(&dir);
        let corner_dir = dir.join("corner");
        fs::create_dir_all(&corner_dir).unwrap();
        fs::write(corner_dir.join("normal.md"), "User edit\n").unwrap();
        fs::write(corner_dir.join("AGENTS.md"), "User instructions\n").unwrap();

        let templates = vec![
            Template {
                destination: "normal.md".to_string(),
                content: "Template reset\n".to_string(),
                quiet_merge: false,
                merge_at_runtime: false,
                source_path: dir.join("normal-template.md"),
            },
            Template {
                destination: "AGENTS.md".to_string(),
                content: "Runtime content\n".to_string(),
                quiet_merge: false,
                merge_at_runtime: true,
                source_path: dir.join("agents-template.md"),
            },
        ];
        let ctx = make_ctx(&corner_dir.to_string_lossy(), "/project", "hash");

        let written = apply_template_set(
            &templates,
            &corner_dir,
            &ctx,
            false,
            TemplateApplyMode::Upgrade,
        )
        .unwrap();

        assert_eq!(written, 1);
        assert_eq!(
            fs::read_to_string(corner_dir.join("normal.md")).unwrap(),
            "Template reset\n"
        );
        assert_eq!(
            fs::read_to_string(corner_dir.join("AGENTS.md")).unwrap(),
            "User instructions\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_mode_resets_quiet_merge_template() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_resets_quiet_merge");
        let _ = fs::remove_dir_all(&dir);
        let corner_dir = dir.join("corner");
        fs::create_dir_all(&corner_dir).unwrap();
        fs::write(corner_dir.join(".env"), "USER_KEY=custom\n").unwrap();

        let templates = vec![Template {
            destination: ".env".to_string(),
            content: "TEMPLATE_KEY=value\n".to_string(),
            quiet_merge: true,
            merge_at_runtime: false,
            source_path: dir.join("env-template.md"),
        }];
        let ctx = make_ctx(&corner_dir.to_string_lossy(), "/project", "hash");

        apply_template_set(
            &templates,
            &corner_dir,
            &ctx,
            false,
            TemplateApplyMode::Upgrade,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(corner_dir.join(".env")).unwrap(),
            "TEMPLATE_KEY=value\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_create_mode_filters_beads_dir_from_env() {
        let dir = std::env::temp_dir().join("spocket_test_create_filters_beads_env");
        let _ = fs::remove_dir_all(&dir);
        let corner_dir = dir.join("corner");
        fs::create_dir_all(&corner_dir).unwrap();

        let templates = vec![Template {
            destination: ".env".to_string(),
            content: "CORNER_ROOT={{CORNER_ROOT}}\nSPOCKET_ROOT={{CORNER_ROOT}}\nBEADS_DIR={{CORNER_ROOT}}/.beads\n"
                .to_string(),
            quiet_merge: true,
            merge_at_runtime: false,
            source_path: dir.join("env-template.md"),
        }];
        let ctx = make_ctx(&corner_dir.to_string_lossy(), "/project", "hash");

        apply_template_set(
            &templates,
            &corner_dir,
            &ctx,
            false,
            TemplateApplyMode::Create,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(corner_dir.join(".env")).unwrap(),
            format!(
                "CORNER_ROOT={}\nSPOCKET_ROOT={}\n",
                corner_dir.display(),
                corner_dir.display()
            )
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_mode_expands_template_variables_in_env_files() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_expands_env_values");
        let _ = fs::remove_dir_all(&dir);
        let corner_dir = dir.join("corner");
        let project_dir = dir.join("project");
        fs::create_dir_all(&corner_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let templates = vec![
            Template {
                destination: "{{PROJECT_ROOT}}/.env".to_string(),
                content: "CORNER_ROOT={{CORNER_ROOT}}\nSPOCKET_ROOT={{CORNER_ROOT}}\n"
                    .to_string(),
                quiet_merge: true,
                merge_at_runtime: false,
                source_path: dir.join("project-env-template.md"),
            },
            Template {
                destination: "{{SPOCKET_ROOT}}/.env".to_string(),
                content: "PROJECT_ROOT={{PROJECT_ROOT}}\nCORNER_ROOT={{CORNER_ROOT}}\nSPOCKET_ROOT={{CORNER_ROOT}}\n"
                    .to_string(),
                quiet_merge: true,
                merge_at_runtime: false,
                source_path: dir.join("safe-pocket-env-template.md"),
            },
        ];
        let ctx = make_ctx(
            &corner_dir.to_string_lossy(),
            &project_dir.to_string_lossy(),
            "hash",
        );

        apply_template_set(
            &templates,
            &corner_dir,
            &ctx,
            false,
            TemplateApplyMode::Upgrade,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(project_dir.join(".env")).unwrap(),
            format!(
                "CORNER_ROOT={}\nSPOCKET_ROOT={}\n",
                corner_dir.display(),
                corner_dir.display()
            )
        );
        assert_eq!(
            fs::read_to_string(corner_dir.join(".env")).unwrap(),
            format!(
                "PROJECT_ROOT={}\nCORNER_ROOT={}\nSPOCKET_ROOT={}\n",
                project_dir.display(),
                corner_dir.display(),
                corner_dir.display()
            )
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_runtime_strips_legacy_beads_and_injects_task_block() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_runtime_strips_legacy_beads");
        let _ = fs::remove_dir_all(&dir);
        let corner_dir = dir.join("corner");
        fs::create_dir_all(&corner_dir).unwrap();

        let file = corner_dir.join("AGENTS.md");
        fs::write(
            &file,
            format!(
                "{begin}\nlegacy beads guidance\n{end_legacy}\n{start}\nOld runtime\n{end}\n",
                begin = LEGACY_BEADS_BEGIN_MARKER,
                end_legacy = LEGACY_BEADS_END_MARKER,
                start = RUNTIME_START_MARKER,
                end = RUNTIME_END_MARKER
            ),
        )
        .unwrap();

        let tmpl = Template {
            destination: "{{SPOCKET_ROOT}}/AGENTS.md".to_string(),
            content: "All agents MUST obey {{SPOCKET_ROOT}}/.github/copilot-instructions.md\n"
                .to_string(),
            quiet_merge: false,
            merge_at_runtime: true,
            source_path: dir.join("agents-template.md"),
        };
        let ctx = make_ctx(&corner_dir.to_string_lossy(), "/project", "hash");

        let content = runtime_content_for_template(&tmpl, &ctx);
        inject_runtime_content(&file, &content).unwrap();

        let updated = fs::read_to_string(&file).unwrap();
        // The legacy beads block must be gone entirely.
        assert!(!updated.contains(LEGACY_BEADS_BEGIN_MARKER));
        // The new task block lives inside the runtime markers.
        let start_idx = updated.find(RUNTIME_START_MARKER).unwrap();
        let runtime_block = &updated[start_idx..];
        assert!(runtime_block.contains("<!-- BEGIN SPOCKET TASK INTEGRATION -->"));

        let _ = fs::remove_dir_all(&dir);
    }

    // ── merge_content ───────────────────────────────────────────────────

    #[test]
    fn test_merge_content_adds_new_key() {
        let existing = "EXISTING_KEY=old_value\n";
        let new_content = "NEW_KEY=new_value\n";
        let result = merge_content(existing, new_content);
        assert!(result.contains("EXISTING_KEY=old_value"));
        assert!(result.contains("NEW_KEY=new_value"));
    }

    #[test]
    fn test_merge_content_skips_existing_key() {
        let existing = "MY_KEY=original\n";
        let new_content = "MY_KEY=replacement\n";
        let result = merge_content(existing, new_content);
        assert!(result.contains("MY_KEY=original"));
        // The new value should NOT replace the existing one
        assert!(!result.contains("MY_KEY=replacement"));
    }

    #[test]
    fn test_merge_content_into_empty() {
        let existing = "";
        let new_content = "KEY=value\n";
        let result = merge_content(existing, new_content);
        assert!(result.contains("KEY=value"));
    }

    #[test]
    fn test_merge_content_skips_blank_lines() {
        let existing = "A=1\n";
        let new_content = "\nB=2\n\nC=3\n";
        let result = merge_content(existing, new_content);
        assert!(result.contains("A=1"));
        assert!(result.contains("B=2"));
        assert!(result.contains("C=3"));
        // Should not have double blank lines added
        assert!(!result.contains("\n\n\n"));
    }

    // ── embedded templates + install_embedded_templates ─────────────────────

    #[test]
    fn test_embedded_templates_populated() {
        // build.rs must have embedded the whole src/templates tree.
        assert!(
            EMBEDDED_TEMPLATES.len() >= 5,
            "expected the embedded template table to be populated, got {}",
            EMBEDDED_TEMPLATES.len()
        );
        let rels: Vec<&str> = EMBEDDED_TEMPLATES.iter().map(|(r, _)| *r).collect();
        assert!(rels.contains(&"AGENTS.md"), "AGENTS.md should be embedded");
        assert!(
            rels.contains(&"directory_structure.md"),
            "directory_structure.md should be embedded"
        );
        assert!(
            rels.iter().any(|r| r.starts_with("agents/")),
            "agent definitions should be embedded under agents/"
        );
    }

    #[test]
    fn test_install_interprets_config_and_stages_corner_templates() {
        let base = std::env::temp_dir().join("spocket_test_install_embedded");
        let _ = fs::remove_dir_all(&base);
        let config_dir = base.join("config");
        let registry_root = base.join("registry");
        fs::create_dir_all(&config_dir).unwrap();
        fs::create_dir_all(&registry_root).unwrap();

        install_embedded_templates(&config_dir, &registry_root).unwrap();

        // INSTALL_DESTINATION: directory_structure.md is placed at the config
        // root as directory_structure.yaml, with the INSTALL directive stripped
        // (and no leading blank line) — and the raw .md is NOT mirrored.
        let dir_struct = config_dir.join("directory_structure.yaml");
        assert!(
            dir_struct.exists(),
            "directory_structure.yaml should be installed"
        );
        let ds = fs::read_to_string(&dir_struct).unwrap();
        assert!(
            !ds.contains("#SPOCKET_INSTALL_DESTINATION"),
            "INSTALL directive must be stripped from the placed file"
        );
        assert!(
            !ds.starts_with('\n'),
            "placed config asset must not start with a blank line"
        );
        assert!(
            !config_dir.join("templates/directory_structure.md").exists(),
            "a file with INSTALL_DESTINATION must not be mirrored into templates/"
        );

        // feature_tags.yaml and the conversation tag are installed into the
        // config tree (not templates/).
        assert!(
            config_dir.join("feature_tags.yaml").exists(),
            "feature_tags.yaml should be installed to the config root"
        );
        assert!(
            config_dir
                .join("feature_tags/conversation.feature.tag.yaml")
                .exists(),
            "conversation feature tag should be installed under feature_tags/"
        );

        // Default mirror: AGENTS.md has no INSTALL_DESTINATION, so it is mirrored
        // verbatim (TEMPLATE_DESTINATION preserved) for the runtime loader.
        let staged_agents = config_dir.join("templates/AGENTS.md");
        assert!(
            staged_agents.exists(),
            "AGENTS.md should be mirrored to templates/"
        );
        assert!(
            fs::read_to_string(&staged_agents)
                .unwrap()
                .contains("#SPOCKET_TEMPLATE_DESTINATION"),
            "mirrored template must keep its TEMPLATE_DESTINATION directive"
        );

        // Directive-less unified agents are mirrored under templates/agents/.
        assert!(
            config_dir.join("templates/agents/builder.md").exists(),
            "agent definitions should be mirrored under templates/agents/"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_install_is_idempotent_and_non_destructive() {
        let base = std::env::temp_dir().join("spocket_test_install_idempotent");
        let _ = fs::remove_dir_all(&base);
        let config_dir = base.join("config");
        let registry_root = base.join("registry");
        fs::create_dir_all(&config_dir).unwrap();
        fs::create_dir_all(&registry_root).unwrap();

        // Pre-seed a user-customised file; it must never be overwritten.
        fs::create_dir_all(config_dir.join("templates")).unwrap();
        let user_agents = config_dir.join("templates/AGENTS.md");
        fs::write(&user_agents, "MY CUSTOM AGENTS\n").unwrap();

        install_embedded_templates(&config_dir, &registry_root).unwrap();
        // Second run must succeed and remain a no-op.
        install_embedded_templates(&config_dir, &registry_root).unwrap();

        assert_eq!(
            fs::read_to_string(&user_agents).unwrap(),
            "MY CUSTOM AGENTS\n",
            "existing user file must be preserved"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_expand_install_roots_leaves_corner_vars_literal() {
        let out = expand_install_roots(
            "cfg={{CORNER_CONFIG_ROOT}} reg={{CORNER_REGISTRY_ROOT}} pkt={{CORNER_ROOT}}",
            Path::new("/cfg"),
            Path::new("/reg"),
        );
        assert_eq!(out, "cfg=/cfg reg=/reg pkt={{CORNER_ROOT}}");
    }

    #[test]
    fn test_parse_install_destination_directive() {
        assert_eq!(
            parse_install_destination_directive("#SPOCKET_INSTALL_DESTINATION: /a/b.yaml"),
            Some("/a/b.yaml".to_string())
        );
        assert_eq!(
            parse_install_destination_directive("#SPOCKET_INSTALL_DESTINATION /a/b.yaml"),
            Some("/a/b.yaml".to_string())
        );
        // Must not match the runtime directive or arbitrary lines.
        assert_eq!(
            parse_install_destination_directive("#SPOCKET_TEMPLATE_DESTINATION: /x"),
            None
        );
        assert_eq!(parse_install_destination_directive("plain text"), None);
    }

    #[test]
    fn test_strip_install_directives_keeps_template_directive() {
        let content = "#SPOCKET_INSTALL_DESTINATION: {{SPOCKET_CONFIG_ROOT}}/templates/x.md\n\
                       #SPOCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/.github/x.md\n\
                       body line\n";
        let out = strip_install_directives(content);
        assert!(
            !out.contains("#SPOCKET_INSTALL_DESTINATION"),
            "INSTALL directive must be removed"
        );
        assert!(
            out.contains("#SPOCKET_TEMPLATE_DESTINATION"),
            "TEMPLATE directive must be preserved"
        );
        assert!(out.contains("body line"));
    }

    #[test]
    fn test_strip_install_directives_trims_leading_blank() {
        let content = "#SPOCKET_INSTALL_DESTINATION: /a.yaml\n\nreal content\n";
        let out = strip_install_directives(content);
        assert_eq!(out, "real content\n");
    }
}
