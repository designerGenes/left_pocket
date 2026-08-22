//! Lightweight, SQLite-backed task tracking — left_pocket's built-in replacement
//! for Beads.
//!
//! Tasks are stored in a single global database at
//! `$HOME/.left_pocket/global_data/tasks.db` (with fallback to legacy roots). Each task
//! is associated with a left_pocket via a *prefix* derived from the left_pocket directory's
//! name (the same short id used everywhere else). Task ids look like
//! `27472722730d-AB12CD`, so they sort and group naturally by project and never
//! collide across pockets.
//!
//! The CLI surface (`left_pocket task …`) is intentionally small and Jira-like:
//!
//! ```text
//! left_pocket task list [--priority N] [--project PATH] [--raw]
//! left_pocket task create --named "…" --description "…" --priority N [--project PATH]
//! left_pocket task <ID> assign --agent "Builder"
//! left_pocket task <ID> start  [--notes "…"]
//! left_pocket task <ID> log     --notes "…"
//! left_pocket task <ID> close  [--notes "…"]
//! left_pocket task <ID> discard
//! left_pocket task <ID> describe [--raw]
//! left_pocket task reprefix --from OLD --to NEW
//! ```

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use colored::Colorize;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// ── Status constants ──────────────────────────────────────────────────────────

pub const STATUS_OPEN: &str = "open";
pub const STATUS_IN_PROGRESS: &str = "in_progress";
pub const STATUS_CLOSED: &str = "closed";
pub const STATUS_DISCARDED: &str = "discarded";

/// Statuses considered "open" / still actionable for `task list`.
const ACTIVE_STATUSES: &[&str] = &[STATUS_OPEN, STATUS_IN_PROGRESS];

const DEFAULT_PRIORITY: i64 = 2;

// ── Database location ─────────────────────────────────────────────────────────

/// `$HOME/.left_pocket/global_data` — created on demand, with fallback to legacy roots.
pub fn global_data_dir() -> Result<PathBuf> {
    let dir = crate::branding::resolve_registry_relative_path(Path::new("global_data"))?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create global data dir: {}", dir.display()))?;
    Ok(dir)
}

/// Path to the global tasks database.
pub fn database_path() -> Result<PathBuf> {
    Ok(global_data_dir()?.join("tasks.db"))
}

/// Open (and initialise) the global tasks database.
pub fn open_db() -> Result<Connection> {
    let path = database_path()?;
    let conn = Connection::open(&path)
        .with_context(|| format!("Failed to open tasks database: {}", path.display()))?;
    init_schema(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS tasks (
            id          TEXT PRIMARY KEY,
            prefix      TEXT NOT NULL,
            name        TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            priority    INTEGER NOT NULL DEFAULT 2,
            status      TEXT NOT NULL DEFAULT 'open',
            assignee    TEXT,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            started_at  TEXT,
            closed_at   TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_tasks_prefix ON tasks(prefix);
        CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks(status);

        CREATE TABLE IF NOT EXISTS task_logs (
            log_id     INTEGER PRIMARY KEY AUTOINCREMENT,
            task_id    TEXT NOT NULL,
            kind       TEXT NOT NULL,
            notes      TEXT,
            created_at TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_task_logs_task ON task_logs(task_id);
        ",
    )
    .context("Failed to initialise tasks database schema")?;
    Ok(())
}

// ── Task model ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct Task {
    pub id: String,
    pub prefix: String,
    pub name: String,
    pub description: String,
    pub priority: i64,
    pub status: String,
    pub assignee: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub closed_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskLog {
    pub kind: String,
    pub notes: Option<String>,
    pub created_at: String,
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get("id")?,
        prefix: row.get("prefix")?,
        name: row.get("name")?,
        description: row.get("description")?,
        priority: row.get("priority")?,
        status: row.get("status")?,
        assignee: row.get("assignee")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
        started_at: row.get("started_at")?,
        closed_at: row.get("closed_at")?,
    })
}

// ── Prefix derivation ─────────────────────────────────────────────────────────

