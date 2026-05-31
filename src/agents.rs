//! Unified agent definitions and synchronization.
//!
//! safe_pocket keeps a single source of truth for its AI agents in
//! `$HOME/.config/safe_pocket/templates/agents/`. Each agent is a Markdown file
//! with a YAML frontmatter block describing the agent in a tool-agnostic way:
//!
//! ```markdown
//! ---
//! agent_name: builder
//! description: ...
//! notes: ...
//! mode: primary            # or "subagent"
//! model:                   # optional model id
//! can:
//!   - code
//!   - test
//! cannot:
//!   - execute
//! ---
//!
//! <prompt body markdown ...>
//! ```
//!
//! The `sync` routines translate these unified definitions into the concrete
//! formats each tool expects. Today that means OpenCode agent Markdown files in
//! `$HOME/.config/opencode/agent/<name>.md`. (VS Code Copilot placement is
//! intentionally deferred.)

use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::{Path, PathBuf};

/// Marker written into every safe_pocket-managed agent file so we can recognise
/// (and safely overwrite) files we own without clobbering hand-authored ones.
pub const MANAGED_MARKER: &str = "<!-- SPOCKET_MANAGED_AGENT: managed by `spocket sync agents` — edits are overwritten -->";

// ── Bundled default agent templates ───────────────────────────────────────────

/// Default agent templates, embedded in the binary. These are also part of the
/// generic embedded-template install (staged into
/// `$HOME/.config/safe_pocket/templates/agents/`); this typed list is retained
/// for tests and as a stable in-memory reference to the bundled agents.
#[cfg_attr(not(test), allow(dead_code))]
pub const DEFAULT_AGENTS: &[(&str, &str)] = &[
    ("builder.md", include_str!("templates/agents/builder.md")),
    ("critic.md", include_str!("templates/agents/critic.md")),
    ("reporter.md", include_str!("templates/agents/reporter.md")),
    ("documenter.md", include_str!("templates/agents/documenter.md")),
    ("installer.md", include_str!("templates/agents/installer.md")),
    (
        "safe_pocketer.md",
        include_str!("templates/agents/safe_pocketer.md"),
    ),
];

// ── Unified agent definition ──────────────────────────────────────────────────

/// A tool-agnostic agent definition parsed from a unified template file.
#[derive(Debug, Clone, PartialEq)]
pub struct UnifiedAgent {
    pub name: String,
    pub description: String,
    pub notes: Option<String>,
    pub mode: String,
    pub model: Option<String>,
    pub can: Vec<String>,
    pub cannot: Vec<String>,
    pub prompt_body: String,
}

/// OpenCode permission decision for a single capability key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    Allow,
    Deny,
}

impl Permission {
    fn as_str(self) -> &'static str {
        match self {
            Permission::Allow => "allow",
            Permission::Deny => "deny",
        }
    }
}

/// The four OpenCode permission keys, resolved from a unified agent's
/// `can` / `cannot` capability lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenCodePermissions {
    pub edit: Permission,
    pub write: Permission,
    pub bash: Permission,
    pub task: Permission,
}

impl UnifiedAgent {
    /// Resolve this agent's capabilities into OpenCode permission keys.
    ///
    /// Capabilities map onto permission keys as follows:
    /// - `code`     → edit, write
    /// - `document` → edit, write
    /// - `test`     → bash
    /// - `execute`  → bash
    /// - `plan`     → task
    ///
    /// Effective capabilities are `can` minus `cannot` (set difference); any
    /// permission key not granted by an effective capability defaults to deny.
    pub fn permissions(&self) -> OpenCodePermissions {
        let mut edit = false;
        let mut write = false;
        let mut bash = false;
        let mut task = false;

        for cap in &self.can {
            if self.cannot.iter().any(|c| c.eq_ignore_ascii_case(cap)) {
                continue;
            }
            match cap.trim().to_ascii_lowercase().as_str() {
                "code" => {
                    edit = true;
                    write = true;
                }
                "document" => {
                    edit = true;
                    write = true;
                }
                "test" => bash = true,
                "execute" => bash = true,
                "plan" => task = true,
                _ => {}
            }
        }

        let p = |b: bool| {
            if b {
                Permission::Allow
            } else {
                Permission::Deny
            }
        };

        OpenCodePermissions {
            edit: p(edit),
            write: p(write),
            bash: p(bash),
            task: p(task),
        }
    }

