use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;

use crate::registry;

pub fn append_pocket_event(pocket_dir: &Path, action: &str, details: Value) -> Result<()> {
    append_event(&pocket_dir.join("events.jsonl"), action, details)
}

pub fn append_registry_event(action: &str, details: Value) -> Result<()> {
    append_event(
        &registry::registry_dir()?.join("events.jsonl"),
        action,
        details,
    )
}

fn append_event(path: &Path, action: &str, details: Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create event log directory: {}", parent.display())
        })?;
    }

    let event = serde_json::json!({
        "timestamp": Utc::now().to_rfc3339(),
        "action": action,
        "details": details,
    });
    let line = serde_json::to_string(&event).context("Failed to serialize event")?;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open event log: {}", path.display()))?;
    writeln!(file, "{line}").context("Failed to write event log")?;

    Ok(())
}
