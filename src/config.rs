use anyhow::{Context, Result};
use colored::Colorize;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::registry;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    pub aliases: HashMap<String, String>,
}

impl Config {
    pub fn registry_dir() -> Result<PathBuf> {
        registry::registry_dir()
    }

    pub fn config_path() -> Result<PathBuf> {
        registry::aliases_path()
    }

    fn legacy_config_paths() -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(registry_dir) = Self::registry_dir() {
            paths.push(registry_dir.join("registry").join("aliases.json"));
        }

        if let Some(config_dir) = dirs::config_dir() {
            paths.push(config_dir.join("safe_pocket").join("aliases.json"));
            paths.push(config_dir.join("spocket").join("config.json"));
        }

        paths
    }

    pub fn load() -> Result<Self> {
        let new_path = Self::config_path()?;

        if new_path.exists() {
            let content = fs::read_to_string(&new_path).context("Failed to read config file")?;
            return Ok(serde_json::from_str(&content).context("Failed to parse config file")?);
        }

        for old_path in Self::legacy_config_paths() {
            if old_path == new_path || !old_path.exists() {
                continue;
            }

            let content =
                fs::read_to_string(&old_path).context("Failed to read legacy config file")?;
            let config: Config =
                serde_json::from_str(&content).context("Failed to parse legacy config file")?;
            config.save()?;
            let _ = fs::remove_file(&old_path);
            println!(
                "{} {}",
                "Migrated alias registry to:".bright_green(),
                new_path.display().to_string().dimmed()
            );
            return Ok(config);
        }

        Ok(Config::default())
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = serde_json::to_string_pretty(self).context("Failed to serialize config")?;

        fs::write(&path, content).context("Failed to write config file")?;

        Ok(())
    }

    pub fn register_alias(&mut self, name: String, path: String) -> Result<()> {
        self.aliases.insert(name, path);
        self.save()
    }

    pub fn unregister_alias(&mut self, name: &str) -> Result<bool> {
        let removed = self.aliases.remove(name).is_some();
        self.save()?;
        Ok(removed)
    }

    pub fn resolve_path(&self, path: &str) -> Result<PathBuf> {
        // Check if it's an alias
        if let Some(aliased_path) = self.aliases.get(path) {
            return Ok(PathBuf::from(aliased_path));
        }

        // Otherwise, expand and canonicalize the path
        let expanded = shellexpand::full(path).context("Failed to expand path")?;

        let path_buf = PathBuf::from(expanded.as_ref());

        // Canonicalize to get absolute path
        if path_buf.exists() {
            path_buf
                .canonicalize()
                .context("Failed to canonicalize path")
        } else {
            Ok(path_buf)
        }
    }
}