/// Derive the task prefix (left_pocket directory name) for a starting path.
///
/// Resolution order:
/// 1. If `start` lives inside a known left_pocket root such as `~/.left_pocket/<hash>/…`
///    (or `.../temporary/<hash>/…`), the `<hash>` is the prefix.
/// 2. Otherwise ask the workspace registry which left_pocket owns `start`, covering
///    both project directories and left_pocket directories.
/// 3. Finally walk up from `start` looking for a `.env` file containing
///    `LEFT_POCKET_ROOT=<path>` or legacy `SPOCKET_ROOT=<path>`; the basename of that
///    path is the prefix.
pub fn detect_prefix(start: &Path) -> Result<String> {
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());

    // 1. Inside the registry root.
    if let Ok(roots) = crate::branding::known_registry_roots() {
        for root in roots {
            let root = root.canonicalize().unwrap_or(root);
            if let Ok(rel) = start.strip_prefix(&root) {
                let mut comps = rel.components();
                if let Some(first) = comps.next() {
                    let name = first.as_os_str().to_string_lossy().to_string();
                    if name == "temporary" {
                        if let Some(second) = comps.next() {
                            let hash = second.as_os_str().to_string_lossy().to_string();
                            if !hash.is_empty() {
                                return Ok(hash);
                            }
                        }
                    } else if !is_reserved_dir(&name) && !name.is_empty() {
                        return Ok(name);
                    }
                }
            }
        }
    }

    // 2. Bridge through the registry/cache. This lets `left_pocket task` work the
    // same from either the project folder or its left_pocket folder.
    if let Ok(Some(workspace)) = crate::workspace::Workspace::find_workspace_for_cwd(&start) {
        if !workspace.hash.is_empty() {
            return Ok(workspace.hash);
        }
    }

    // 3. Walk up looking for a project .env with LEFT_POCKET_ROOT or SPOCKET_ROOT.
    let mut dir: Option<&Path> = Some(start.as_path());
    while let Some(d) = dir {
        let env = d.join(".env");
        if env.is_file() {
            if let Ok(content) = fs::read_to_string(&env) {
                if let Some(root) =
                    parse_preferred_env_value(&content, &crate::branding::root_env_keys())
                {
                    if let Some(name) = Path::new(&root).file_name() {
                        let name = name.to_string_lossy().to_string();
                        if !name.is_empty() {
                            return Ok(name);
                        }
                    }
                }
            }
        }
        dir = d.parent();
    }

    bail!(
        "Could not determine which left_pocket this directory belongs to.\n\
         Run `left_pocket task …` from inside a registered project (one whose .env\n\
         contains LEFT_POCKET_ROOT or SPOCKET_ROOT) or pass --project <path>."
    )
}

fn is_reserved_dir(name: &str) -> bool {
    matches!(
        name,
        "observations" | "registry" | "unhoused" | "snapshots" | "global_data" | ".git"
    )
}

fn parse_env_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(val) = rest.strip_prefix('=') {
                let val = val.trim().trim_matches('"').trim_matches('\'').trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

fn parse_preferred_env_value(content: &str, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = parse_env_value(content, key) {
            return Some(value);
        }
    }
    None
}

// ── ID generation ─────────────────────────────────────────────────────────────

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

const ID_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
const ID_SUFFIX_LEN: usize = 6;

fn random_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut state = nanos
        ^ (std::process::id() as u64).rotate_left(32)
        ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ 0xD1B5_4A32_D192_ED03;
    if state == 0 {
        state = 0x1234_5678_9ABC_DEF0;
    }
    let mut out = String::with_capacity(ID_SUFFIX_LEN);
    for _ in 0..ID_SUFFIX_LEN {
        // xorshift64
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let idx = (state % ID_ALPHABET.len() as u64) as usize;
        out.push(ID_ALPHABET[idx] as char);
    }
    out
}

fn generate_unique_id(conn: &Connection, prefix: &str) -> Result<String> {
    for _ in 0..32 {
        let candidate = format!("{prefix}-{}", random_suffix());
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM tasks WHERE id = ?1",
                params![candidate],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !exists {
            return Ok(candidate);
        }
    }
    bail!("Failed to generate a unique task id after many attempts")
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

// ── Core operations ───────────────────────────────────────────────────────────

fn add_log(conn: &Connection, task_id: &str, kind: &str, notes: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT INTO task_logs (task_id, kind, notes, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, kind, notes, now_rfc3339()],
    )?;
    Ok(())
}

pub fn create_task(
    conn: &Connection,
    prefix: &str,
    name: &str,
    description: &str,
    priority: i64,
) -> Result<Task> {
    let id = generate_unique_id(conn, prefix)?;
    let now = now_rfc3339();
    conn.execute(
        "INSERT INTO tasks (id, prefix, name, description, priority, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
        params![id, prefix, name, description, priority, STATUS_OPEN, now],
    )?;
    add_log(conn, &id, "create", None)?;
    get_task(conn, &id)?.ok_or_else(|| anyhow!("Task vanished immediately after creation"))
}