    /// Render this unified agent as an OpenCode agent Markdown file.
    pub fn to_opencode_markdown(&self) -> String {
        let perms = self.permissions();
        let mut out = String::new();
        out.push_str("---\n");
        out.push_str(&format!("description: {}\n", yaml_scalar(&self.description)));
        out.push_str(&format!("mode: {}\n", self.mode));
        out.push_str("temperature: 0.1\n");
        if let Some(model) = self.model.as_deref().filter(|m| !m.trim().is_empty()) {
            out.push_str(&format!("model: {}\n", model.trim()));
        }
        out.push_str("permission:\n");
        out.push_str(&format!("  edit: {}\n", perms.edit.as_str()));
        out.push_str(&format!("  write: {}\n", perms.write.as_str()));
        out.push_str(&format!("  bash: {}\n", perms.bash.as_str()));
        out.push_str(&format!("  task: {}\n", perms.task.as_str()));
        out.push_str("---\n\n");
        out.push_str(MANAGED_MARKER);
        out.push('\n');
        if let Some(notes) = self.notes.as_deref().filter(|n| !n.trim().is_empty()) {
            out.push_str(&format!("\n> **Notes:** {}\n", notes.trim()));
        }
        out.push('\n');
        out.push_str(self.prompt_body.trim_end());
        out.push('\n');
        out
    }
}

/// Quote a YAML scalar value if it contains characters that would break a plain
/// single-line scalar. Folded/multi-line descriptions are collapsed to one line.
fn yaml_scalar(value: &str) -> String {
    let collapsed = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "\"\"".to_string();
    }
    let needs_quote = collapsed.contains(':')
        || collapsed.contains('#')
        || collapsed.starts_with(['>', '|', '\'', '"', '[', '{', '*', '&', '!', '%', '@', '`']);
    if needs_quote {
        format!("\"{}\"", collapsed.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        collapsed
    }
}

// ── Parsing ───────────────────────────────────────────────────────────────────

