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
// - Files whose `#LEFT_POCKET_TEMPLATE_DESTINATION` targets `{{SPOCKET_CONFIG_ROOT}}`
//   or `{{SPOCKET_REGISTRY_ROOT}}` are *interpreted* now — the directive is
//   stripped, the two install-known roots are expanded, and the result is
//   written to the resolved path (e.g. `directory_structure.md` →
//   `$HOME/.config/safe_pocket/directory_structure.yaml`).
// - All other files are *staged verbatim* under
//   `$HOME/.config/safe_pocket/templates/<relative_path>` so the runtime
//   template loader can parse and apply them to individual pockets.
include!(concat!(env!("OUT_DIR"), "/embedded_templates.rs"));

// ── Template variables ───────────────────────────────────────────────────────

/// Context needed to expand template variables.
pub struct TemplateContext {
    /// Absolute path to the pocket root (e.g. `~/.left_pocket/19930adf3aaa`).
    pub pocket_root: PathBuf,
    /// Absolute path to the primary project directory.
    pub project_root: PathBuf,
    /// Short name of the pocket (the directory basename / hash).
    pub pocket_name: String,
    /// Absolute path to the global observations directory (`~/.left_pocket/observations`).
    pub global_observations_path: PathBuf,
    /// Absolute path to the pocket config directory (`$HOME/.config/left_pocket`).
    pub config_root: PathBuf,
}

/// Legacy markers for the old Beads integration block. Retained only so that
/// pockets created before the built-in task tracker can have the stale block
/// stripped out on their next runtime merge. Nothing new is ever written with
/// these markers.
const LEGACY_BEADS_BEGIN_MARKER: &str = "<!-- BEGIN BEADS INTEGRATION -->";
const LEGACY_BEADS_END_MARKER: &str = "<!-- END BEADS INTEGRATION -->";

/// Agent-facing guidance injected into AGENTS.md at runtime, describing
/// left_pocket's built-in task tracker (`left_pocket task`).
const TASK_RUNTIME_BLOCK: &str = r#"<!-- BEGIN POCKET TASK INTEGRATION -->
## Issue Tracking with `left_pocket task`

**IMPORTANT**: This project tracks all work in left_pocket's built-in task
tracker. Do NOT use markdown TODO lists or external issue trackers — use
`left_pocket task` so every agent shares one source of truth.

### Why `left_pocket task`?

- Fast: backed by a local SQLite database, no network or daemon required.
- Shared: every agent on this left_pocket sees the same task list.
- Scoped: tasks are grouped per project by a prefix derived automatically from
  the left_pocket — just run the commands from inside the project directory.

### Quick reference

```bash
# What work is open (lowest priority number = highest urgency)?
left_pocket task list
left_pocket task list --priority 1      # only P0 and P1
left_pocket task list --raw             # JSON, for programmatic use

# Create work
left_pocket task create --named "Implement feature X" \
  --description "Why this matters and what to do" --priority 1

# Drive a task through its lifecycle (ID may be the full id or the suffix)
left_pocket task <ID> assign --agent "Builder"
left_pocket task <ID> start  --notes "Starting now"
left_pocket task <ID> log    --notes "Progress / findings"
left_pocket task <ID> close  --notes "Done and verified"
left_pocket task <ID> discard
left_pocket task <ID> describe          # full details + history
left_pocket task <ID> describe --raw    # JSON
```

### Priorities

- `0` — Critical (security, data loss, broken builds)
- `1` — High (major features, important bugs)
- `2` — Medium (default)
- `3` — Low (polish, optimization)
- `4` — Backlog (future ideas)

### Workflow for AI Agents

1. **Check open work**: `left_pocket task list` before asking what to do.
2. **Claim it**: `left_pocket task <ID> assign --agent "<you>"` then
   `left_pocket task <ID> start`.
3. **Record progress**: `left_pocket task <ID> log --notes "…"` as you go.
4. **Discover new work?** `left_pocket task create --named "…" --description "…"`.
5. **Finish**: `left_pocket task <ID> close --notes "…"`.

### Rules

- ✅ Use `left_pocket task` for ALL task tracking.
- ✅ Use `--raw` when you need structured (JSON) output.
- ❌ Do NOT create markdown TODO lists.
- ❌ Do NOT use external issue trackers.
<!-- END POCKET TASK INTEGRATION -->
"#;

/// Replace `{{POCKET_ROOT}}`/`{{LEFT_POCKET_ROOT}}`/`{{CORNER_ROOT}}`/`{{SPOCKET_ROOT}}`,
/// `{{PROJECT_ROOT}}`, `{{POCKET_NAME}}`/`{{LEFT_POCKET_NAME}}`/`{{CORNER_NAME}}`/`{{SPOCKET_NAME}}`,
/// `{{GLOBAL_OBSERVATIONS_PATH}}`,
/// `{{POCKET_CONFIG_ROOT}}`/`{{LEFT_POCKET_CONFIG_ROOT}}`/`{{CORNER_CONFIG_ROOT}}`/`{{SPOCKET_CONFIG_ROOT}}`,
/// and `{{POCKET_REGISTRY_ROOT}}`/`{{LEFT_POCKET_REGISTRY_ROOT}}`/`{{CORNER_REGISTRY_ROOT}}`/`{{SPOCKET_REGISTRY_ROOT}}`
/// in `text`.
pub fn expand_variables(text: &str, ctx: &TemplateContext) -> String {
    let registry_root = crate::registry::registry_root()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "{{POCKET_REGISTRY_ROOT}}".to_string());
    text.replace("{{POCKET_ROOT}}", &ctx.pocket_root.to_string_lossy())
        .replace("{{LEFT_POCKET_ROOT}}", &ctx.pocket_root.to_string_lossy())
        .replace("{{CORNER_ROOT}}", &ctx.pocket_root.to_string_lossy())
        .replace("{{SPOCKET_ROOT}}", &ctx.pocket_root.to_string_lossy())
        .replace("{{PROJECT_ROOT}}", &ctx.project_root.to_string_lossy())
        .replace("{{POCKET_NAME}}", &ctx.pocket_name)
        .replace("{{LEFT_POCKET_NAME}}", &ctx.pocket_name)
        .replace("{{CORNER_NAME}}", &ctx.pocket_name)
        .replace("{{SPOCKET_NAME}}", &ctx.pocket_name)
        .replace(
            "{{GLOBAL_OBSERVATIONS_PATH}}",
            &ctx.global_observations_path.to_string_lossy(),
        )
        .replace("{{POCKET_CONFIG_ROOT}}", &ctx.config_root.to_string_lossy())
        .replace(
            "{{LEFT_POCKET_CONFIG_ROOT}}",
            &ctx.config_root.to_string_lossy(),
        )
        .replace("{{CORNER_CONFIG_ROOT}}", &ctx.config_root.to_string_lossy())
        .replace(
            "{{SPOCKET_CONFIG_ROOT}}",
            &ctx.config_root.to_string_lossy(),
        )
        .replace("{{POCKET_REGISTRY_ROOT}}", &registry_root)
        .replace("{{LEFT_POCKET_REGISTRY_ROOT}}", &registry_root)
        .replace("{{CORNER_REGISTRY_ROOT}}", &registry_root)
        .replace("{{SPOCKET_REGISTRY_ROOT}}", &registry_root)
}

/// Token marking a destination/content as resolvable at install time (before any
/// pocket exists), using only the two install-known roots.
const CONFIG_ROOT_TOKEN: &str = "{{SPOCKET_CONFIG_ROOT}}";
const LEFT_POCKET_CONFIG_ROOT_TOKEN: &str = "{{LEFT_POCKET_CONFIG_ROOT}}";
const POCKET_CONFIG_ROOT_TOKEN: &str = "{{POCKET_CONFIG_ROOT}}";
const REGISTRY_ROOT_TOKEN: &str = "{{SPOCKET_REGISTRY_ROOT}}";
const LEFT_POCKET_REGISTRY_ROOT_TOKEN: &str = "{{LEFT_POCKET_REGISTRY_ROOT}}";
const POCKET_REGISTRY_ROOT_TOKEN: &str = "{{POCKET_REGISTRY_ROOT}}";