pub fn get_task(conn: &Connection, id: &str) -> Result<Option<Task>> {
    let task = conn
        .query_row(
            "SELECT * FROM tasks WHERE id = ?1",
            params![id],
            row_to_task,
        )
        .optional()?;
    Ok(task)
}

/// Resolve a user-supplied id reference to a full task id.
///
/// Accepts a full id (`prefix-ABC123`) or a bare suffix (`ABC123`), in which
/// case the prefix derived from `cwd` is prepended. Matching is case-insensitive
/// on the suffix.
pub fn resolve_task(conn: &Connection, reference: &str, cwd: &Path) -> Result<Task> {
    // Exact match first.
    if let Some(task) = get_task(conn, reference)? {
        return Ok(task);
    }
    // Case-insensitive exact match.
    if let Some(task) = conn
        .query_row(
            "SELECT * FROM tasks WHERE id = ?1 COLLATE NOCASE",
            params![reference],
            row_to_task,
        )
        .optional()?
    {
        return Ok(task);
    }
    // Bare suffix: prepend the detected prefix.
    if !reference.contains('-') {
        if let Ok(prefix) = detect_prefix(cwd) {
            let full = format!("{prefix}-{reference}");
            if let Some(task) = conn
                .query_row(
                    "SELECT * FROM tasks WHERE id = ?1 COLLATE NOCASE",
                    params![full],
                    row_to_task,
                )
                .optional()?
            {
                return Ok(task);
            }
        }
    }
    bail!("No task found matching id '{reference}'")
}

pub fn list_tasks(conn: &Connection, prefix: &str, max_priority: Option<i64>) -> Result<Vec<Task>> {
    let active = ACTIVE_STATUSES
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!("SELECT * FROM tasks WHERE prefix = ?1 AND status IN ({active})");
    if max_priority.is_some() {
        sql.push_str(" AND priority <= ?2");
    }
    sql.push_str(" ORDER BY priority ASC, created_at ASC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = if let Some(p) = max_priority {
        stmt.query_map(params![prefix, p], row_to_task)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map(params![prefix], row_to_task)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows)
}

pub fn task_logs(conn: &Connection, id: &str) -> Result<Vec<TaskLog>> {
    let mut stmt = conn.prepare(
        "SELECT kind, notes, created_at FROM task_logs WHERE task_id = ?1 ORDER BY log_id ASC",
    )?;
    let logs = stmt
        .query_map(params![id], |row| {
            Ok(TaskLog {
                kind: row.get(0)?,
                notes: row.get(1)?,
                created_at: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(logs)
}

fn touch(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET updated_at = ?2 WHERE id = ?1",
        params![id, now_rfc3339()],
    )?;
    Ok(())
}

pub fn assign_task(conn: &Connection, id: &str, agent: &str) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET assignee = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, agent, now_rfc3339()],
    )?;
    add_log(conn, id, "assign", Some(agent))?;
    Ok(())
}

pub fn start_task(conn: &Connection, id: &str, notes: Option<&str>) -> Result<()> {
    let now = now_rfc3339();
    conn.execute(
        "UPDATE tasks
         SET status = ?2,
             started_at = COALESCE(started_at, ?3),
             updated_at = ?3
         WHERE id = ?1",
        params![id, STATUS_IN_PROGRESS, now],
    )?;
    add_log(conn, id, "start", notes)?;
    Ok(())
}

pub fn close_task(conn: &Connection, id: &str, notes: Option<&str>) -> Result<()> {
    let now = now_rfc3339();
    conn.execute(
        "UPDATE tasks SET status = ?2, closed_at = ?3, updated_at = ?3 WHERE id = ?1",
        params![id, STATUS_CLOSED, now],
    )?;
    add_log(conn, id, "close", notes)?;
    Ok(())
}

pub fn discard_task(conn: &Connection, id: &str, notes: Option<&str>) -> Result<()> {
    conn.execute(
        "UPDATE tasks SET status = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, STATUS_DISCARDED, now_rfc3339()],
    )?;
    add_log(conn, id, "discard", notes)?;
    let _ = touch(conn, id);
    Ok(())
}

