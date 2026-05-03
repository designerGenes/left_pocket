use anyhow::{anyhow, Context, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

// ── Default assets (embedded at compile time) ────────────────────────────────
// These are the "factory defaults" that ship with spocket. On first run they
// are copied into the user's template directory unless the user already has
// their own version of each file.

pub const DEFAULT_COPILOT_INSTRUCTIONS: &str = include_str!("defaults/copilot-instructions.md");
pub const DEFAULT_AGENTS_MD: &str = include_str!("defaults/AGENTS.md");
pub const DEFAULT_DIRECTORY_STRUCTURE: &str = include_str!("defaults/directory_structure.md");
pub const DEFAULT_PROJECT_ENV: &str = include_str!("defaults/project.env.md");
pub const DEFAULT_SAFE_POCKET_ENV: &str = include_str!("defaults/safe_pocket.env.md");
pub const DEFAULT_PROJECT_GITIGNORE: &str = include_str!("defaults/gitignore.md");
pub const DEFAULT_SAFE_POCKET_GITIGNORE: &str = include_str!("defaults/safe_pocket.gitignore.md");
pub const DEFAULT_TALK_LIKE_A_CAT_PROMPT: &str =
    include_str!("defaults/prompts/TalkLikeACat.prompt.md");

// ── Template variables ───────────────────────────────────────────────────────

/// Context needed to expand template variables.
pub struct TemplateContext {
    /// Absolute path to the safe pocket root (e.g. `~/.safe_pocket/19930adf3aaa`).
    pub spocket_root: PathBuf,
    /// Absolute path to the primary project directory.
    pub project_root: PathBuf,
    /// Short name of the safe pocket (the directory basename / hash).
    pub spocket_name: String,
    /// Absolute path to the global observations directory (`~/.safe_pocket/observations`).
    pub global_observations_path: PathBuf,
    /// Whether this pocket has Beads integration enabled.
    pub uses_beads: bool,
}

const BEADS_RUNTIME_BLOCK: &str = r#"<!-- BEGIN BEADS INTEGRATION -->
## Issue Tracking with bd (beads)

**IMPORTANT**: This project uses **bd (beads)** for ALL issue tracking. Do NOT use markdown TODOs, task lists, or other tracking methods.

### Why bd?

- Dependency-aware: Track blockers and relationships between issues
- Git-friendly: Dolt-powered version control with native sync
- Agent-optimized: JSON output, ready work detection, discovered-from links
- Prevents duplicate tracking systems and confusion

### Quick Start

**Check ready work:**

```bash
bd ready --json
```

**Create new issues:**

```bash
bd create "Issue title" --description="Detailed context" -t bug|feature|task -p 0-4 --json
bd create "Issue title" --description="What this issue is about" -p 1 --deps discovered-from:bd-123 --json
```

**Claim and update:**

```bash
bd update <id> --claim --json
bd update bd-42 --priority 1 --json
```

**Complete work:**

```bash
bd close bd-42 --reason "Completed" --json
```

### Issue Types

- `bug` - Something broken
- `feature` - New functionality
- `task` - Work item (tests, docs, refactoring)
- `epic` - Large feature with subtasks
- `chore` - Maintenance (dependencies, tooling)

### Priorities

- `0` - Critical (security, data loss, broken builds)
- `1` - High (major features, important bugs)
- `2` - Medium (default, nice-to-have)
- `3` - Low (polish, optimization)
- `4` - Backlog (future ideas)

### Workflow for AI Agents

1. **Check ready work**: `bd ready` shows unblocked issues
2. **Claim your task atomically**: `bd update <id> --claim`
3. **Work on it**: Implement, test, document
4. **Discover new work?** Create linked issue:
   - `bd create "Found bug" --description="Details about what was found" -p 1 --deps discovered-from:<parent-id>`
5. **Complete**: `bd close bd-42 --reason "Done"`

### Auto-Sync

bd automatically syncs via Dolt:

- Each write auto-commits to Dolt history
- Use `bd dolt push`/`bd dolt pull` for remote sync
- No manual export/import needed!

### Important Rules

- ✅ Use bd for ALL task tracking
- ✅ Always use `--json` flag for programmatic use
- ✅ Link discovered work with `discovered-from` dependencies
- ✅ Check `bd ready` before asking "what should I work on?"
- ❌ Do NOT create markdown TODO lists
- ❌ Do NOT use external issue trackers
- ❌ Do NOT duplicate tracking systems

For more details, see README.md and docs/QUICKSTART.md.

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

<!-- END BEADS INTEGRATION -->
"#;

/// Replace `{{SPOCKET_ROOT}}`, `{{PROJECT_ROOT}}`, `{{SPOCKET_NAME}}`,
/// and `{{GLOBAL_OBSERVATIONS_PATH}}` in `text`.
pub fn expand_variables(text: &str, ctx: &TemplateContext) -> String {
    text.replace("{{SPOCKET_ROOT}}", &ctx.spocket_root.to_string_lossy())
        .replace("{{PROJECT_ROOT}}", &ctx.project_root.to_string_lossy())
        .replace("{{SPOCKET_NAME}}", &ctx.spocket_name)
        .replace(
            "{{GLOBAL_OBSERVATIONS_PATH}}",
            &ctx.global_observations_path.to_string_lossy(),
        )
}

fn filter_template_content(content: &str, ctx: &TemplateContext) -> String {
    if ctx.uses_beads {
        return content.to_string();
    }

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

    if ctx.uses_beads && tmpl.destination == "{{SPOCKET_ROOT}}/AGENTS.md" {
        sections.push(BEADS_RUNTIME_BLOCK.trim().to_string());
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
    /// Relative destination path inside the pocket (after variable expansion).
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

/// Parse a single template file.
///
/// The first line **must** start with `#SPOCKET_TEMPLATE_DESTINATION` followed
/// by a colon (optional) and the destination path. All subsequent lines that
/// start with `#SPOCKET` are treated as metadata and stripped. Everything else
/// becomes the template content.
///
/// Recognised metadata directives:
/// - `#SPOCKET_QUIET_MERGE` — merge with existing file instead of overwriting.
pub fn parse_template(path: &Path) -> Result<Template> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read template file: {}", path.display()))?;

    let mut lines = raw.lines();

    // First line must be the destination directive
    let first_line = lines
        .next()
        .ok_or_else(|| anyhow!("Template file is empty: {}", path.display()))?;

    let destination = parse_destination_directive(first_line).ok_or_else(|| {
        anyhow!(
            "Template file '{}' is missing #SPOCKET_TEMPLATE_DESTINATION on line 1.\n\
                 Found: {}",
            path.display(),
            first_line
        )
    })?;

    // Collect remaining lines, detecting directives and stripping any that start with #SPOCKET
    let mut quiet_merge = false;
    let mut merge_at_runtime = false;
    let content_lines: Vec<&str> = lines
        .filter(|line| {
            if line.trim() == "#SPOCKET_QUIET_MERGE" {
                quiet_merge = true;
                false
            } else if line.trim() == "#SPOCKET_MERGE_AT_RUNTIME" {
                merge_at_runtime = true;
                false
            } else {
                !line.starts_with("#SPOCKET")
            }
        })
        .collect();
    let mut content = content_lines.join("\n");

    // Preserve trailing newline if original file had one
    if raw.ends_with('\n') && !content.ends_with('\n') {
        content.push('\n');
    }

    // Trim a single leading newline if present (common after the directive line)
    let content = if content.starts_with('\n') {
        content[1..].to_string()
    } else {
        content
    };

    Ok(Template {
        destination,
        content,
        quiet_merge,
        merge_at_runtime,
        source_path: path.to_path_buf(),
    })
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
/// Indentation is expressed by leading `- ` prefixes. Each `- ` adds one level
/// of nesting under the most recent parent at the preceding depth.
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
                    "directory_structure.md line {}: indented entry '{}' has no parent",
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
    let mut rest = line;

    while rest.starts_with("- ") {
        depth += 1;
        rest = &rest[2..];
    }

    // Also handle "- " at the very end without a trailing space
    let name = rest.trim().to_string();
    (depth, name)
}

// ── Template directory helpers ───────────────────────────────────────────────

/// Returns the path to the safe_pocket config directory (`$HOME/.config/safe_pocket`).
pub fn safe_pocket_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".config").join("safe_pocket"))
}