// ── Directive prefixes ───────────────────────────────────────────────────────
//
// left_pocket writes `#POCKET_*` directives and runtime markers. The legacy
// `#LEFT_POCKET_`, `#CORNER_`, and `#SPOCKET_` forms are still recognised when
// *reading* templates and placed files so that pockets created before the
// renames keep working without a manual upgrade. New content is always emitted
// with the `#POCKET_` prefix.

const POCKET_DIRECTIVE_PREFIX: &str = "#POCKET_";
const LEGACY_DIRECTIVE_PREFIXES: &[&str] = &["#LEFT_POCKET_", "#CORNER_", "#SPOCKET_"];

/// Return the directive suffix (the part after `#POCKET_` / `#LEFT_POCKET_` /
/// `#CORNER_` / `#SPOCKET_`) if `line` begins with any recognised prefix, else
/// `None`.
fn directive_suffix(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix(POCKET_DIRECTIVE_PREFIX) {
        return Some(rest);
    }
    for prefix in LEGACY_DIRECTIVE_PREFIXES {
        if let Some(rest) = trimmed.strip_prefix(*prefix) {
            return Some(rest);
        }
    }
    None
}

/// True if `line` is any recognised directive line (`#POCKET_*` or a legacy
/// `#LEFT_POCKET_*`/`#CORNER_*`/`#SPOCKET_*` form).
fn is_directive_line(line: &str) -> bool {
    directive_suffix(line).is_some()
}

/// Expand only the two install-known roots (`{{SPOCKET_CONFIG_ROOT}}` and
/// `{{SPOCKET_REGISTRY_ROOT}}`). left_pocket-level variables such as
/// `{{SPOCKET_ROOT}}` are intentionally left untouched so that literal examples
/// embedded in interpreted files (e.g. feature-tag descriptions) survive.
fn expand_install_roots(text: &str, config_root: &Path, registry_root: &Path) -> String {
    text.replace(POCKET_CONFIG_ROOT_TOKEN, &config_root.to_string_lossy())
        .replace(
            LEFT_POCKET_CONFIG_ROOT_TOKEN,
            &config_root.to_string_lossy(),
        )
        .replace(CONFIG_ROOT_TOKEN, &config_root.to_string_lossy())
        .replace(POCKET_REGISTRY_ROOT_TOKEN, &registry_root.to_string_lossy())
        .replace(
            LEFT_POCKET_REGISTRY_ROOT_TOKEN,
            &registry_root.to_string_lossy(),
        )
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

    if tmpl.destination == "{{POCKET_ROOT}}/AGENTS.md"
        || tmpl.destination == "{{LEFT_POCKET_ROOT}}/AGENTS.md"
        || tmpl.destination == "{{SPOCKET_ROOT}}/AGENTS.md"
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
    /// Relative destination path inside the left_pocket (after variable expansion).
    pub destination: String,
    /// File content (everything after the metadata lines, with `#SPOCKET` lines stripped).
    pub content: String,
    /// If true, merge with existing file rather than overwriting.
    pub quiet_merge: bool,
    /// If true, an empty destination file is placed at creation and content is
    /// injected at runtime (VS Code open/close)
    /// wrapped in `#LEFT_POCKET_RUNTIME_CONTENT_START` / `#LEFT_POCKET_RUNTIME_CONTENT_END` markers.
    pub merge_at_runtime: bool,
    /// Original source file path (for diagnostics).
    #[allow(dead_code)]
    pub source_path: PathBuf,
}