pub fn log_task(conn: &Connection, id: &str, notes: &str) -> Result<()> {
    add_log(conn, id, "log", Some(notes))?;
    touch(conn, id)?;
    Ok(())
}

/// Re-point every task from `old` prefix to `new`, rewriting both the `prefix`
/// column and the leading `<old>-` of each task id. Returns the number of tasks
/// updated. Used when a left_pocket's directory name changes.
pub fn reprefix(conn: &Connection, old: &str, new: &str) -> Result<usize> {
    if old == new {
        return Ok(0);
    }
    let tx = conn.unchecked_transaction()?;
    let updated = {
        // Update ids: prefix-XYZ -> newprefix-XYZ
        let id_changes: usize = tx.execute(
            "UPDATE tasks SET id = ?2 || substr(id, length(?1) + 1) WHERE prefix = ?1",
            params![old, new],
        )?;
        // Update prefix column.
        tx.execute(
            "UPDATE tasks SET prefix = ?2 WHERE prefix = ?1",
            params![old, new],
        )?;
        // Update log references.
        tx.execute(
            "UPDATE task_logs SET task_id = ?2 || substr(task_id, length(?1) + 1)
             WHERE task_id LIKE ?1 || '-%'",
            params![old, new],
        )?;
        id_changes
    };
    tx.commit()?;
    Ok(updated)
}

/// Reprefix tasks in the global database when a left_pocket's directory name
/// changes (e.g. after `heal` renames the left_pocket). This is a no-op when the
/// names match or when no database exists yet, so it is safe to call
/// unconditionally from left_pocket-mutating commands. Returns the number of tasks
/// migrated.
pub fn reprefix_global(old: &str, new: &str) -> Result<usize> {
    if old == new || old.is_empty() || new.is_empty() {
        return Ok(0);
    }
    // Don't create the database just to migrate a left_pocket that has no tasks.
    let path = database_path()?;
    if !path.exists() {
        return Ok(0);
    }
    let conn = open_db()?;
    reprefix(&conn, old, new)
}

// ── CLI entry point ───────────────────────────────────────────────────────────

/// Parse and dispatch the raw trailing args after `left_pocket task`.
pub fn run_cli(args: Vec<String>) -> Result<()> {
    let mut iter = args.into_iter();
    let first = iter.next().ok_or_else(|| {
        anyhow!(
            "Missing task subcommand.\n\
             Usage: left_pocket task <list|create|ID|reprefix> …\n\
             Try `left_pocket task list` or `left_pocket task create --named \"…\"`."
        )
    })?;
    let rest: Vec<String> = iter.collect();

    match first.as_str() {
        "list" | "ls" => cmd_list(&rest),
        "create" | "new" | "add" => cmd_create(&rest),
        "reprefix" => cmd_reprefix(&rest),
        other => {
            // `task <ID> <action> …`
            let mut rest_iter = rest.into_iter();
            let action = rest_iter.next().ok_or_else(|| {
                anyhow!(
                    "Missing action for task '{other}'.\n\
                     Usage: left_pocket task <ID> <assign|start|log|close|discard|describe> …"
                )
            })?;
            let action_args: Vec<String> = rest_iter.collect();
            cmd_task_action(other, &action, &action_args)
        }
    }
}

fn cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn prefix_for(flags: &Flags) -> Result<String> {
    if let Some(project) = flags.value("project") {
        let path = shellexpand::tilde(project).into_owned();
        detect_prefix(Path::new(&path))
    } else {
        detect_prefix(&cwd())
    }
}

fn cmd_list(args: &[String]) -> Result<()> {
    let flags = Flags::parse(args, &["priority", "project"], &["raw", "json"])?;
    let prefix = prefix_for(&flags)?;
    let max_priority = flags.int("priority")?;

    let conn = open_db()?;
    let tasks = list_tasks(&conn, &prefix, max_priority)?;

    if flags.has("raw") || flags.has("json") {
        println!("{}", serde_json::to_string_pretty(&tasks)?);
        return Ok(());
    }

    if tasks.is_empty() {
        println!(
            "{} {}",
            "No open tasks for".dimmed(),
            prefix.bright_yellow()
        );
        return Ok(());
    }

    println!(
        "{} {}",
        "Open tasks for".bright_white().bold(),
        prefix.bright_yellow()
    );
    println!();
    for t in &tasks {
        let pri = format!("P{}", t.priority);
        let assignee = t
            .assignee
            .as_deref()
            .map(|a| format!(" @{a}"))
            .unwrap_or_default();
        println!(
            "  {}  {}  {}{}  {}",
            t.id.bright_cyan(),
            color_priority(t.priority, &pri),
            color_status(&t.status),
            assignee.bright_magenta(),
            t.name
        );
    }
    Ok(())
}