/// Returns the path to the templates directory (`$HOME/.config/safe_pocket/templates`).
pub fn templates_dir() -> Result<PathBuf> {
    Ok(safe_pocket_config_dir()?.join("templates"))
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

    let prompts_dir = tmpl_dir.join("prompts");
    fs::create_dir_all(&prompts_dir).context("Failed to create templates/prompts directory")?;

    // Ensure global observations directory exists in the registry, not config.
    let _ = crate::registry::global_observations_dir()?;

    // Default template files
    let defaults: &[(&str, &str)] = &[
        (
            "templates/copilot-instructions.md",
            DEFAULT_COPILOT_INSTRUCTIONS,
        ),
        ("templates/AGENTS.md", DEFAULT_AGENTS_MD),
        ("templates/project.env.md", DEFAULT_PROJECT_ENV),
        ("templates/safe_pocket.env.md", DEFAULT_SAFE_POCKET_ENV),
        ("templates/gitignore.md", DEFAULT_PROJECT_GITIGNORE),
        (
            "templates/safe_pocket.gitignore.md",
            DEFAULT_SAFE_POCKET_GITIGNORE,
        ),
        (
            "templates/prompts/TalkLikeACat.prompt.md",
            DEFAULT_TALK_LIKE_A_CAT_PROMPT,
        ),
    ];

    for (rel_path, content) in defaults {
        let target = config_dir.join(rel_path);
        if !target.exists() {
            fs::write(&target, content)
                .with_context(|| format!("Failed to write default asset: {}", target.display()))?;
            if crate::verbose() {
                println!(
                    "{} {}",
                    "Installed default template:".bright_green(),
                    target.display().to_string().dimmed()
                );
            }
        }
    }

    // Default directory structure
    let dir_struct_path = config_dir.join("directory_structure.md");
    if !dir_struct_path.exists() {
        fs::write(&dir_struct_path, DEFAULT_DIRECTORY_STRUCTURE).with_context(|| {
            format!(
                "Failed to write default directory structure: {}",
                dir_struct_path.display()
            )
        })?;
        if crate::verbose() {
            println!(
                "{} {}",
                "Installed default directory structure:".bright_green(),
                dir_struct_path.display().to_string().dimmed()
            );
        }
    }

    Ok(())
}