/// Parse a template file into one or more [`Template`] blocks.
///
/// The first non-empty line **must** start with `#LEFT_POCKET_TEMPLATE_DESTINATION`
/// followed by a colon (optional) and the destination path. A file may contain
/// **multiple** `#LEFT_POCKET_TEMPLATE_DESTINATION` directives: each one begins a new
/// block, and all content beneath it applies to that destination until the next
/// `#LEFT_POCKET_TEMPLATE_DESTINATION` directive (or end of file). Blocks that target
/// the same destination are concatenated.
///
/// All other lines that start with `#SPOCKET` are treated as metadata and
/// stripped. Recognised per-block metadata directives:
/// - `#POCKET_QUIET_MERGE` — merge with existing file instead of overwriting.
/// - `#LEFT_POCKET_MERGE_AT_RUNTIME` — inject content at runtime.
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
            // Tolerate other directive lines (e.g. a stray            // `#LEFT_POCKET_INSTALL_DESTINATION`) that may precede the first
            // template block — they are install-time metadata and are simply
            // stripped here.
            if is_directive_line(line) {
                continue;
            }
            return Err(anyhow!(
                "Template file '{}' is missing #POCKET_TEMPLATE_DESTINATION before its content.\n\
                     Found: {}",
                path.display(),
                line
            ));
        };

        let trimmed = line.trim();
        match directive_suffix(trimmed) {
            Some("QUIET_MERGE") => current.quiet_merge = true,
            Some("MERGE_AT_RUNTIME") => current.merge_at_runtime = true,
            // Strip unrecognised metadata directives (both #POCKET_* and the
            // legacy #LEFT_POCKET_*/#CORNER_*/#SPOCKET_* forms).
            _ if is_directive_line(line) => {}
            _ => current.lines.push(line.to_string()),
        }
    }

    if blocks.is_empty() {
        return Err(anyhow!(
            "Template file '{}' is missing #POCKET_TEMPLATE_DESTINATION.",
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

/// Parse a `#LEFT_POCKET_TEMPLATE_DESTINATION` (or legacy `#LEFT_POCKET_TEMPLATE_DESTINATION`)
/// directive from a line. Accepts both `#LEFT_POCKET_TEMPLATE_DESTINATION: path` and
/// `#LEFT_POCKET_TEMPLATE_DESTINATION path` (with or without colon).
fn parse_destination_directive(line: &str) -> Option<String> {
    let line = line.trim();
    let suffix = directive_suffix(line)?;
    let directive = "TEMPLATE_DESTINATION";
    let rest = suffix.strip_prefix(directive)?;
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

/// Returns the path to the templates directory, preferring left_pocket's config root
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

/// Install default assets into left_pocket's canonical roots regardless of whether
/// legacy safe_pocket directories already exist. This is used by the install
/// script so installation always seeds `~/.config/left_pocket` and `~/.left_pocket`.
pub fn install_default_assets_to_current_roots() -> Result<()> {
    install_default_assets_to_current_roots_with_mode(false)
}

/// Force-install default assets into left_pocket's canonical roots, overwriting any
/// existing templates and config assets. Used by
/// `left_pocket install-default-assets --replace` and the install script's "replace"
/// path. Legacy-named template files (e.g. `safe_pocket.env.md`) are removed
/// because the `templates/` directory is cleared first.
pub fn install_default_assets_to_current_roots_replacing() -> Result<()> {
    install_default_assets_to_current_roots_with_mode(true)
}

fn install_default_assets_to_current_roots_with_mode(force: bool) -> Result<()> {
    let config_dir = crate::branding::current_config_root()?;
    let tmpl_dir = config_dir.join("templates");
    if force {
        if tmpl_dir.exists() {
            fs::remove_dir_all(&tmpl_dir).with_context(|| {
                format!("Failed to remove templates dir: {}", tmpl_dir.display())
            })?;
        }
        fs::create_dir_all(&tmpl_dir).context("Failed to create primary templates directory")?;
    } else {
        fs::create_dir_all(&tmpl_dir).context("Failed to create primary templates directory")?;
    }

    let registry_root = crate::registry::current_registry_dir()?;
    fs::create_dir_all(registry_root.join("observations"))
        .context("Failed to create primary global observations directory")?;

    if force {
        install_embedded_templates_replacing(&config_dir, &registry_root)
    } else {
        install_embedded_templates(&config_dir, &registry_root)
    }
}

/// Materialise every embedded template into the user's config directory, never
/// overwriting an existing file.
///
/// Placement is driven entirely by directives inside each file (no hard-coded
/// file names):
/// - `#LEFT_POCKET_INSTALL_DESTINATION` / `#LEFT_POCKET_INSTALL_DESTINATION` places the
///   file at `<path>` (one or more) at install time. These directives are
///   **stripped** from the placed file. Used for assets that live directly in
///   the config root, e.g. `directory_structure.yaml`, `feature_tags.yaml`,
///   and the conversation feature-tag definition.
/// - When a file has **no** install destination, it is mirrored to
///   `<config_dir>/templates/<relative_path>` so the runtime loader can find it.
///
/// `#LEFT_POCKET_TEMPLATE_DESTINATION` / `#LEFT_POCKET_TEMPLATE_DESTINATION` directives
/// are always **left intact** in the placed file — they are consumed later, at
/// runtime, when a new left_pocket is created.
pub fn install_embedded_templates(config_dir: &Path, registry_root: &Path) -> Result<()> {
    install_embedded_templates_with_mode(config_dir, registry_root, false)
}

/// Force-install every embedded template, overwriting existing files and
/// clearing the `templates/` directory first so legacy-named files (e.g. a
/// `safe_pocket.env.md` left over after a rename to `left_pocket.env.md`) are
/// removed. Config-root assets (`directory_structure.yaml`, `feature_tags.yaml`,
/// …) are overwritten in place.
///
/// This is the destructive "replace" path invoked by
/// `left_pocket install-default-assets --replace`. User customisations inside
/// `templates/` are lost; the user is expected to re-apply them or skip the
/// replace step.
pub fn install_embedded_templates_replacing(config_dir: &Path, registry_root: &Path) -> Result<()> {
    let tmpl_dir = config_dir.join("templates");
    if tmpl_dir.exists() {
        fs::remove_dir_all(&tmpl_dir).with_context(|| {
            format!(
                "Failed to remove existing templates dir: {}",
                tmpl_dir.display()
            )
        })?;
    }
    install_embedded_templates_with_mode(config_dir, registry_root, true)
}

fn install_embedded_templates_with_mode(
    config_dir: &Path,
    registry_root: &Path,
    force: bool,
) -> Result<()> {
    for (rel, content) in EMBEDDED_TEMPLATES {
        install_one_embedded(rel, content, config_dir, registry_root, force)?;
    }
    Ok(())
}

fn install_one_embedded(
    rel: &str,
    content: &str,
    config_dir: &Path,
    registry_root: &Path,
    force: bool,
) -> Result<()> {
    // Collect any explicit install destinations and produce the placed body
    // (the same content with every `#LEFT_POCKET_INSTALL_DESTINATION` /
    // `#LEFT_POCKET_INSTALL_DESTINATION` line removed).
    let install_dirs: Vec<String> = content
        .lines()
        .filter_map(parse_install_destination_directive)
        .collect();
    let placed = strip_install_directives(content);

    if install_dirs.is_empty() {
        // Default: mirror into the system-wide templates directory so the
        // runtime loader can pick it up (its TEMPLATE_DESTINATION is preserved).
        let staged = config_dir.join("templates").join(rel);
        return write_template(&staged, &placed, "template", force);
    }

    // Explicit install destination(s): place directly into the config tree.
    for dir in &install_dirs {
        let dest = expand_install_roots(dir, config_dir, registry_root);
        write_template(Path::new(&dest), &placed, "config asset", force)?;
    }
    Ok(())
}

/// Remove every `#LEFT_POCKET_INSTALL_DESTINATION` line from `content`. If any were
/// removed, a leading run of blank lines is trimmed so the placed file starts at
/// its real content. `#LEFT_POCKET_TEMPLATE_DESTINATION` and all other lines are
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

/// Parse a `#LEFT_POCKET_INSTALL_DESTINATION` (or legacy `#LEFT_POCKET_INSTALL_DESTINATION`)
/// directive from a line. Accepts both `#LEFT_POCKET_INSTALL_DESTINATION: path` and
/// `#LEFT_POCKET_INSTALL_DESTINATION path`.
fn parse_install_destination_directive(line: &str) -> Option<String> {
    let line = line.trim();
    let suffix = directive_suffix(line)?;
    let directive = "INSTALL_DESTINATION";
    let rest = suffix.strip_prefix(directive)?;
    let rest = rest.trim_start_matches(':').trim();
    if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    }
}

fn write_template(target: &Path, content: &str, label: &str, force: bool) -> Result<()> {
    if target.exists() && !force {
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

#[allow(dead_code)]
fn write_if_absent(target: &Path, content: &str, label: &str) -> Result<()> {
    write_template(target, content, label, false)
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
                // (synced via `spocket sync agents`), not left_pocket templates.
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

pub const RUNTIME_START_MARKER: &str = "#POCKET_RUNTIME_CONTENT_START";
pub const RUNTIME_END_MARKER: &str = "#POCKET_RUNTIME_CONTENT_END";

/// Legacy runtime markers from the left_pocket, Corner, and Spocket eras. New
/// content is always written with the `#POCKET_*` markers above, but we still
/// recognise (and strip) the old ones so pockets created before the renames get
/// cleaned up on the next runtime merge.
const LEGACY_RUNTIME_START_MARKERS: &[&str] = &[
    "#LEFT_POCKET_RUNTIME_CONTENT_START",
    "#CORNER_RUNTIME_CONTENT_START",
    "#SPOCKET_RUNTIME_CONTENT_START",
];
const LEGACY_RUNTIME_END_MARKERS: &[&str] = &[
    "#LEFT_POCKET_RUNTIME_CONTENT_END",
    "#CORNER_RUNTIME_CONTENT_END",
    "#SPOCKET_RUNTIME_CONTENT_END",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum TemplateApplyMode {
    Create,
    Upgrade,
}

fn strip_markers(content: &str) -> String {
    // Strip the current `#POCKET_RUNTIME_CONTENT_*` markers and the legacy
    // `#LEFT_POCKET_*`/`#CORNER_*`/`#SPOCKET_*` markers so old runtime blocks
    // are removed alongside new ones.
    let mut out = strip_managed_block(content, RUNTIME_START_MARKER, RUNTIME_END_MARKER);
    for (start, end) in LEGACY_RUNTIME_START_MARKERS
        .iter()
        .zip(LEGACY_RUNTIME_END_MARKERS)
    {
        out = strip_managed_block(&out, start, end);
    }
    out
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

        if is_directive_line(line_content) {
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

    if !existing.contains(RUNTIME_START_MARKER)
        && !LEGACY_RUNTIME_START_MARKERS
            .iter()
            .any(|marker| existing.contains(marker))
    {
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

pub fn apply_merge_at_runtime(pocket_dir: &Path, ctx: &TemplateContext) -> Result<usize> {
    let templates = load_templates()?;
    let mut count = 0;

    for tmpl in &templates {
        let dest_rel = expand_variables(&tmpl.destination, ctx);
        let dest_path = resolve_template_destination(&dest_rel, pocket_dir, ctx);

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
        let dest_path = resolve_template_destination(&dest_rel, pocket_dir, ctx);

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

pub fn strip_merge_at_runtime(pocket_dir: &Path, ctx: &TemplateContext) -> Result<usize> {
    let templates = load_templates()?;
    let mut count = 0;

    for tmpl in templates.iter().filter(|t| t.merge_at_runtime) {
        let dest_rel = expand_variables(&tmpl.destination, ctx);
        let dest_path = resolve_template_destination(&dest_rel, pocket_dir, ctx);

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

/// Apply all quiet-merge templates to the left_pocket, creating or updating each
/// destination file.  Unlike the initial `apply_templates` call (Create mode),
/// this function is safe to run on every workspace open because:
///
/// 1. **Idempotent** — `merge_content` only appends lines/keys that are not
///    already present.  Running it twice on a fully-populated file is a no-op.
/// 2. **Non-destructive** — existing content is always preserved; only missing
///    keys are inserted.
///
/// This is used to catch templates that were added (or updated) after a left_pocket
/// was first created so that the project `.env` and other managed files are
/// kept up to date without requiring users to manually upgrade or recreate
/// their pockets.
pub fn apply_quiet_merge_templates(pocket_dir: &Path, ctx: &TemplateContext) -> Result<usize> {
    let templates = load_templates()?;
    apply_quiet_merge_template_set(&templates, pocket_dir, ctx)
}

/// Inner implementation of quiet-merge, factored out so tests can supply their
/// own template list without touching the filesystem config directories.
fn apply_quiet_merge_template_set(
    templates: &[Template],
    pocket_dir: &Path,
    ctx: &TemplateContext,
) -> Result<usize> {
    let mut count = 0;

    for tmpl in templates.iter().filter(|t| t.quiet_merge) {
        let dest_rel = expand_variables(&tmpl.destination, ctx);
        let dest_path = resolve_template_destination(&dest_rel, pocket_dir, ctx);
        let content = expand_template_content(&filter_template_content(&tmpl.content, ctx), ctx);

        if content.is_empty() {
            continue;
        }

        // Ensure parent directory exists (best-effort; if it fails, skip this template)
        if let Some(parent) = dest_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                if crate::verbose() {
                    eprintln!(
                        "  {} could not create parent dir for {}: {}",
                        "Warning:".bright_yellow(),
                        dest_path.display(),
                        e
                    );
                }
                continue;
            }
        }

        let existing = if dest_path.exists() {
            fs::read_to_string(&dest_path).unwrap_or_default()
        } else {
            String::new()
        };

        let merged = merge_content(&existing, &content);
        if merged == existing {
            // Nothing new to add — already up to date
            continue;
        }

        fs::write(&dest_path, &merged).with_context(|| {
            format!(
                "Failed to apply quiet-merge template to: {}",
                dest_path.display()
            )
        })?;
        count += 1;

        if crate::verbose() {
            println!(
                "  {} {}",
                "Quiet-merged:".bright_green(),
                dest_path.display().to_string().bright_blue()
            );
        }
    }

    Ok(count)
}

// ── Apply templates to a left_pocket ──────────────────────────────────────────────

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

/// Apply all loaded templates to a left_pocket directory.
///
/// - Creates directories from the directory structure.
/// - Expands template variables and writes files.
/// - Merge-at-runtime templates place only an empty destination file.
/// - If `interactive` is true, warns before overwriting existing non-runtime files.
///
/// Returns the number of files written.
pub fn apply_templates(
    pocket_dir: &Path,
    ctx: &TemplateContext,
    project_dir: Option<&Path>,
    interactive: bool,
) -> Result<usize> {
    apply_templates_with_mode(
        pocket_dir,
        ctx,
        project_dir,
        interactive,
        TemplateApplyMode::Create,
    )
}

fn apply_templates_with_mode(
    pocket_dir: &Path,
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
        let full_path = pocket_dir.join(dir);
        fs::create_dir_all(&full_path)
            .with_context(|| format!("Failed to create directory: {}", full_path.display()))?;
    }

    // 2. Load and apply templates
    let templates = load_templates()?;

    apply_template_set(&templates, pocket_dir, ctx, interactive, mode)
}

fn apply_template_set(
    templates: &[Template],
    pocket_dir: &Path,
    ctx: &TemplateContext,
    interactive: bool,
    mode: TemplateApplyMode,
) -> Result<usize> {
    let mut files_written = 0;

    for tmpl in templates {
        // Expand variables in the destination path
        let dest_rel = expand_variables(&tmpl.destination, ctx);
        // Runtime-merge templates place an empty file; normal templates keep
        // content variables for replacement when the left_pocket is opened.
        let content = if tmpl.merge_at_runtime {
            String::new()
        } else {
            expand_template_content(&filter_template_content(&tmpl.content, ctx), ctx)
        };

        // Resolve the destination: if it starts with the pocket_root, make it
        // relative to the left_pocket dir. Otherwise treat it as relative to left_pocket dir.
        let dest_path = resolve_template_destination(&dest_rel, pocket_dir, ctx);

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

            // Quiet-merge templates ALWAYS merge, in both Create and Upgrade
            // mode. This is the whole point of `#LEFT_POCKET_QUIET_MERGE`: the
            // destination is a file left_pocket shares with the user and other tools
            // (`.env`, `.gitignore`), so existing content must be preserved and
            // only genuinely new lines/keys appended. Overwriting here would
            // silently destroy user content — e.g. flattening a project's
            // 27-line .gitignore down to the template's single `.env` line.
            if tmpl.quiet_merge {
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
            move_existing_to_unhoused(pocket_dir, &dest_path, "template upgrade")?;
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

fn move_existing_to_unhoused(pocket_dir: &Path, path: &Path, operation: &str) -> Result<()> {
    if !path.starts_with(pocket_dir) || !path.exists() {
        return Ok(());
    }

    let relative = path.strip_prefix(pocket_dir).unwrap_or(path);
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    let mut target = pocket_dir.join("unhoused").join(&timestamp).join(relative);
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
            "Failed to move existing left_pocket content to unhoused: {} -> {}",
            path.display(),
            target.display()
        )
    })?;

    let log_path = pocket_dir.join("unhoused.log");
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
    let _ = crate::event::append_pocket_event(
        pocket_dir,
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
/// - An absolute path (e.g. `/Users/.../left_pocket/AGENTS.md`) → use as-is
/// - A relative path (e.g. `.github/copilot-instructions.md`) → relative to pocket_dir
fn resolve_template_destination(dest: &str, pocket_dir: &Path, _ctx: &TemplateContext) -> PathBuf {
    let path = PathBuf::from(dest);
    if path.is_absolute() {
        path
    } else {
        pocket_dir.join(path)
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

/// Upgrade an existing left_pocket to match the current templates.
///
/// This is called by `spocket -u <path>`. It does NOT open the workspace;
/// it resets non-runtime template destinations to match the templates while
/// preserving merge-at-runtime destinations for runtime injection.
pub fn upgrade_pocket(pocket_dir: &Path) -> Result<()> {
    // Validate the left_pocket directory exists and has a manifest
    if !pocket_dir.exists() {
        return Err(anyhow!(
            "left_pocket directory does not exist: {}",
            pocket_dir.display()
        ));
    }

    let manifest_path = pocket_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(anyhow!(
            "No manifest.json found in {}. Is this a valid left_pocket?",
            pocket_dir.display()
        ));
    }

    // Load manifest to get core_paths
    let manifest = crate::manifest::Manifest::load(pocket_dir)?
        .ok_or_else(|| anyhow!("Failed to load manifest from {}", pocket_dir.display()))?;

    let pocket_name = pocket_dir
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
        pocket_root: pocket_dir.to_path_buf(),
        project_root: project_root.clone(),
        pocket_name,
        global_observations_path: global_obs,
        config_root,
    };

    println!(
        "{} {}",
        "Upgrading left_pocket:".bright_white().bold(),
        pocket_dir.display().to_string().bright_yellow()
    );

    // Upgrade force-resets non-runtime templates while preserving user-authored
    // content in merge-at-runtime destinations.
    let files_written = apply_templates_with_mode(
        pocket_dir,
        &ctx,
        Some(&project_root),
        false,
        TemplateApplyMode::Upgrade,
    )?;

    let runtime_updated = apply_merge_at_runtime(pocket_dir, &ctx)?;
    let total_updated = files_written + runtime_updated;

    if total_updated == 0 {
        println!(
            "{}",
            "left_pocket is already up to date with templates.".bright_green()
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
            "#LEFT_POCKET_TEMPLATE_DESTINATION: .github/copilot-instructions.md",
        );
        assert_eq!(result, Some(".github/copilot-instructions.md".to_string()));
    }

    #[test]
    fn test_parse_destination_without_colon() {
        let result = parse_destination_directive(
            "#LEFT_POCKET_TEMPLATE_DESTINATION .github/prompts/talkLikeACat.md",
        );
        assert_eq!(result, Some(".github/prompts/talkLikeACat.md".to_string()));
    }

    #[test]
    fn test_parse_destination_with_template_variable() {
        let result = parse_destination_directive(
            "#LEFT_POCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/AGENTS.md",
        );
        assert_eq!(result, Some("{{SPOCKET_ROOT}}/AGENTS.md".to_string()));
    }

    #[test]
    fn test_parse_destination_empty_returns_none() {
        let result = parse_destination_directive("#LEFT_POCKET_TEMPLATE_DESTINATION:");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_destination_no_prefix() {
        let result = parse_destination_directive("some random line");
        assert_eq!(result, None);
    }

    // ── expand_variables ────────────────────────────────────────────────

    fn make_ctx(pocket_root: &str, project_root: &str, name: &str) -> TemplateContext {
        TemplateContext {
            pocket_root: PathBuf::from(pocket_root),
            project_root: PathBuf::from(project_root),
            pocket_name: name.to_string(),
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
    fn test_expand_variables_supports_pocket_aliases() {
        let ctx = make_ctx(
            "/home/user/.left_pocket/abc123",
            "/home/user/project",
            "abc123",
        );
        let input = "Root: {{LEFT_POCKET_ROOT}}\nName: {{LEFT_POCKET_NAME}}\nCfg: {{LEFT_POCKET_CONFIG_ROOT}}";

        let output = expand_variables(input, &ctx);

        assert_eq!(
            output,
            "Root: /home/user/.left_pocket/abc123\nName: abc123\nCfg: /home/user/.config/safe_pocket"
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
            "#LEFT_POCKET_TEMPLATE_DESTINATION: .github/test.md\nHello world\nSecond line\n",
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
            "#LEFT_POCKET_TEMPLATE_DESTINATION: test.md\n\
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
            "#LEFT_POCKET_TEMPLATE_DESTINATION file1.md\n\
             bow wow wow\n\
             \n\
             more stuff\n\
             \n\
             #LEFT_POCKET_TEMPLATE_DESTINATION file2.md\n\
             meow meow meow\n\
             \n\
             #LEFT_POCKET_TEMPLATE_DESTINATION file1.md\n\
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
            "#LEFT_POCKET_TEMPLATE_DESTINATION a.md\n\
             #LEFT_POCKET_QUIET_MERGE\n\
             alpha\n\
             #LEFT_POCKET_TEMPLATE_DESTINATION b.md\n\
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
        let ctx = make_ctx("/left_pocket", "/project", "hash");
        let left_pocket = PathBuf::from("/left_pocket");

        let result = resolve_template_destination(".github/test.md", &left_pocket, &ctx);
        assert_eq!(result, PathBuf::from("/left_pocket/.github/test.md"));
    }

    #[test]
    fn test_resolve_destination_absolute() {
        let ctx = make_ctx("/left_pocket", "/project", "hash");
        let left_pocket = PathBuf::from("/left_pocket");

        let result = resolve_template_destination("/left_pocket/AGENTS.md", &left_pocket, &ctx);
        assert_eq!(result, PathBuf::from("/left_pocket/AGENTS.md"));
    }

    // ── apply_templates (integration) ───────────────────────────────────

    #[test]
    fn test_apply_templates_creates_dirs_and_files() {
        // This test creates a mini template setup in a temp dir and verifies
        // that apply_templates creates the expected structure.
        let base = std::env::temp_dir().join("spocket_test_apply");
        let _ = fs::remove_dir_all(&base);

        let pocket_dir = base.join("left_pocket");
        let config_dir = base.join("config");
        let tmpl_dir = config_dir.join("templates");
        fs::create_dir_all(&tmpl_dir).unwrap();
        fs::create_dir_all(&pocket_dir).unwrap();

        // Write a template
        fs::write(
            tmpl_dir.join("test.md"),
            "#LEFT_POCKET_TEMPLATE_DESTINATION: subdir/test.md\nHello {{SPOCKET_NAME}}\n",
        )
        .unwrap();

        // Write directory structure
        fs::write(config_dir.join("directory_structure.md"), "subdir\nother\n").unwrap();

        let ctx = make_ctx(
            &pocket_dir.to_string_lossy(),
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

    // ── #LEFT_POCKET_QUIET_MERGE ────────────────────────────────────────────

    #[test]
    fn test_parse_template_quiet_merge_flag() {
        let dir = std::env::temp_dir().join("spocket_test_quiet_merge_flag");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("env.md");
        fs::write(
            &file,
            "#LEFT_POCKET_TEMPLATE_DESTINATION: .env\n#LEFT_POCKET_QUIET_MERGE\n\nMY_KEY=value\n",
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
            "#LEFT_POCKET_TEMPLATE_DESTINATION: regular.txt\nSome content\n",
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
            "#LEFT_POCKET_TEMPLATE_DESTINATION: AGENTS.md\n\
             #LEFT_POCKET_MERGE_AT_RUNTIME\n\nRuntime content\n",
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
            "Meow like a cat\n#POCKET_RUNTIME_CONTENT_START\nBark like a dog\n#POCKET_RUNTIME_CONTENT_END\n"
        );

        assert!(strip_runtime_content(&file).unwrap());
        assert_eq!(fs::read_to_string(&file).unwrap(), "Meow like a cat\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_runtime_variable_expansion_skips_spocket_directives() {
        let ctx = make_ctx("/left_pocket", "/project", "hash");
        let input = "Path: {{SPOCKET_ROOT}}\n#SPOCKET_NOTE: {{SPOCKET_ROOT}}\n";

        assert_eq!(
            expand_runtime_variables_in_content(input, &ctx),
            "Path: /left_pocket\n#SPOCKET_NOTE: {{SPOCKET_ROOT}}\n"
        );
    }

    #[test]
    fn test_runtime_variable_expansion_skips_pocket_directives() {
        let ctx = make_ctx("/left_pocket", "/project", "hash");
        let input = "Path: {{LEFT_POCKET_ROOT}}\n#LEFT_POCKET_NOTE: {{LEFT_POCKET_ROOT}}\n";

        assert_eq!(
            expand_runtime_variables_in_content(input, &ctx),
            "Path: /left_pocket\n#LEFT_POCKET_NOTE: {{LEFT_POCKET_ROOT}}\n"
        );
    }

    #[test]
    fn test_parse_destination_recognises_pocket_prefix() {
        assert_eq!(
            parse_destination_directive(
                "#LEFT_POCKET_TEMPLATE_DESTINATION: .github/copilot-instructions.md",
            ),
            Some(".github/copilot-instructions.md".to_string())
        );
        assert_eq!(
            parse_destination_directive(
                "#LEFT_POCKET_TEMPLATE_DESTINATION {{LEFT_POCKET_ROOT}}/AGENTS.md"
            ),
            Some("{{LEFT_POCKET_ROOT}}/AGENTS.md".to_string())
        );
        // Legacy SPOCKET form is still recognised.
        assert_eq!(
            parse_destination_directive("#LEFT_POCKET_TEMPLATE_DESTINATION: legacy.md"),
            Some("legacy.md".to_string())
        );
    }

    #[test]
    fn test_parse_template_recognises_pocket_directives() {
        let dir = std::env::temp_dir().join("pocket_test_parse_pocket_directives");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("test.md");
        fs::write(
            &file,
            "#LEFT_POCKET_TEMPLATE_DESTINATION: .env\n#LEFT_POCKET_QUIET_MERGE\n\nMY_KEY=value\n",
        )
        .unwrap();

        let tmpl = parse_template(&file).unwrap().remove(0);
        assert_eq!(tmpl.destination, ".env");
        assert!(tmpl.quiet_merge);
        assert_eq!(tmpl.content, "MY_KEY=value\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_strip_markers_removes_legacy_spocket_runtime_block() {
        let existing = "User content\n#LEFT_POCKET_RUNTIME_CONTENT_START\nold runtime\n#LEFT_POCKET_RUNTIME_CONTENT_END\nmore user\n";
        assert_eq!(strip_markers(existing), "User content\nmore user\n");
    }

    #[test]
    fn test_strip_markers_removes_both_pocket_and_spocket_runtime_blocks() {
        let existing = "a\n#LEFT_POCKET_RUNTIME_CONTENT_START\nc1\n#LEFT_POCKET_RUNTIME_CONTENT_END\nb\n#LEFT_POCKET_RUNTIME_CONTENT_START\ns1\n#LEFT_POCKET_RUNTIME_CONTENT_END\nc\n";
        assert_eq!(strip_markers(existing), "a\nb\nc\n");
    }

    #[test]
    fn test_strip_markers_removes_corner_runtime_block() {
        let existing =
            "a\n#LEFT_POCKET_RUNTIME_CONTENT_START\nc1\n#LEFT_POCKET_RUNTIME_CONTENT_END\nb\n";
        assert_eq!(strip_markers(existing), "a\nb\n");
    }

    #[test]
    fn test_directive_suffix_recognises_corner_prefix() {
        assert_eq!(
            directive_suffix("#LEFT_POCKET_TEMPLATE_DESTINATION: x.md"),
            Some("TEMPLATE_DESTINATION: x.md")
        );
        assert_eq!(
            directive_suffix("#LEFT_POCKET_QUIET_MERGE"),
            Some("QUIET_MERGE")
        );
        assert!(is_directive_line("  #LEFT_POCKET_MERGE_AT_RUNTIME"));
    }

    #[test]
    fn test_inject_runtime_replaces_legacy_spocket_block_with_pocket() {
        let dir = std::env::temp_dir().join("pocket_test_inject_replaces_legacy");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        let file = dir.join("AGENTS.md");
        fs::write(
            &file,
            "keep\n#LEFT_POCKET_RUNTIME_CONTENT_START\nold\n#LEFT_POCKET_RUNTIME_CONTENT_END\n",
        )
        .unwrap();

        assert!(inject_runtime_content(&file, "new runtime\n").unwrap());
        let after = fs::read_to_string(&file).unwrap();
        assert!(!after.contains("#SPOCKET_RUNTIME_CONTENT"));
        assert!(!after.contains("#LEFT_POCKET_RUNTIME_CONTENT"));
        assert!(after.contains("#POCKET_RUNTIME_CONTENT_START"));
        assert!(after.contains("new runtime"));
        assert!(after.contains("keep"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_filter_template_content_removes_beads_dir() {
        let ctx = make_ctx("/left_pocket", "/project", "hash");
        let content = "SPOCKET_ROOT={{SPOCKET_ROOT}}\nBEADS_DIR={{SPOCKET_ROOT}}/.beads\n";

        assert_eq!(
            filter_template_content(content, &ctx),
            "SPOCKET_ROOT={{SPOCKET_ROOT}}\n"
        );
    }

    #[test]
    fn test_runtime_content_for_agents_includes_task_block() {
        let ctx = make_ctx("/left_pocket", "/project", "hash");
        let tmpl = Template {
            destination: "{{SPOCKET_ROOT}}/AGENTS.md".to_string(),
            content: "Base runtime\n".to_string(),
            quiet_merge: false,
            merge_at_runtime: true,
            source_path: PathBuf::from("/tmp/AGENTS.md"),
        };

        let content = runtime_content_for_template(&tmpl, &ctx);
        assert!(content.contains("Base runtime"));
        assert!(content.contains("<!-- BEGIN POCKET TASK INTEGRATION -->"));
        assert!(content.contains("left_pocket task"));
    }

    #[test]
    fn test_create_mode_places_empty_merge_at_runtime_destination() {
        let dir = std::env::temp_dir().join("spocket_test_create_places_empty_runtime");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("left_pocket");
        fs::create_dir_all(&pocket_dir).unwrap();

        let templates = vec![Template {
            destination: "AGENTS.md".to_string(),
            content: "Runtime content\n".to_string(),
            quiet_merge: false,
            merge_at_runtime: true,
            source_path: dir.join("template.md"),
        }];
        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "hash");

        let written = apply_template_set(
            &templates,
            &pocket_dir,
            &ctx,
            false,
            TemplateApplyMode::Create,
        )
        .unwrap();

        assert_eq!(written, 1);
        assert_eq!(
            fs::read_to_string(pocket_dir.join("AGENTS.md")).unwrap(),
            ""
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_create_mode_does_not_overwrite_existing_runtime_destination() {
        let dir = std::env::temp_dir().join("spocket_test_create_keeps_runtime_destination");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("left_pocket");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::write(pocket_dir.join("AGENTS.md"), "User content\n").unwrap();

        let templates = vec![Template {
            destination: "AGENTS.md".to_string(),
            content: "Runtime content\n".to_string(),
            quiet_merge: false,
            merge_at_runtime: true,
            source_path: dir.join("template.md"),
        }];
        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "hash");

        let written = apply_template_set(
            &templates,
            &pocket_dir,
            &ctx,
            false,
            TemplateApplyMode::Create,
        )
        .unwrap();

        assert_eq!(written, 0);
        assert_eq!(
            fs::read_to_string(pocket_dir.join("AGENTS.md")).unwrap(),
            "User content\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_mode_overwrites_non_runtime_and_preserves_runtime() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_template_rules");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("left_pocket");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::write(pocket_dir.join("normal.md"), "User edit\n").unwrap();
        fs::write(pocket_dir.join("AGENTS.md"), "User instructions\n").unwrap();

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
        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "hash");

        let written = apply_template_set(
            &templates,
            &pocket_dir,
            &ctx,
            false,
            TemplateApplyMode::Upgrade,
        )
        .unwrap();

        assert_eq!(written, 1);
        assert_eq!(
            fs::read_to_string(pocket_dir.join("normal.md")).unwrap(),
            "Template reset\n"
        );
        assert_eq!(
            fs::read_to_string(pocket_dir.join("AGENTS.md")).unwrap(),
            "User instructions\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_mode_merges_quiet_merge_template_preserving_user_content() {
        // Per the "Quiet merging" contract: on `left_pocket -u`, a #LEFT_POCKET_QUIET_MERGE
        // template must PRESERVE existing user content and only add genuinely new
        // lines. It must never overwrite the destination.
        let dir = std::env::temp_dir().join("pocket_test_upgrade_merges_quiet_merge");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("left_pocket");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::write(pocket_dir.join(".env"), "USER_KEY=custom\n").unwrap();

        let templates = vec![Template {
            destination: ".env".to_string(),
            content: "TEMPLATE_KEY=value\n".to_string(),
            quiet_merge: true,
            merge_at_runtime: false,
            source_path: dir.join("env-template.md"),
        }];
        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "hash");

        apply_template_set(
            &templates,
            &pocket_dir,
            &ctx,
            false,
            TemplateApplyMode::Upgrade,
        )
        .unwrap();

        let result = fs::read_to_string(pocket_dir.join(".env")).unwrap();
        assert!(
            result.contains("USER_KEY=custom"),
            "upgrade must preserve the user's existing key, got: {result}"
        );
        assert!(
            result.contains("TEMPLATE_KEY=value"),
            "upgrade must add the new template key, got: {result}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_mode_quiet_merge_does_not_truncate_gitignore() {
        // Regression test for the bug where `left_pocket -u` flattened a project's
        // multi-line .gitignore down to the template's single `.env` line.
        let dir = std::env::temp_dir().join("pocket_test_upgrade_gitignore_preserved");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("left_pocket");
        fs::create_dir_all(&pocket_dir).unwrap();

        let user_gitignore = ".env\ntarget/**/*\nnode_modules/\n*.log\n.DS_Store\n";
        fs::write(pocket_dir.join(".gitignore"), user_gitignore).unwrap();

        let templates = vec![Template {
            destination: ".gitignore".to_string(),
            content: ".env\n".to_string(),
            quiet_merge: true,
            merge_at_runtime: false,
            source_path: dir.join("gitignore-template.md"),
        }];
        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "hash");

        apply_template_set(
            &templates,
            &pocket_dir,
            &ctx,
            false,
            TemplateApplyMode::Upgrade,
        )
        .unwrap();

        let result = fs::read_to_string(pocket_dir.join(".gitignore")).unwrap();
        assert_eq!(
            result, user_gitignore,
            "the template adds nothing new, so the .gitignore must be untouched"
        );
        for needed in ["target/**/*", "node_modules/", "*.log", ".DS_Store"] {
            assert!(
                result.contains(needed),
                "upgrade destroyed .gitignore entry {needed}, got: {result}"
            );
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_create_mode_filters_beads_dir_from_env() {
        let dir = std::env::temp_dir().join("spocket_test_create_filters_beads_env");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("left_pocket");
        fs::create_dir_all(&pocket_dir).unwrap();

        let templates = vec![Template {
            destination: ".env".to_string(),
            content: "LEFT_POCKET_ROOT={{LEFT_POCKET_ROOT}}\nSPOCKET_ROOT={{LEFT_POCKET_ROOT}}\nBEADS_DIR={{LEFT_POCKET_ROOT}}/.beads\n"
                .to_string(),
            quiet_merge: true,
            merge_at_runtime: false,
            source_path: dir.join("env-template.md"),
        }];
        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "hash");

        apply_template_set(
            &templates,
            &pocket_dir,
            &ctx,
            false,
            TemplateApplyMode::Create,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(pocket_dir.join(".env")).unwrap(),
            format!(
                "LEFT_POCKET_ROOT={}\nSPOCKET_ROOT={}\n",
                pocket_dir.display(),
                pocket_dir.display()
            )
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_mode_expands_template_variables_in_env_files() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_expands_env_values");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("left_pocket");
        let project_dir = dir.join("project");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let templates = vec![
            Template {
                destination: "{{PROJECT_ROOT}}/.env".to_string(),
                content: "LEFT_POCKET_ROOT={{LEFT_POCKET_ROOT}}\nSPOCKET_ROOT={{LEFT_POCKET_ROOT}}\n"
                    .to_string(),
                quiet_merge: true,
                merge_at_runtime: false,
                source_path: dir.join("project-env-template.md"),
            },
            Template {
                destination: "{{SPOCKET_ROOT}}/.env".to_string(),
                content: "PROJECT_ROOT={{PROJECT_ROOT}}\nLEFT_POCKET_ROOT={{LEFT_POCKET_ROOT}}\nSPOCKET_ROOT={{LEFT_POCKET_ROOT}}\n"
                    .to_string(),
                quiet_merge: true,
                merge_at_runtime: false,
                source_path: dir.join("safe-pocket-env-template.md"),
            },
        ];
        let ctx = make_ctx(
            &pocket_dir.to_string_lossy(),
            &project_dir.to_string_lossy(),
            "hash",
        );

        apply_template_set(
            &templates,
            &pocket_dir,
            &ctx,
            false,
            TemplateApplyMode::Upgrade,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(project_dir.join(".env")).unwrap(),
            format!(
                "LEFT_POCKET_ROOT={}\nSPOCKET_ROOT={}\n",
                pocket_dir.display(),
                pocket_dir.display()
            )
        );
        assert_eq!(
            fs::read_to_string(pocket_dir.join(".env")).unwrap(),
            format!(
                "PROJECT_ROOT={}\nLEFT_POCKET_ROOT={}\nSPOCKET_ROOT={}\n",
                project_dir.display(),
                pocket_dir.display(),
                pocket_dir.display()
            )
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_runtime_strips_legacy_beads_and_injects_task_block() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_runtime_strips_legacy_beads");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("left_pocket");
        fs::create_dir_all(&pocket_dir).unwrap();

        let file = pocket_dir.join("AGENTS.md");
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
        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "hash");

        let content = runtime_content_for_template(&tmpl, &ctx);
        inject_runtime_content(&file, &content).unwrap();

        let updated = fs::read_to_string(&file).unwrap();
        // The legacy beads block must be gone entirely.
        assert!(!updated.contains(LEGACY_BEADS_BEGIN_MARKER));
        // The new task block lives inside the runtime markers.
        let start_idx = updated.find(RUNTIME_START_MARKER).unwrap();
        let runtime_block = &updated[start_idx..];
        assert!(runtime_block.contains("<!-- BEGIN POCKET TASK INTEGRATION -->"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_install_replacing_overwrites_and_clears_legacy_files() {
        let base = std::env::temp_dir().join("pocket_test_install_replacing");
        let _ = fs::remove_dir_all(&base);
        let config_dir = base.join("config");
        let registry_root = base.join("registry");
        let tmpl_dir = config_dir.join("templates");
        fs::create_dir_all(&tmpl_dir).unwrap();
        fs::create_dir_all(&registry_root).unwrap();

        // Pre-seed a stale legacy-named template the new embedded set no longer
        // ships, plus a user-customised AGENTS.md that should be overwritten.
        fs::write(tmpl_dir.join("safe_pocket.env.md"), "STALE\n").unwrap();
        fs::write(tmpl_dir.join("AGENTS.md"), "USER CUSTOM\n").unwrap();

        install_embedded_templates_replacing(&config_dir, &registry_root).unwrap();

        // Legacy-named file must be gone (templates/ was cleared).
        assert!(
            !tmpl_dir.join("safe_pocket.env.md").exists(),
            "replace mode must clear legacy-named template files"
        );
        // AGENTS.md must be overwritten with the embedded content.
        let agents = fs::read_to_string(tmpl_dir.join("AGENTS.md")).unwrap();
        assert!(agents.contains("#POCKET_TEMPLATE_DESTINATION"));
        assert!(!agents.contains("USER CUSTOM"));
        // The new left_pocket.env.md must be present.
        assert!(tmpl_dir.join("left_pocket.env.md").exists());

        let _ = fs::remove_dir_all(&base);
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
    fn test_install_interprets_config_and_stages_pocket_templates() {
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
            !ds.contains("#POCKET_INSTALL_DESTINATION"),
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

        // logo.md is installed directly to the config root and is editable at
        // runtime; it is not mirrored into templates/.
        assert!(
            config_dir.join("logo.md").is_file(),
            "logo.md should be installed to the config root"
        );
        let logo = fs::read_to_string(config_dir.join("logo.md")).unwrap();
        assert!(
            !logo.contains("#POCKET_INSTALL_DESTINATION"),
            "INSTALL directive must be stripped from the placed logo"
        );
        assert!(
            logo.contains("{pocket_id}"),
            "placed logo must retain the pocket-id placeholder"
        );
        assert!(
            !config_dir.join("templates/values/logo.md").exists(),
            "a file with INSTALL_DESTINATION must not be mirrored into templates/"
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
                .contains("#POCKET_TEMPLATE_DESTINATION"),
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
    fn test_expand_install_roots_leaves_pocket_vars_literal() {
        let out = expand_install_roots(
            "cfg={{LEFT_POCKET_CONFIG_ROOT}} reg={{LEFT_POCKET_REGISTRY_ROOT}} pkt={{LEFT_POCKET_ROOT}}",
            Path::new("/cfg"),
            Path::new("/reg"),
        );
        assert_eq!(out, "cfg=/cfg reg=/reg pkt={{LEFT_POCKET_ROOT}}");
    }

    #[test]
    fn test_parse_install_destination_directive() {
        assert_eq!(
            parse_install_destination_directive("#LEFT_POCKET_INSTALL_DESTINATION: /a/b.yaml"),
            Some("/a/b.yaml".to_string())
        );
        assert_eq!(
            parse_install_destination_directive("#LEFT_POCKET_INSTALL_DESTINATION /a/b.yaml"),
            Some("/a/b.yaml".to_string())
        );
        // Legacy SPOCKET form is still recognised.
        assert_eq!(
            parse_install_destination_directive("#LEFT_POCKET_INSTALL_DESTINATION: /a/b.yaml"),
            Some("/a/b.yaml".to_string())
        );
        // Must not match the runtime directive or arbitrary lines.
        assert_eq!(
            parse_install_destination_directive("#LEFT_POCKET_TEMPLATE_DESTINATION: /x"),
            None
        );
        assert_eq!(parse_install_destination_directive("plain text"), None);
    }

    #[test]
    fn test_strip_install_directives_keeps_template_directive() {
        let content = "#POCKET_INSTALL_DESTINATION: {{SPOCKET_CONFIG_ROOT}}/templates/x.md\n\
                       #POCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/.github/x.md\n\
                       body line\n";
        let out = strip_install_directives(content);
        assert!(
            !out.contains("#POCKET_INSTALL_DESTINATION"),
            "INSTALL directive must be removed"
        );
        assert!(
            out.contains("#POCKET_TEMPLATE_DESTINATION"),
            "TEMPLATE directive must be preserved"
        );
        assert!(out.contains("body line"));
    }

    #[test]
    fn test_strip_install_directives_trims_leading_blank() {
        let content = "#POCKET_INSTALL_DESTINATION: /a.yaml\n\nreal content\n";
        let out = strip_install_directives(content);
        assert_eq!(out, "real content\n");
    }

    // ── apply_quiet_merge_template_set ────────────────────────────────────────

    /// Helper: build a single quiet-merge Template pointing at `destination`
    /// (relative or absolute) with the given `content`.
    fn qm_template(destination: &str, content: &str) -> Template {
        Template {
            destination: destination.to_string(),
            content: content.to_string(),
            quiet_merge: true,
            merge_at_runtime: false,
            source_path: PathBuf::from("test-template.md"),
        }
    }

    #[test]
    fn test_quiet_merge_creates_missing_project_env() {
        // Simulates the "challenges" scenario: a left_pocket that was created before
        // project.env.md existed.  The project directory exists but has no .env.
        let dir = std::env::temp_dir().join("pocket_test_quiet_merge_creates_project_env");
        let _ = fs::remove_dir_all(&dir);

        let pocket_dir = dir.join("left_pocket");
        let project_dir = dir.join("project");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let project_str = project_dir.to_string_lossy().into_owned();
        let pocket_str = pocket_dir.to_string_lossy().into_owned();

        let ctx = make_ctx(&pocket_str, &project_str, "testhash");

        // Template targets PROJECT_ROOT/.env — mimics project.env.md
        let templates = vec![qm_template(
            &format!("{}/.env", project_str),
            "PROJECT_ROOT={{PROJECT_ROOT}}\nLEFT_POCKET_ROOT={{LEFT_POCKET_ROOT}}\n",
        )];

        let count = apply_quiet_merge_template_set(&templates, &pocket_dir, &ctx).unwrap();

        assert_eq!(count, 1, "should have written 1 file");

        let env_content = fs::read_to_string(project_dir.join(".env")).unwrap();
        assert!(
            env_content.contains(&format!("PROJECT_ROOT={}", project_str)),
            "project .env must contain PROJECT_ROOT, got: {env_content}"
        );
        assert!(
            env_content.contains(&format!("LEFT_POCKET_ROOT={}", pocket_str)),
            "project .env must contain LEFT_POCKET_ROOT, got: {env_content}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_quiet_merge_adds_missing_key_to_existing_env() {
        // left_pocket .env exists but is missing PROJECT_ROOT (e.g. written by the
        // old sync_root_env_file which only wrote LEFT_POCKET_ROOT).
        let dir = std::env::temp_dir().join("pocket_test_quiet_merge_adds_missing_key");
        let _ = fs::remove_dir_all(&dir);

        let pocket_dir = dir.join("left_pocket");
        let project_dir = dir.join("project");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let project_str = project_dir.to_string_lossy().into_owned();
        let pocket_str = pocket_dir.to_string_lossy().into_owned();

        // Pre-populate with only LEFT_POCKET_ROOT (simulates migrate_post_install_root_state output)
        let existing_env = format!("LEFT_POCKET_ROOT={}\n", pocket_str);
        fs::write(project_dir.join(".env"), &existing_env).unwrap();

        let ctx = make_ctx(&pocket_str, &project_str, "testhash");

        let templates = vec![qm_template(
            &format!("{}/.env", project_str),
            "PROJECT_ROOT={{PROJECT_ROOT}}\nLEFT_POCKET_ROOT={{LEFT_POCKET_ROOT}}\n",
        )];

        let count = apply_quiet_merge_template_set(&templates, &pocket_dir, &ctx).unwrap();

        assert_eq!(count, 1, "should have written 1 file (added PROJECT_ROOT)");

        let env_content = fs::read_to_string(project_dir.join(".env")).unwrap();
        assert!(
            env_content.contains(&format!("PROJECT_ROOT={}", project_str)),
            "PROJECT_ROOT must have been added, got: {env_content}"
        );
        assert!(
            env_content.contains(&format!("LEFT_POCKET_ROOT={}", pocket_str)),
            "existing LEFT_POCKET_ROOT must be preserved, got: {env_content}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_quiet_merge_is_idempotent_when_file_complete() {
        // Calling apply_quiet_merge_template_set twice on a fully-populated file
        // must be a no-op on the second call.
        let dir = std::env::temp_dir().join("pocket_test_quiet_merge_idempotent");
        let _ = fs::remove_dir_all(&dir);

        let pocket_dir = dir.join("left_pocket");
        let project_dir = dir.join("project");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let project_str = project_dir.to_string_lossy().into_owned();
        let pocket_str = pocket_dir.to_string_lossy().into_owned();

        let ctx = make_ctx(&pocket_str, &project_str, "testhash");

        let templates = vec![qm_template(
            &format!("{}/.env", project_str),
            "PROJECT_ROOT={{PROJECT_ROOT}}\nLEFT_POCKET_ROOT={{LEFT_POCKET_ROOT}}\n",
        )];

        // First call — creates the file
        apply_quiet_merge_template_set(&templates, &pocket_dir, &ctx).unwrap();

        let after_first = fs::read_to_string(project_dir.join(".env")).unwrap();

        // Second call — must be a no-op
        let count = apply_quiet_merge_template_set(&templates, &pocket_dir, &ctx).unwrap();

        assert_eq!(count, 0, "second call must be a no-op");

        let after_second = fs::read_to_string(project_dir.join(".env")).unwrap();
        assert_eq!(
            after_first, after_second,
            "file content must be unchanged after second call"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_quiet_merge_preserves_existing_user_content() {
        // User has custom keys in their .env — they must not be removed.
        let dir = std::env::temp_dir().join("pocket_test_quiet_merge_preserves_user_content");
        let _ = fs::remove_dir_all(&dir);

        let pocket_dir = dir.join("left_pocket");
        let project_dir = dir.join("project");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let project_str = project_dir.to_string_lossy().into_owned();
        let pocket_str = pocket_dir.to_string_lossy().into_owned();

        // Existing file has user content AND the left_pocket keys are missing
        let existing_env = "DATABASE_URL=postgres://localhost/mydb\nSECRET_KEY=supersecret\n";
        fs::write(project_dir.join(".env"), existing_env).unwrap();

        let ctx = make_ctx(&pocket_str, &project_str, "testhash");

        let templates = vec![qm_template(
            &format!("{}/.env", project_str),
            "PROJECT_ROOT={{PROJECT_ROOT}}\nLEFT_POCKET_ROOT={{LEFT_POCKET_ROOT}}\n",
        )];

        apply_quiet_merge_template_set(&templates, &pocket_dir, &ctx).unwrap();

        let env_content = fs::read_to_string(project_dir.join(".env")).unwrap();
        assert!(
            env_content.contains("DATABASE_URL=postgres://localhost/mydb"),
            "user DATABASE_URL must be preserved, got: {env_content}"
        );
        assert!(
            env_content.contains("SECRET_KEY=supersecret"),
            "user SECRET_KEY must be preserved, got: {env_content}"
        );
        assert!(
            env_content.contains(&format!("PROJECT_ROOT={}", project_str)),
            "PROJECT_ROOT must have been added, got: {env_content}"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_quiet_merge_skips_non_quiet_templates() {
        // Templates without quiet_merge must not be touched by apply_quiet_merge_template_set.
        let dir = std::env::temp_dir().join("pocket_test_quiet_merge_skips_non_quiet");
        let _ = fs::remove_dir_all(&dir);

        let pocket_dir = dir.join("left_pocket");
        fs::create_dir_all(&pocket_dir).unwrap();

        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "testhash");

        let templates = vec![Template {
            destination: "should_not_be_created.txt".to_string(),
            content: "some content\n".to_string(),
            quiet_merge: false,
            merge_at_runtime: false,
            source_path: PathBuf::from("test.md"),
        }];

        let count = apply_quiet_merge_template_set(&templates, &pocket_dir, &ctx).unwrap();
        assert_eq!(count, 0, "non-quiet template must be skipped");
        assert!(
            !pocket_dir.join("should_not_be_created.txt").exists(),
            "file must not be created for non-quiet template"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