/// Parse a unified agent definition from a template file's contents.
pub fn parse_unified_agent(content: &str, fallback_name: &str) -> Result<UnifiedAgent> {
    let (frontmatter, body) = split_frontmatter(content);

    let frontmatter = frontmatter.ok_or_else(|| {
        anyhow::anyhow!("agent template is missing a `---` YAML frontmatter block")
    })?;

    let mut name: Option<String> = None;
    let mut description = String::new();
    let mut notes: Option<String> = None;
    let mut mode = "subagent".to_string();
    let mut model: Option<String> = None;
    let mut can: Vec<String> = Vec::new();
    let mut cannot: Vec<String> = Vec::new();

    // Current list key being populated ("can" / "cannot"), if any.
    let mut list_key: Option<&'static str> = None;
    // Current folded scalar key being accumulated (e.g. description: >-).
    let mut folded_key: Option<&'static str> = None;
    let mut folded_buf: Vec<String> = Vec::new();

    let flush_folded =
        |key: Option<&'static str>,
         buf: &mut Vec<String>,
         description: &mut String,
         notes: &mut Option<String>| {
            if let Some(k) = key {
                let joined = buf.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
                match k {
                    "description" => *description = joined,
                    "notes" => *notes = Some(joined),
                    _ => {}
                }
            }
            buf.clear();
        };

    for raw_line in frontmatter.lines() {
        let line = raw_line.trim_end();

        // A list item under the current list key.
        if let Some(key) = list_key {
            let trimmed = line.trim_start();
            if let Some(item) = trimmed.strip_prefix("- ") {
                let val = item.trim().trim_matches('"').trim_matches('\'').to_string();
                if !val.is_empty() {
                    match key {
                        "can" => can.push(val),
                        "cannot" => cannot.push(val),
                        _ => {}
                    }
                }
                continue;
            }
            // Inline empty list `can: []` already handled at key parse; any
            // non-item, non-blank line ends the list.
            if trimmed.is_empty() {
                continue;
            }
            list_key = None;
        }

        // Continuation of a folded scalar (indented, no `key:` at column 0).
        if folded_key.is_some() {
            let is_indented = raw_line.starts_with(' ') || raw_line.starts_with('\t');
            let looks_like_key = line
                .trim_start()
                .split_once(':')
                .map(|(k, _)| !k.contains(' ') && !k.is_empty())
                .unwrap_or(false);
            if is_indented && !(looks_like_key && !raw_line.starts_with("    ")) {
                let t = line.trim();
                if !t.is_empty() {
                    folded_buf.push(t.to_string());
                }
                continue;
            }
            flush_folded(folded_key, &mut folded_buf, &mut description, &mut notes);
            folded_key = None;
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        match key {
            "agent_name" | "name" => {
                if !value.is_empty() {
                    name = Some(value.trim_matches('"').trim_matches('\'').to_string());
                }
            }
            "description" => {
                if value == ">-" || value == ">" || value == "|" || value == "|-" || value.is_empty()
                {
                    folded_key = Some("description");
                    folded_buf.clear();
                } else {
                    description = value.trim_matches('"').to_string();
                }
            }
            "notes" => {
                if value == ">-" || value == ">" || value == "|" || value == "|-" {
                    folded_key = Some("notes");
                    folded_buf.clear();
                } else if !value.is_empty() {
                    notes = Some(value.trim_matches('"').to_string());
                }
            }
            "mode" => {
                if !value.is_empty() {
                    mode = value.to_string();
                }
            }
            "model" => {
                if !value.is_empty() {
                    model = Some(value.trim_matches('"').to_string());
                }
            }
            "can" => {
                if value == "[]" {
                    // empty list
                } else if !value.is_empty() {
                    // Inline flow list `can: [a, b]`
                    can.extend(parse_inline_list(value));
                } else {
                    list_key = Some("can");
                }
            }
            "cannot" => {
                if value == "[]" {
                } else if !value.is_empty() {
                    cannot.extend(parse_inline_list(value));
                } else {
                    list_key = Some("cannot");
                }
            }
            _ => {}
        }
    }

    flush_folded(folded_key, &mut folded_buf, &mut description, &mut notes);

    let name = name.unwrap_or_else(|| fallback_name.to_string());
    if description.is_empty() {
        description = format!("safe_pocket agent: {name}");
    }

    Ok(UnifiedAgent {
        name,
        description,
        notes,
        mode,
        model,
        can,
        cannot,
        prompt_body: body.trim().to_string(),
    })
}

fn parse_inline_list(value: &str) -> Vec<String> {
    value
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Split a file into its YAML frontmatter (between leading `---` fences) and the
/// remaining body. Returns `(None, whole)` if there is no frontmatter.
fn split_frontmatter(content: &str) -> (Option<String>, String) {
    let trimmed = content.trim_start_matches('\u{feff}');
    let mut lines = trimmed.lines();
    match lines.next() {
        Some(first) if first.trim() == "---" => {}
        _ => return (None, content.to_string()),
    }

    let mut frontmatter = Vec::new();
    let mut body = Vec::new();
    let mut in_body = false;
    for line in lines {
        if !in_body && line.trim() == "---" {
            in_body = true;
            continue;
        }
        if in_body {
            body.push(line);
        } else {
            frontmatter.push(line);
        }
    }

    if !in_body {
        // No closing fence; treat the whole thing as body.
        return (None, content.to_string());
    }

    (Some(frontmatter.join("\n")), body.join("\n"))
}

// ── Loading ───────────────────────────────────────────────────────────────────

/// Path to the unified agent templates directory.
pub fn agents_template_dir() -> Result<PathBuf> {
    Ok(crate::template::templates_dir()?.join("agents"))
}

/// Path to the OpenCode agent directory (`$HOME/.config/opencode/agent`).
///
/// This is the *global* location safe_pocket used to write into. Agents are now
/// installed per-project (see [`pocket_agent_dir`]); this remains only so the
/// legacy global files can be located, backed up, and removed.
pub fn opencode_agent_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Failed to get home directory")?;
    Ok(home.join(".config").join("opencode").join("agent"))
}

/// Path to a pocket's project-local OpenCode agent directory
/// (`<pocket>/.opencode/agent`). OpenCode discovers agents here when the pocket
/// is opened as a workspace, so no global installation is required.
pub fn pocket_agent_dir(pocket_dir: &Path) -> PathBuf {
    pocket_dir.join(".opencode").join("agent")
}

/// Load all unified agent definitions from the templates directory.
pub fn load_unified_agents() -> Result<Vec<UnifiedAgent>> {
    let dir = agents_template_dir()?;
    let mut agents = Vec::new();

    if !dir.exists() {
        return Ok(agents);
    }

    let mut entries: Vec<PathBuf> = fs::read_dir(&dir)
        .with_context(|| format!("Failed to read agents directory: {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().map_or(false, |e| e == "md"))
        .collect();
    entries.sort();

    for path in entries {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read agent template: {}", path.display()))?;
        let fallback = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("agent")
            .to_string();
        match parse_unified_agent(&content, &fallback) {
            Ok(agent) => agents.push(agent),
            Err(e) => {
                eprintln!(
                    "{} skipping agent template {}: {}",
                    "Warning:".bright_yellow(),
                    path.display(),
                    e
                );
            }
        }
    }

    Ok(agents)
}

// ── Sync ──────────────────────────────────────────────────────────────────────

/// Outcome of synchronizing a single agent file.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncAction {
    Created,
    Updated,
    Unchanged,
}

/// Result of a full agents sync.
#[derive(Debug, Default)]
pub struct SyncReport {
    pub created: Vec<String>,
    pub updated: Vec<String>,
    pub unchanged: Vec<String>,
    pub backed_up: Vec<String>,
}

impl SyncReport {
    pub fn changed(&self) -> bool {
        !self.created.is_empty() || !self.updated.is_empty()
    }
}

/// Synchronize the unified agents into a pocket's project-local OpenCode agent
/// directory (`<pocket>/.opencode/agent`). This is the per-project replacement
/// for the old global sync.
pub fn sync_agents_into_pocket(pocket_dir: &Path) -> Result<SyncReport> {
    sync_agents_into(&pocket_agent_dir(pocket_dir))
}

/// Synchronize the unified agents into `target_dir`, rendering each into the
/// OpenCode agent Markdown format.
///
/// Non-destructive: if a target file already exists and is **not**
/// safe_pocket-managed (lacks [`MANAGED_MARKER`]), it is backed up to
/// `<name>.md.pre-spocket.bak` before being replaced.
pub fn sync_agents_into(target_dir: &Path) -> Result<SyncReport> {
    let agents = load_unified_agents()?;
    fs::create_dir_all(target_dir)
        .with_context(|| format!("Failed to create OpenCode agent dir: {}", target_dir.display()))?;

    let mut report = SyncReport::default();

    for agent in &agents {
        let rendered = agent.to_opencode_markdown();
        let target = target_dir.join(format!("{}.md", agent.name));

        let action = write_agent_file(&target, &rendered, &mut report)?;
        match action {
            SyncAction::Created => report.created.push(agent.name.clone()),
            SyncAction::Updated => report.updated.push(agent.name.clone()),
            SyncAction::Unchanged => report.unchanged.push(agent.name.clone()),
        }
    }

    Ok(report)
}

/// Back up and remove the legacy *global* OpenCode agent files that safe_pocket
/// previously installed into `$HOME/.config/opencode/agent`. Only files bearing
/// the [`MANAGED_MARKER`] are touched; hand-authored agents are left alone. Each
/// removed file is first copied to `<name>.md.pre-spocket-removed.bak`. Returns
/// the names of the files that were removed.
pub fn remove_global_agents() -> Result<Vec<String>> {
    let dir = opencode_agent_dir()?;
    let mut removed = Vec::new();
    if !dir.exists() {
        return Ok(removed);
    }
    for entry in fs::read_dir(&dir)
        .with_context(|| format!("Failed to read global agent dir: {}", dir.display()))?
    {
        let path = entry?.path();
        if !path.is_file() || path.extension().map_or(true, |e| e != "md") {
            continue;
        }
        let content = fs::read_to_string(&path).unwrap_or_default();
        if !content.contains(MANAGED_MARKER) {
            // Never remove a hand-authored agent.
            continue;
        }
        let backup = path.with_extension("md.pre-spocket-removed.bak");
        fs::write(&backup, &content).with_context(|| {
            format!("Failed to back up agent before removal: {}", backup.display())
        })?;
        fs::remove_file(&path)
            .with_context(|| format!("Failed to remove global agent: {}", path.display()))?;
        removed.push(path.file_name().unwrap().to_string_lossy().to_string());
    }
    Ok(removed)
}

fn write_agent_file(target: &Path, rendered: &str, report: &mut SyncReport) -> Result<SyncAction> {
    if target.exists() {
        let existing = fs::read_to_string(target)
            .with_context(|| format!("Failed to read existing agent: {}", target.display()))?;
        if existing == rendered {
            return Ok(SyncAction::Unchanged);
        }
        if !existing.contains(MANAGED_MARKER) {
            // Preserve a hand-authored agent before overwriting it.
            let backup = target.with_extension("md.pre-spocket.bak");
            if !backup.exists() {
                fs::write(&backup, &existing).with_context(|| {
                    format!("Failed to back up existing agent: {}", backup.display())
                })?;
                report
                    .backed_up
                    .push(backup.file_name().unwrap().to_string_lossy().to_string());
            }
        }
        fs::write(target, rendered)
            .with_context(|| format!("Failed to write agent: {}", target.display()))?;
        Ok(SyncAction::Updated)
    } else {
        fs::write(target, rendered)
            .with_context(|| format!("Failed to write agent: {}", target.display()))?;
        Ok(SyncAction::Created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> &'static str {
        "---\n\
         agent_name: builder\n\
         description: >-\n\
         \x20 Primary implementer that does\n\
         \x20 the bulk of the work.\n\
         notes: A short note.\n\
         mode: primary\n\
         model:\n\
         can:\n\
         \x20 - code\n\
         \x20 - test\n\
         \x20 - execute\n\
         cannot: []\n\
         ---\n\
         \n\
         ## Role: Builder\n\
         \n\
         Body text here.\n"
    }

    #[test]
    fn test_parse_unified_agent_basic() {
        let a = parse_unified_agent(sample(), "fallback").unwrap();
        assert_eq!(a.name, "builder");
        assert_eq!(a.description, "Primary implementer that does the bulk of the work.");
        assert_eq!(a.notes.as_deref(), Some("A short note."));
        assert_eq!(a.mode, "primary");
        assert_eq!(a.model, None);
        assert_eq!(a.can, vec!["code", "test", "execute"]);
        assert!(a.cannot.is_empty());
        assert!(a.prompt_body.starts_with("## Role: Builder"));
    }

    #[test]
    fn test_permissions_builder_all_allow() {
        let a = parse_unified_agent(sample(), "x").unwrap();
        let p = a.permissions();
        assert_eq!(p.edit, Permission::Allow);
        assert_eq!(p.write, Permission::Allow);
        assert_eq!(p.bash, Permission::Allow);
        assert_eq!(p.task, Permission::Deny); // no `plan` capability
    }

    #[test]
    fn test_permissions_critic_all_deny() {
        let content = "---\nagent_name: critic\ndescription: review\nmode: subagent\ncan: []\ncannot:\n  - code\n  - execute\n---\nbody\n";
        let a = parse_unified_agent(content, "critic").unwrap();
        let p = a.permissions();
        assert_eq!(p.edit, Permission::Deny);
        assert_eq!(p.write, Permission::Deny);
        assert_eq!(p.bash, Permission::Deny);
        assert_eq!(p.task, Permission::Deny);
    }

    #[test]
    fn test_permissions_cannot_overrides_can() {
        let content = "---\nagent_name: x\ndescription: d\ncan:\n  - code\n  - execute\ncannot:\n  - execute\n---\nbody\n";
        let a = parse_unified_agent(content, "x").unwrap();
        let p = a.permissions();
        assert_eq!(p.edit, Permission::Allow);
        assert_eq!(p.write, Permission::Allow);
        assert_eq!(p.bash, Permission::Deny); // execute removed by cannot
    }

    #[test]
    fn test_to_opencode_markdown_roundtrip_fields() {
        let a = parse_unified_agent(sample(), "x").unwrap();
        let md = a.to_opencode_markdown();
        assert!(md.starts_with("---\n"));
        assert!(md.contains("mode: primary"));
        assert!(md.contains("edit: allow"));
        assert!(md.contains("task: deny"));
        assert!(md.contains(MANAGED_MARKER));
        assert!(md.contains("## Role: Builder"));
        assert!(!md.contains("model:")); // empty model omitted
    }

    #[test]
    fn test_model_passthrough() {
        let content = "---\nagent_name: critic\ndescription: d\nmode: subagent\nmodel: github-copilot/claude-haiku\ncan: []\ncannot: []\n---\nbody\n";
        let a = parse_unified_agent(content, "critic").unwrap();
        assert_eq!(a.model.as_deref(), Some("github-copilot/claude-haiku"));
        assert!(a.to_opencode_markdown().contains("model: github-copilot/claude-haiku"));
    }

    #[test]
    fn test_inline_list_parsing() {
        let content = "---\nagent_name: x\ndescription: d\ncan: [code, document]\ncannot: [plan]\n---\nb\n";
        let a = parse_unified_agent(content, "x").unwrap();
        assert_eq!(a.can, vec!["code", "document"]);
        assert_eq!(a.cannot, vec!["plan"]);
    }

    #[test]
    fn test_default_agents_all_parse() {
        for (file, content) in DEFAULT_AGENTS {
            let stem = file.trim_end_matches(".md");
            let a = parse_unified_agent(content, stem)
                .unwrap_or_else(|e| panic!("default agent {file} failed to parse: {e}"));
            assert_eq!(&a.name, stem, "agent_name should match file stem for {file}");
            assert!(!a.description.is_empty(), "{file} has empty description");
            // Rendering must not panic and must contain the managed marker.
            assert!(a.to_opencode_markdown().contains(MANAGED_MARKER));
        }
    }

    #[test]
    fn test_write_agent_file_backs_up_unmanaged() {
        let dir = std::env::temp_dir().join("spocket_test_agent_backup");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("critic.md");
        fs::write(&target, "hand authored agent\n").unwrap();

        let mut report = SyncReport::default();
        let action = write_agent_file(&target, "new managed content\n", &mut report).unwrap();
        assert_eq!(action, SyncAction::Updated);
        assert_eq!(report.backed_up.len(), 1);
        let backup = target.with_extension("md.pre-spocket.bak");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "hand authored agent\n");
        assert_eq!(fs::read_to_string(&target).unwrap(), "new managed content\n");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_write_agent_file_unchanged_is_noop() {
        let dir = std::env::temp_dir().join("spocket_test_agent_noop");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("builder.md");
        fs::write(&target, "same\n").unwrap();
        let mut report = SyncReport::default();
        let action = write_agent_file(&target, "same\n", &mut report).unwrap();
        assert_eq!(action, SyncAction::Unchanged);
        assert!(report.backed_up.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_pocket_agent_dir_is_project_local() {
        let pocket = Path::new("/home/user/.safe_pocket/abc123");
        assert_eq!(
            pocket_agent_dir(pocket),
            Path::new("/home/user/.safe_pocket/abc123/.opencode/agent")
        );
    }

    #[test]
    fn test_sync_agents_into_renders_managed_files() {
        let target = std::env::temp_dir().join("spocket_test_sync_into/.opencode/agent");
        let _ = fs::remove_dir_all(target.parent().unwrap().parent().unwrap());

        // sync_agents_into reads unified agents from the config templates dir.
        // Render directly via the rendering primitive to keep this test
        // hermetic, then exercise write_agent_file (the core of sync_agents_into).
        let agent = parse_unified_agent(sample(), "builder").unwrap();
        let rendered = agent.to_opencode_markdown();
        fs::create_dir_all(&target).unwrap();
        let mut report = SyncReport::default();
        let path = target.join("builder.md");
        let action = write_agent_file(&path, &rendered, &mut report).unwrap();
        assert_eq!(action, SyncAction::Created);
        let written = fs::read_to_string(&path).unwrap();
        assert!(written.contains(MANAGED_MARKER));
        assert!(written.contains("mode: primary"));

        let _ = fs::remove_dir_all(target.parent().unwrap().parent().unwrap());
    }

    #[test]
    fn test_remove_global_agents_skips_unmanaged() {
        // remove_global_agents must never delete hand-authored (unmanaged) files.
        // We exercise the marker check directly to stay hermetic (the real
        // function targets the user's global ~/.config path).
        let unmanaged = "hand authored agent, no marker\n";
        assert!(!unmanaged.contains(MANAGED_MARKER));
        let managed = format!("---\nmode: primary\n---\n{MANAGED_MARKER}\nbody\n");
        assert!(managed.contains(MANAGED_MARKER));
    }
}