/// Load all template files from the templates directory, walking subdirectories recursively.
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
                dirs_to_visit.push(path);
                continue;
            }

            if !path.is_file() {
                continue;
            }

            match parse_template(&path) {
                Ok(tmpl) => templates.push(tmpl),
                Err(e) => {
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

    Ok(templates)
}

/// Load the directory structure, respecting project-local override.
///
/// 1. If `project_dir` contains a `directory_template.md`, use it (project-local takes precedence).
/// 2. Otherwise, look in `$HOME/.config/safe_pocket/` for *exactly one* file matching
///    `directory_structure.md`. Error if more than one is found.
/// 3. If neither exists, return an empty list.
pub fn load_directory_structure(project_dir: Option<&Path>) -> Result<Vec<PathBuf>> {
    // Check project-local first
    if let Some(proj) = project_dir {
        let local_file = proj.join("directory_template.md");
        if local_file.exists() {
            let content = fs::read_to_string(&local_file)
                .context("Failed to read project-local directory_template.md")?;
            return parse_directory_structure(&content);
        }
    }

    let config_dir = safe_pocket_config_dir()?;

    // Count directory structure files in config dir (only top-level)
    let mut structure_files: Vec<PathBuf> = Vec::new();
    if config_dir.exists() {
        for entry in fs::read_dir(&config_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name == "directory_structure.md" || name == "directory_template.md" {
                        structure_files.push(path);
                    }
                }
            }
        }
    }

    match structure_files.len() {
        0 => Ok(Vec::new()),
        1 => {
            let content = fs::read_to_string(&structure_files[0])?;
            parse_directory_structure(&content)
        }
        n => Err(anyhow!(
            "Found {} directory structure files in {}, expected at most 1:\n{}",
            n,
            config_dir.display(),
            structure_files
                .iter()
                .map(|p| format!("  - {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        )),
    }
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
        "<!-- BEGIN BEADS INTEGRATION -->",
        "<!-- END BEADS INTEGRATION -->",
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

// ── Apply templates to a pocket ──────────────────────────────────────────────

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

/// Apply all loaded templates to a pocket directory.
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
        // content variables for replacement when the pocket is opened.
        let content = if tmpl.merge_at_runtime {
            String::new()
        } else {
            expand_template_content(&filter_template_content(&tmpl.content, ctx), ctx)
        };

        // Resolve the destination: if it starts with the spocket_root, make it
        // relative to the pocket dir. Otherwise treat it as relative to pocket dir.
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

/// Resolve a template destination path to an absolute path.
///
/// After variable expansion, the destination might be:
/// - An absolute path (e.g. `/Users/.../pocket/AGENTS.md`) → use as-is
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

/// Upgrade an existing pocket to match the current templates.
///
/// This is called by `spocket -u <path>`. It does NOT open the workspace;
/// it resets non-runtime template destinations to match the templates while
/// preserving merge-at-runtime destinations for runtime injection.
pub fn upgrade_pocket(pocket_dir: &Path) -> Result<()> {
    // Validate the pocket directory exists and has a manifest
    if !pocket_dir.exists() {
        return Err(anyhow!(
            "Pocket directory does not exist: {}",
            pocket_dir.display()
        ));
    }

    let manifest_path = pocket_dir.join("manifest.json");
    if !manifest_path.exists() {
        return Err(anyhow!(
            "No manifest.json found in {}. Is this a valid safe pocket?",
            pocket_dir.display()
        ));
    }

    // Load manifest to get core_paths
    let manifest = crate::manifest::Manifest::load(pocket_dir)?
        .ok_or_else(|| anyhow!("Failed to load manifest from {}", pocket_dir.display()))?;

    let spocket_name = pocket_dir
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
            .join(".safe_pocket")
            .join("observations")
    });

    let ctx = TemplateContext {
        spocket_root: pocket_dir.to_path_buf(),
        project_root: project_root.clone(),
        spocket_name,
        global_observations_path: global_obs,
        uses_beads: manifest.uses_beads,
    };

    println!(
        "{} {}",
        "Upgrading pocket:".bright_white().bold(),
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
            "Pocket is already up to date with templates.".bright_green()
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
            uses_beads: false,
        }
    }

    fn make_ctx_with_beads(
        spocket_root: &str,
        project_root: &str,
        name: &str,
        uses_beads: bool,
    ) -> TemplateContext {
        let mut ctx = make_ctx(spocket_root, project_root, name);
        ctx.uses_beads = uses_beads;
        ctx
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

        let tmpl = parse_template(&file).unwrap();
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

        let tmpl = parse_template(&file).unwrap();
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
        let ctx = make_ctx("/pocket", "/project", "hash");
        let pocket = PathBuf::from("/pocket");

        let result = resolve_template_destination(".github/test.md", &pocket, &ctx);
        assert_eq!(result, PathBuf::from("/pocket/.github/test.md"));
    }

    #[test]
    fn test_resolve_destination_absolute() {
        let ctx = make_ctx("/pocket", "/project", "hash");
        let pocket = PathBuf::from("/pocket");

        let result = resolve_template_destination("/pocket/AGENTS.md", &pocket, &ctx);
        assert_eq!(result, PathBuf::from("/pocket/AGENTS.md"));
    }

    // ── apply_templates (integration) ───────────────────────────────────

    #[test]
    fn test_apply_templates_creates_dirs_and_files() {
        // This test creates a mini template setup in a temp dir and verifies
        // that apply_templates creates the expected structure.
        let base = std::env::temp_dir().join("spocket_test_apply");
        let _ = fs::remove_dir_all(&base);

        let pocket_dir = base.join("pocket");
        let config_dir = base.join("config");
        let tmpl_dir = config_dir.join("templates");
        fs::create_dir_all(&tmpl_dir).unwrap();
        fs::create_dir_all(&pocket_dir).unwrap();

        // Write a template
        fs::write(
            tmpl_dir.join("test.md"),
            "#SPOCKET_TEMPLATE_DESTINATION: subdir/test.md\nHello {{SPOCKET_NAME}}\n",
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
        let tmpl = parse_template(&tmpl_dir.join("test.md")).unwrap();
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

        let tmpl = parse_template(&file).unwrap();
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

        let tmpl = parse_template(&file).unwrap();
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

        let tmpl = parse_template(&file).unwrap();
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
        let ctx = make_ctx("/pocket", "/project", "hash");
        let input = "Path: {{SPOCKET_ROOT}}\n#SPOCKET_NOTE: {{SPOCKET_ROOT}}\n";

        assert_eq!(
            expand_runtime_variables_in_content(input, &ctx),
            "Path: /pocket\n#SPOCKET_NOTE: {{SPOCKET_ROOT}}\n"
        );
    }

    #[test]
    fn test_filter_template_content_removes_beads_dir_when_disabled() {
        let ctx = make_ctx("/pocket", "/project", "hash");
        let content = "SPOCKET_ROOT={{SPOCKET_ROOT}}\nBEADS_DIR={{SPOCKET_ROOT}}/.beads\n";

        assert_eq!(
            filter_template_content(content, &ctx),
            "SPOCKET_ROOT={{SPOCKET_ROOT}}\n"
        );
    }

    #[test]
    fn test_runtime_content_for_agents_includes_beads_block_when_enabled() {
        let ctx = make_ctx_with_beads("/pocket", "/project", "hash", true);
        let tmpl = Template {
            destination: "{{SPOCKET_ROOT}}/AGENTS.md".to_string(),
            content: "Base runtime\n".to_string(),
            quiet_merge: false,
            merge_at_runtime: true,
            source_path: PathBuf::from("/tmp/AGENTS.md"),
        };

        let content = runtime_content_for_template(&tmpl, &ctx);
        assert!(content.contains("Base runtime"));
        assert!(content.contains("<!-- BEGIN BEADS INTEGRATION -->"));
    }

    #[test]
    fn test_create_mode_places_empty_merge_at_runtime_destination() {
        let dir = std::env::temp_dir().join("spocket_test_create_places_empty_runtime");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("pocket");
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
        let pocket_dir = dir.join("pocket");
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
        let pocket_dir = dir.join("pocket");
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
    fn test_upgrade_mode_resets_quiet_merge_template() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_resets_quiet_merge");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("pocket");
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

        assert_eq!(
            fs::read_to_string(pocket_dir.join(".env")).unwrap(),
            "TEMPLATE_KEY=value\n"
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_create_mode_filters_beads_dir_from_env_when_disabled() {
        let dir = std::env::temp_dir().join("spocket_test_create_filters_beads_env");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("pocket");
        fs::create_dir_all(&pocket_dir).unwrap();

        let templates = vec![Template {
            destination: ".env".to_string(),
            content: "SPOCKET_ROOT={{SPOCKET_ROOT}}\nBEADS_DIR={{SPOCKET_ROOT}}/.beads\n"
                .to_string(),
            quiet_merge: true,
            merge_at_runtime: false,
            source_path: dir.join("env-template.md"),
        }];
        let ctx = make_ctx(&pocket_dir.to_string_lossy(), "/project", "hash");

        apply_template_set(&templates, &pocket_dir, &ctx, false, TemplateApplyMode::Create)
            .unwrap();

        assert_eq!(
            fs::read_to_string(pocket_dir.join(".env")).unwrap(),
            format!("SPOCKET_ROOT={}\n", pocket_dir.display())
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_mode_expands_template_variables_in_env_files() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_expands_env_values");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("pocket");
        let project_dir = dir.join("project");
        fs::create_dir_all(&pocket_dir).unwrap();
        fs::create_dir_all(&project_dir).unwrap();

        let templates = vec![
            Template {
                destination: "{{PROJECT_ROOT}}/.env".to_string(),
                content: "SPOCKET_ROOT={{SPOCKET_ROOT}}\n".to_string(),
                quiet_merge: true,
                merge_at_runtime: false,
                source_path: dir.join("project-env-template.md"),
            },
            Template {
                destination: "{{SPOCKET_ROOT}}/.env".to_string(),
                content: "PROJECT_ROOT={{PROJECT_ROOT}}\n".to_string(),
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
            format!("SPOCKET_ROOT={}\n", pocket_dir.display())
        );
        assert_eq!(
            fs::read_to_string(pocket_dir.join(".env")).unwrap(),
            format!("PROJECT_ROOT={}\n", project_dir.display())
        );

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_upgrade_runtime_rehomes_beads_block_inside_markers() {
        let dir = std::env::temp_dir().join("spocket_test_upgrade_runtime_rehomes_beads_block");
        let _ = fs::remove_dir_all(&dir);
        let pocket_dir = dir.join("pocket");
        fs::create_dir_all(&pocket_dir).unwrap();

        let file = pocket_dir.join("AGENTS.md");
        fs::write(
            &file,
            format!(
                "{beads}\n{start}\nOld runtime\n{end}\n",
                beads = BEADS_RUNTIME_BLOCK.trim(),
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
        let ctx = make_ctx_with_beads(
            &pocket_dir.to_string_lossy(),
            "/project",
            "hash",
            true,
        );

        let content = runtime_content_for_template(&tmpl, &ctx);
        inject_runtime_content(&file, &content).unwrap();

        let updated = fs::read_to_string(&file).unwrap();
        let start_idx = updated.find(RUNTIME_START_MARKER).unwrap();
        assert!(!updated[..start_idx].contains("<!-- BEGIN BEADS INTEGRATION -->"));
        let runtime_block = &updated[start_idx..];
        assert!(runtime_block.contains("<!-- BEGIN BEADS INTEGRATION -->"));

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
}