fn cmd_create(args: &[String]) -> Result<()> {
    let flags = Flags::parse(
        args,
        &[
            "named",
            "name",
            "description",
            "desc",
            "priority",
            "project",
        ],
        &["raw", "json"],
    )?;
    let name = flags
        .value("named")
        .or_else(|| flags.value("name"))
        .ok_or_else(|| anyhow!("`task create` requires --named \"<title>\""))?
        .to_string();
    let description = flags
        .value("description")
        .or_else(|| flags.value("desc"))
        .unwrap_or("")
        .to_string();
    let priority = flags.int("priority")?.unwrap_or(DEFAULT_PRIORITY);
    let prefix = prefix_for(&flags)?;

    let conn = open_db()?;
    let task = create_task(&conn, &prefix, &name, &description, priority)?;

    if flags.has("raw") || flags.has("json") {
        println!("{}", serde_json::to_string_pretty(&task)?);
        return Ok(());
    }

    println!(
        "{} {}  {}  {}",
        "Created task".bright_green(),
        task.id.bright_cyan().bold(),
        color_priority(task.priority, &format!("P{}", task.priority)),
        task.name
    );
    Ok(())
}

fn cmd_reprefix(args: &[String]) -> Result<()> {
    let flags = Flags::parse(args, &["from", "to"], &[])?;
    let from = flags
        .value("from")
        .ok_or_else(|| anyhow!("`task reprefix` requires --from <old-prefix>"))?;
    let to = flags
        .value("to")
        .ok_or_else(|| anyhow!("`task reprefix` requires --to <new-prefix>"))?;

    let conn = open_db()?;
    let n = reprefix(&conn, from, to)?;
    println!(
        "{} {} task(s) from {} → {}",
        "Reprefixed".bright_green(),
        n.to_string().bright_yellow(),
        from.dimmed(),
        to.bright_yellow()
    );
    Ok(())
}

fn cmd_task_action(id_ref: &str, action: &str, args: &[String]) -> Result<()> {
    let conn = open_db()?;

    match action {
        "assign" => {
            let flags = Flags::parse(args, &["agent"], &[])?;
            let agent = flags
                .value("agent")
                .ok_or_else(|| anyhow!("`task <ID> assign` requires --agent \"<name>\""))?;
            let task = resolve_task(&conn, id_ref, &cwd())?;
            assign_task(&conn, &task.id, agent)?;
            println!(
                "{} {} → {}",
                "Assigned".bright_green(),
                task.id.bright_cyan(),
                agent.bright_magenta()
            );
        }
        "start" => {
            let flags = Flags::parse(args, &["notes", "note"], &[])?;
            let notes = flags.value("notes").or_else(|| flags.value("note"));
            let task = resolve_task(&conn, id_ref, &cwd())?;
            start_task(&conn, &task.id, notes)?;
            println!(
                "{} {} ({})",
                "Started".bright_green(),
                task.id.bright_cyan(),
                STATUS_IN_PROGRESS.bright_blue()
            );
        }
        "log" => {
            let flags = Flags::parse(args, &["notes", "note"], &[])?;
            let notes = flags
                .value("notes")
                .or_else(|| flags.value("note"))
                .ok_or_else(|| anyhow!("`task <ID> log` requires --notes \"<message>\""))?;
            let task = resolve_task(&conn, id_ref, &cwd())?;
            log_task(&conn, &task.id, notes)?;
            println!(
                "{} {}",
                "Logged note on".bright_green(),
                task.id.bright_cyan()
            );
        }
        "close" | "done" => {
            let flags = Flags::parse(args, &["notes", "note"], &[])?;
            let notes = flags.value("notes").or_else(|| flags.value("note"));
            let task = resolve_task(&conn, id_ref, &cwd())?;
            close_task(&conn, &task.id, notes)?;
            println!(
                "{} {} ({})",
                "Closed".bright_green(),
                task.id.bright_cyan(),
                STATUS_CLOSED.dimmed()
            );
        }
        "discard" | "delete" | "rm" => {
            let flags = Flags::parse(args, &["notes", "note"], &[])?;
            let notes = flags.value("notes").or_else(|| flags.value("note"));
            let task = resolve_task(&conn, id_ref, &cwd())?;
            discard_task(&conn, &task.id, notes)?;
            println!(
                "{} {} ({})",
                "Discarded".bright_yellow(),
                task.id.bright_cyan(),
                STATUS_DISCARDED.dimmed()
            );
        }
        "describe" | "show" | "info" => {
            let flags = Flags::parse(args, &[], &["raw", "json"])?;
            let task = resolve_task(&conn, id_ref, &cwd())?;
            if flags.has("raw") || flags.has("json") {
                let logs = task_logs(&conn, &task.id)?;
                let out = serde_json::json!({ "task": task, "logs": logs });
                println!("{}", serde_json::to_string_pretty(&out)?);
            } else {
                print_describe(&conn, &task)?;
            }
        }
        other => bail!(
            "Unknown task action '{other}'.\n\
             Valid actions: assign, start, log, close, discard, describe."
        ),
    }
    Ok(())
}

fn print_describe(conn: &Connection, task: &Task) -> Result<()> {
    println!(
        "{} {}",
        "Task".bright_white().bold(),
        task.id.bright_cyan().bold()
    );
    println!("  {:<12} {}", "Name:".dimmed(), task.name);
    if !task.description.is_empty() {
        println!("  {:<12} {}", "Description:".dimmed(), task.description);
    }
    println!(
        "  {:<12} {}",
        "Priority:".dimmed(),
        color_priority(task.priority, &format!("P{}", task.priority))
    );
    println!(
        "  {:<12} {}",
        "Status:".dimmed(),
        color_status(&task.status)
    );
    println!(
        "  {:<12} {}",
        "Assignee:".dimmed(),
        task.assignee.as_deref().unwrap_or("(unassigned)")
    );
    println!(
        "  {:<12} {}",
        "Created:".dimmed(),
        friendly_time(&task.created_at)
    );
    println!(
        "  {:<12} {}",
        "Updated:".dimmed(),
        friendly_time(&task.updated_at)
    );
    if let Some(s) = &task.started_at {
        println!("  {:<12} {}", "Started:".dimmed(), friendly_time(s));
    }
    if let Some(c) = &task.closed_at {
        println!("  {:<12} {}", "Closed:".dimmed(), friendly_time(c));
    }

    let logs = task_logs(conn, &task.id)?;
    if !logs.is_empty() {
        println!();
        println!("  {}", "History:".bright_white());
        for log in logs {
            let when = friendly_time(&log.created_at);
            match log.notes {
                Some(n) if !n.is_empty() => {
                    println!("    {} {} — {}", when.dimmed(), log.kind.bright_blue(), n)
                }
                _ => println!("    {} {}", when.dimmed(), log.kind.bright_blue()),
            }
        }
    }
    Ok(())
}

fn friendly_time(rfc3339: &str) -> String {
    DateTime::parse_from_rfc3339(rfc3339)
        .map(|dt| {
            dt.with_timezone(&Utc)
                .format("%Y-%m-%d %H:%M UTC")
                .to_string()
        })
        .unwrap_or_else(|_| rfc3339.to_string())
}

fn color_priority(priority: i64, label: &str) -> colored::ColoredString {
    match priority {
        0 => label.bright_red().bold(),
        1 => label.bright_yellow(),
        2 => label.bright_white(),
        _ => label.dimmed(),
    }
}

fn color_status(status: &str) -> colored::ColoredString {
    match status {
        STATUS_OPEN => status.bright_green(),
        STATUS_IN_PROGRESS => status.bright_blue(),
        STATUS_CLOSED => status.dimmed(),
        STATUS_DISCARDED => status.bright_red(),
        _ => status.normal(),
    }
}

// ── Tiny flag parser ──────────────────────────────────────────────────────────

/// A minimal `--flag value` / `--flag=value` / `--bool` parser. Tailored to the
/// task CLI so the spec's `task <ID> <action>` ordering can be honoured exactly.
struct Flags {
    values: HashMap<String, String>,
    bools: HashSet<String>,
}

impl Flags {
    fn parse(args: &[String], value_flags: &[&str], bool_flags: &[&str]) -> Result<Flags> {
        let value_set: HashSet<&str> = value_flags.iter().copied().collect();
        let bool_set: HashSet<&str> = bool_flags.iter().copied().collect();
        let mut values = HashMap::new();
        let mut bools = HashSet::new();

        let mut i = 0;
        while i < args.len() {
            let arg = &args[i];
            if let Some(stripped) = arg.strip_prefix("--") {
                if let Some((key, val)) = stripped.split_once('=') {
                    if value_set.contains(key) {
                        values.insert(key.to_string(), val.to_string());
                    } else if bool_set.contains(key) {
                        bools.insert(key.to_string());
                    } else {
                        bail!("Unknown flag --{key}");
                    }
                } else if value_set.contains(stripped) {
                    let val = args
                        .get(i + 1)
                        .ok_or_else(|| anyhow!("Flag --{stripped} expects a value"))?;
                    values.insert(stripped.to_string(), val.clone());
                    i += 1;
                } else if bool_set.contains(stripped) {
                    bools.insert(stripped.to_string());
                } else {
                    bail!("Unknown flag --{stripped}");
                }
            } else {
                bail!("Unexpected argument '{arg}'");
            }
            i += 1;
        }

        Ok(Flags { values, bools })
    }

    fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(|s| s.as_str())
    }

    fn has(&self, key: &str) -> bool {
        self.bools.contains(key)
    }

    fn int(&self, key: &str) -> Result<Option<i64>> {
        match self.values.get(key) {
            Some(v) => v
                .parse::<i64>()
                .map(Some)
                .map_err(|_| anyhow!("Flag --{key} expects an integer, got '{v}'")),
            None => Ok(None),
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_create_and_get() {
        let conn = mem_db();
        let t = create_task(&conn, "proj", "Do the thing", "details", 1).unwrap();
        assert!(t.id.starts_with("proj-"));
        assert_eq!(t.name, "Do the thing");
        assert_eq!(t.description, "details");
        assert_eq!(t.priority, 1);
        assert_eq!(t.status, STATUS_OPEN);
        let fetched = get_task(&conn, &t.id).unwrap().unwrap();
        assert_eq!(fetched.id, t.id);
    }

    #[test]
    fn test_id_format_and_uniqueness() {
        let conn = mem_db();
        let mut ids = HashSet::new();
        for _ in 0..50 {
            let t = create_task(&conn, "abc123", "n", "", 2).unwrap();
            let suffix = t.id.strip_prefix("abc123-").unwrap();
            assert_eq!(suffix.len(), ID_SUFFIX_LEN);
            assert!(suffix
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
            assert!(ids.insert(t.id), "ids must be unique");
        }
    }

    #[test]
    fn test_list_filters_status_and_priority() {
        let conn = mem_db();
        let a = create_task(&conn, "p", "p0", "", 0).unwrap();
        let b = create_task(&conn, "p", "p2", "", 2).unwrap();
        let c = create_task(&conn, "p", "p3", "", 3).unwrap();
        let _other = create_task(&conn, "q", "other", "", 0).unwrap();

        // All active for prefix p (not q).
        let all = list_tasks(&conn, "p", None).unwrap();
        assert_eq!(all.len(), 3);
        // Sorted by priority ascending.
        assert_eq!(all[0].id, a.id);

        // Priority filter <= 2.
        let filtered = list_tasks(&conn, "p", Some(2)).unwrap();
        let ids: HashSet<_> = filtered.iter().map(|t| t.id.clone()).collect();
        assert!(ids.contains(&a.id));
        assert!(ids.contains(&b.id));
        assert!(!ids.contains(&c.id));

        // Closing removes from open list.
        close_task(&conn, &a.id, Some("done")).unwrap();
        let after = list_tasks(&conn, "p", None).unwrap();
        assert_eq!(after.len(), 2);
    }

    #[test]
    fn test_lifecycle_transitions() {
        let conn = mem_db();
        let t = create_task(&conn, "p", "task", "", 2).unwrap();

        assign_task(&conn, &t.id, "Builder").unwrap();
        assert_eq!(
            get_task(&conn, &t.id).unwrap().unwrap().assignee.as_deref(),
            Some("Builder")
        );

        start_task(&conn, &t.id, Some("starting")).unwrap();
        let started = get_task(&conn, &t.id).unwrap().unwrap();
        assert_eq!(started.status, STATUS_IN_PROGRESS);
        assert!(started.started_at.is_some());

        log_task(&conn, &t.id, "progress note").unwrap();

        discard_task(&conn, &t.id, None).unwrap();
        assert_eq!(
            get_task(&conn, &t.id).unwrap().unwrap().status,
            STATUS_DISCARDED
        );

        // History records every action.
        let logs = task_logs(&conn, &t.id).unwrap();
        let kinds: Vec<_> = logs.iter().map(|l| l.kind.as_str()).collect();
        assert_eq!(kinds, vec!["create", "assign", "start", "log", "discard"]);
    }

    #[test]
    fn test_resolve_bare_suffix_via_exact_and_nocase() {
        let conn = mem_db();
        let t = create_task(&conn, "proj", "task", "", 2).unwrap();
        // Exact.
        let r1 = resolve_task(&conn, &t.id, Path::new("/")).unwrap();
        assert_eq!(r1.id, t.id);
        // Case-insensitive exact.
        let r2 = resolve_task(&conn, &t.id.to_lowercase(), Path::new("/")).unwrap();
        assert_eq!(r2.id, t.id);
        // Missing -> error.
        assert!(resolve_task(&conn, "proj-ZZZZZZ", Path::new("/")).is_err());
    }

    #[test]
    fn test_reprefix_updates_ids_and_logs() {
        let conn = mem_db();
        let t = create_task(&conn, "oldname", "task", "", 1).unwrap();
        let suffix = t.id.strip_prefix("oldname-").unwrap().to_string();
        log_task(&conn, &t.id, "a note").unwrap();

        let n = reprefix(&conn, "oldname", "newname").unwrap();
        assert_eq!(n, 1);

        let new_id = format!("newname-{suffix}");
        let moved = get_task(&conn, &new_id).unwrap().unwrap();
        assert_eq!(moved.prefix, "newname");
        assert!(get_task(&conn, &t.id).unwrap().is_none());

        let logs = task_logs(&conn, &new_id).unwrap();
        assert_eq!(logs.len(), 2); // create + log
    }

    #[test]
    fn test_detect_prefix_from_env() {
        let base = std::env::temp_dir().join("spocket_task_prefix_test");
        let _ = fs::remove_dir_all(&base);
        let project = base.join("my_cool_project");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join(".env"),
            "LEFT_POCKET_ROOT=/Users/x/.left_pocket/deadbeef00\nSPOCKET_ROOT=/Users/x/.safe_pocket/oldvalue00\n",
        )
        .unwrap();

        let prefix = detect_prefix(&project).unwrap();
        assert_eq!(prefix, "deadbeef00");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_parse_env_value() {
        let c = "FOO=bar\nSPOCKET_ROOT=\"/a/b/hashhash\"\n";
        assert_eq!(
            parse_env_value(c, "SPOCKET_ROOT").as_deref(),
            Some("/a/b/hashhash")
        );
        assert_eq!(parse_env_value(c, "MISSING"), None);
    }

    /// Backwards compatibility: left_pocket no longer *writes* `SPOCKET_ROOT`, but a
    /// pre-existing project `.env` that only carries the legacy key must still
    /// resolve to the right left_pocket.
    #[test]
    fn test_detect_prefix_from_legacy_only_env() {
        let base = std::env::temp_dir().join("pocket_task_prefix_legacy_only");
        let _ = fs::remove_dir_all(&base);
        let project = base.join("legacy_project");
        fs::create_dir_all(&project).unwrap();
        fs::write(
            project.join(".env"),
            "SPOCKET_ROOT=/Users/x/.safe_pocket/legacyhash1\n",
        )
        .unwrap();

        let prefix = detect_prefix(&project).unwrap();
        assert_eq!(
            prefix, "legacyhash1",
            "a legacy-only .env must still resolve for backwards compatibility"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn test_parse_preferred_env_value_prefers_pocket_root() {
        let c = "SPOCKET_ROOT=/old/hash\nLEFT_POCKET_ROOT=/new/hash\n";
        assert_eq!(
            parse_preferred_env_value(c, &crate::branding::root_env_keys()).as_deref(),
            Some("/new/hash")
        );
    }

    #[test]
    fn test_flags_parser() {
        let args: Vec<String> = vec![
            "--named".into(),
            "Title here".into(),
            "--priority=1".into(),
            "--raw".into(),
        ];
        let f = Flags::parse(&args, &["named", "priority"], &["raw"]).unwrap();
        assert_eq!(f.value("named"), Some("Title here"));
        assert_eq!(f.int("priority").unwrap(), Some(1));
        assert!(f.has("raw"));
        assert!(!f.has("json"));

        // Unknown flag errors.
        assert!(Flags::parse(&["--bogus".into()], &[], &[]).is_err());
    }
}
