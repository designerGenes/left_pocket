mod cli;
mod config;
mod hash;
mod manifest;
mod registry;
mod template;
mod workspace;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Duration, Utc};
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use colored::Colorize;
use std::fs;
use std::io::{self, Write as IoWrite};
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use cli::{CleanScope, Cli, Commands, MarkChoice, WorktreeAction};
use config::Config;
use manifest::Manifest;
use registry::RegistryEntry;
use workspace::{DriftResult, Workspace};

static VERBOSE: OnceLock<bool> = OnceLock::new();

pub fn verbose() -> bool {
    *VERBOSE.get().unwrap_or(&false)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{} {}", "Error:".bright_red(), e);
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let _ = VERBOSE.set(cli.verbose);

    if cli.short_version {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Handle upgrade (-u) before default setup so it only moves template
    // content downstream and never writes raw template files.
    if let Some(upgrade_path) = cli.upgrade {
        return handle_upgrade(upgrade_path);
    }

    // Ensure default template assets exist for normal commands. This is a no-op
    // after first run and is intentionally skipped for upgrade.
    let _ = template::ensure_default_assets();

    // Handle subcommands first
    if let Some(command) = cli.command {
        return handle_command(command);
    }

    // If no subcommand, we're creating/opening a workspace
    if cli.include.is_empty() {
        return Err(anyhow!(
            "No directories specified. Use -i/--include to add directories."
        ));
    }

    handle_workspace(cli)
}

fn handle_command(command: Commands) -> Result<()> {
    match command {
        Commands::Register { alias } => {
            let parts: Vec<&str> = alias.splitn(2, '=').collect();

            if parts.len() != 2 {
                return Err(anyhow!("Invalid alias format. Use: name=\"path\""));
            }

            let name = parts[0].trim().to_string();
            let path = parts[1].trim().trim_matches('"').to_string();

            let mut config = Config::load()?;
            config.register_alias(name.clone(), path.clone())?;

            println!(
                "{} {} -> {}",
                "Registered alias:".bright_green(),
                name.bright_yellow(),
                path.dimmed()
            );

            Ok(())
        }

        Commands::Unregister { name } => {
            let mut config = Config::load()?;

            if config.unregister_alias(&name)? {
                println!(
                    "{} {}",
                    "Unregistered alias:".bright_green(),
                    name.bright_yellow()
                );
            } else {
                println!("{} {}", "Alias not found:".dimmed(), name.bright_yellow());
            }

            Ok(())
        }

        Commands::List => {
            let config = Config::load()?;

            if config.aliases.is_empty() {
                println!("{}", "No aliases registered.".dimmed());
                return Ok(());
            }

            println!("{}", "Registered aliases:".bright_white().bold());
            println!();

            let mut aliases: Vec<_> = config.aliases.iter().collect();
            aliases.sort_by_key(|(name, _)| *name);

            for (name, path) in aliases {
                println!("  {} -> {}", name.bright_yellow(), path.bright_blue());
            }

            Ok(())
        }

        Commands::ListWorkspaces => {
            let workspaces = Workspace::list_all()?;

            if workspaces.is_empty() {
                println!("{}", "No workspaces found.".dimmed());
                return Ok(());
            }

            println!("{}", "Workspaces:".bright_white().bold());
            println!();

            for workspace in workspaces {
                println!(
                    "  {} {}",
                    workspace.hash.bright_yellow(),
                    format!("({})", workspace.pocket_dir.display()).dimmed()
                );

                for path in &workspace.core_paths {
                    println!("    - {}", path.display().to_string().bright_blue());
                }

                println!();
            }

            Ok(())
        }

        Commands::Sync { pocket } => handle_sync(pocket),

        Commands::MergeStart { pocket } => handle_merge_start(pocket),

        Commands::MergeStop { pocket } => handle_merge_stop(pocket),

        Commands::Augment {
            add,
            remove,
            no_open,
        } => handle_augment(add, remove, no_open),

        Commands::Mark { mark, pocket } => handle_mark(mark, pocket),

        Commands::Clean {
            scope,
            older_than,
            all,
            hard,
            yes,
        } => handle_clean(scope, older_than, all, hard, yes),

        Commands::Heal { project, pocket } => handle_heal(project, pocket),

        Commands::Backup { repo, schedule } => handle_backup(repo, schedule),

        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let shell: clap_complete::Shell = shell.into();
            generate(shell, &mut cmd, "safe_pocket", &mut io::stdout());
            Ok(())
        }

        Commands::Worktree { action } => handle_worktree(action),
    }
}

fn handle_worktree(action: WorktreeAction) -> Result<()> {
    let cwd = std::env::current_dir().context("Failed to get current working directory")?;

    let workspace = Workspace::find_workspace_for_cwd(&cwd)?.ok_or_else(|| {
        anyhow!(
            "No workspace found for current directory: {}\nRun this from inside a project directory that belongs to a safe pocket.",
            cwd.display()
        )
    })?;

    match action {
        WorktreeAction::Add { path } => {
            let config = Config::load()?;

            let target_path = match path {
                Some(p) => config.resolve_path(&p)?,
                None => prompt_worktree_from_git(&workspace.core_paths)?.ok_or_else(|| {
                    anyhow!("No path provided and no git worktrees detected. Use: safe_pocket worktree add <path>")
                })?,
            };

            if !target_path.exists() {
                return Err(anyhow!("Path does not exist: {}", target_path.display()));
            }

            let mut manifest = Manifest::load(&workspace.pocket_dir)?.unwrap_or_else(|| {
                Manifest::new_with_options(
                    workspace.hash.clone(),
                    workspace.core_paths.clone(),
                    workspace.temporary,
                )
            });

            if manifest.add_worktree(target_path.clone()) {
                manifest.save(&workspace.pocket_dir)?;
                println!(
                    "{} {} -> {}",
                    "Worktree registered:".bright_green(),
                    target_path.display().to_string().bright_blue(),
                    workspace.hash.bright_yellow()
                );
            } else {
                println!(
                    "{} {}",
                    "Already registered:".dimmed(),
                    target_path.display().to_string().bright_blue()
                );
            }

            Ok(())
        }

        WorktreeAction::Remove { path } => {
            let config = Config::load()?;
            let target_path = config.resolve_path(&path)?;

            let mut manifest = match Manifest::load(&workspace.pocket_dir)? {
                Some(m) => m,
                None => {
                    return Err(anyhow!(
                        "No manifest found for workspace {}",
                        workspace.hash
                    ))
                }
            };

            if manifest.remove_worktree(&target_path) {
                manifest.save(&workspace.pocket_dir)?;
                println!(
                    "{} {}",
                    "Worktree removed:".bright_green(),
                    target_path.display().to_string().bright_blue()
                );
            } else {
                println!(
                    "{} {} (not registered)",
                    "Not found:".dimmed(),
                    target_path.display().to_string().bright_blue()
                );
            }

            Ok(())
        }

        WorktreeAction::List => {
            let manifest = match Manifest::load(&workspace.pocket_dir)? {
                Some(m) => m,
                None => {
                    return Err(anyhow!(
                        "No manifest found for workspace {}",
                        workspace.hash
                    ))
                }
            };

            println!(
                "{} {}",
                "Pocket:".bright_white().bold(),
                workspace.hash.bright_yellow()
            );
            println!(
                "  {} {}",
                "Location:".dimmed(),
                workspace.pocket_dir.display().to_string().bright_blue()
            );
            println!();

            println!("{}", "Core directories:".bright_white());
            for path in &manifest.core_paths {
                println!("  {}", path.display().to_string().bright_blue());
            }

            if manifest.worktrees.is_empty() {
                println!();
                println!("{}", "No worktrees registered.".dimmed());
            } else {
                println!();
                println!("{}", "Registered worktrees:".bright_white());
                for path in &manifest.worktrees {
                    let exists_marker = if path.exists() { "" } else { " (missing)" };
                    println!(
                        "  {}{}",
                        path.display().to_string().bright_blue(),
                        exists_marker.bright_red()
                    );
                }
            }

            Ok(())
        }
    }
}

fn prompt_worktree_from_git(core_paths: &[PathBuf]) -> Result<Option<PathBuf>> {
    let project_root = match core_paths.first() {
        Some(p) => p,
        None => return Ok(None),
    };

    let output = std::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(project_root)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Ok(None),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let git_worktrees: Vec<PathBuf> = stdout
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from)
        .filter(|p| !core_paths.contains(p))
        .collect();

    if git_worktrees.is_empty() {
        return Ok(None);
    }

    println!("\n{}", "Git worktrees detected:".bright_white());
    for (i, wt) in git_worktrees.iter().enumerate() {
        println!(
            "  {}. {}",
            (i + 1).to_string().bright_yellow(),
            wt.display().to_string().bright_blue()
        );
    }
    println!(
        "  {}. {}",
        "0".bright_yellow(),
        "Enter a path manually".dimmed()
    );
    println!();

    print!("{} ", "Select a worktree to register (0-N):".bright_white());
    use std::io::{self, Write as IoWrite};
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input == "0" || input.is_empty() {
        return Ok(None);
    }

    if let Ok(n) = input.parse::<usize>() {
        if n > 0 && n <= git_worktrees.len() {
            return Ok(Some(git_worktrees[n - 1].clone()));
        }
    }

    println!("{}", "Invalid selection.".bright_yellow());
    Ok(None)
}

fn find_existing_workspace_for_paths(paths: &[PathBuf]) -> Result<Option<Workspace>> {
    let spocket_dir = Workspace::spocket_dir()?;
    let temporary_spocket_dir = Workspace::temporary_spocket_dir()?;

    for path in paths {
        if path.starts_with(&spocket_dir) {
            let relative = path.strip_prefix(&spocket_dir).unwrap();
            if let Some(hash_component) = relative.components().next() {
                let hash = hash_component.as_os_str().to_string_lossy().to_string();
                let pocket_dir = spocket_dir.join(&hash);
                if let Some((_, core_paths)) = Workspace::load_manifest_or_backfill(&pocket_dir)? {
                    return Ok(Some(Workspace {
                        hash,
                        core_paths,
                        sidecar_paths: vec![],
                        pocket_dir,
                        create_readmes: false,
                        temporary: false,
                    }));
                }
            }
        }

        if path.starts_with(&temporary_spocket_dir) {
            let relative = path.strip_prefix(&temporary_spocket_dir).unwrap();
            if let Some(hash_component) = relative.components().next() {
                let hash = hash_component.as_os_str().to_string_lossy().to_string();
                let pocket_dir = temporary_spocket_dir.join(&hash);
                if let Some((_, core_paths)) = Workspace::load_manifest_or_backfill(&pocket_dir)? {
                    return Ok(Some(Workspace {
                        hash,
                        core_paths,
                        sidecar_paths: vec![],
                        pocket_dir,
                        create_readmes: false,
                        temporary: true,
                    }));
                }
            }
        }

        if let Some(ws) = Workspace::find_workspace_containing(path)? {
            return Ok(Some(ws));
        }
    }

    Ok(None)
}

fn handle_workspace(cli: Cli) -> Result<()> {
    let config = Config::load()?;
    let beads_requested = cli
        .use_features
        .iter()
        .any(|feature| feature.eq_ignore_ascii_case("beads"));
    let beads_allowed = !cli.without_beads;

    // Validate unknown --use values
    for feature in &cli.use_features {
        if feature.to_lowercase() != "beads" {
            return Err(anyhow!(
                "Unknown feature '{}'. Supported values: beads",
                feature
            ));
        }
    }

    // Resolve core paths
    let mut core_paths = Vec::new();
    for path_str in &cli.include {
        let resolved = config.resolve_path(path_str)?;

        if !resolved.exists() {
            return Err(anyhow!("Path does not exist: {}", resolved.display()));
        }

        core_paths.push(resolved);
    }

    // Resolve sidecar paths
    let mut sidecar_paths = Vec::new();
    for path_str in &cli.sidecar {
        let resolved = config.resolve_path(path_str)?;

        if !resolved.exists() {
            return Err(anyhow!(
                "Sidecar path does not exist: {}",
                resolved.display()
            ));
        }

        sidecar_paths.push(resolved);
    }

    // Handle clone-from
    if let Some(clone_from) = cli.clone_from {
        let source_path = config.resolve_path(&clone_from)?;

        let workspace = Workspace::clone_from(&source_path, &core_paths, cli.temporary)?;

        if beads_allowed {
            workspace.setup_beads()?;
        }

        open_with_merge(&workspace)?;

        return Ok(());
    }

    if cli.temporary && !cli.force_new {
        if let Some(existing) = find_existing_workspace_for_paths(&core_paths)? {
            if !existing.temporary {
                return Err(anyhow!(
                    "A permanent safe pocket already exists for these paths: {}\nUse `safe_pocket -i ...` to open it, or pass `--new` if you really want a separate temporary pocket.",
                    existing.hash
                ));
            }
        }
    }

    if !cli.force_new {
        if let Some(mut existing) = find_existing_workspace_for_paths(&core_paths)? {
            if cli.temporary && !existing.temporary {
                return Err(anyhow!(
                    "Found a permanent safe pocket for these paths: {}\nTemporary mode only reuses temporary pockets.",
                    existing.hash
                ));
            }
            if verbose() {
                println!(
                    "{} {}",
                    "Opening existing workspace:".bright_green(),
                    existing.hash.bright_yellow()
                );
            }

            existing.migrate_storage_references()?;

            let should_setup_beads = beads_allowed
                && (beads_requested
                    || Manifest::load(&existing.pocket_dir)?
                        .map(|manifest| manifest.uses_beads)
                        .unwrap_or(false));

            if should_setup_beads {
                existing.setup_beads()?;
            }

            // If the CLI paths differ from this pocket's core_paths, the user is
            // opening via a registered worktree path. Inject those paths as sidecars
            // so VS Code shows the worktree branch's files alongside the pocket.
            let existing_path_set: std::collections::HashSet<_> =
                existing.core_paths.iter().collect();
            let extra_paths: Vec<PathBuf> = core_paths
                .iter()
                .filter(|p| !existing_path_set.contains(p))
                .cloned()
                .collect();

            if !extra_paths.is_empty() {
                existing.sidecar_paths.extend(extra_paths);
            }

            open_with_merge(&existing)?;
            return Ok(());
        }
    }

    // Create or open workspace
    let create_readmes = !cli.no_readme;
    let workspace = Workspace::new_with_options(
        core_paths.clone(),
        sidecar_paths,
        create_readmes,
        cli.temporary,
    )?;

    if !workspace.exists() {
        // Secondary lookup: check if any existing pocket's manifest matches these paths
        // (handles pockets that evolved in-place via sync/augment)
        if let Some(existing) = Workspace::find_workspace_by_manifest_paths(&core_paths)? {
            if existing.temporary == cli.temporary {
                if verbose() {
                    println!(
                        "{} {} (matched by manifest)",
                        "Found existing pocket:".bright_green(),
                        existing.hash.bright_yellow()
                    );
                }

                let should_setup_beads = beads_allowed
                    && (beads_requested
                        || Manifest::load(&existing.pocket_dir)?
                            .map(|manifest| manifest.uses_beads)
                            .unwrap_or(false));

                if should_setup_beads {
                    existing.setup_beads()?;
                }

                open_with_merge(&existing)?;
                return Ok(());
            }
        }

        // Check for similar workspaces (smart cloning)
        let similar_workspaces = Workspace::find_similar_workspaces(&core_paths, 0.3)?;

        if !similar_workspaces.is_empty() {
            if let Some(selected) = Workspace::prompt_clone_selection(&similar_workspaces)? {
                // Clone from selected workspace
                println!("Cloning from: {}", selected.hash.bright_yellow());

                // Copy safe pocket contents
                if workspace.pocket_dir.exists() {
                    registry::move_to_unhoused(
                        &workspace.pocket_dir,
                        "smart clone target replacement",
                    )?;
                    registry::remove_pocket(&workspace.pocket_dir)?;
                }

                copy_dir_all(&selected.pocket_dir, &workspace.pocket_dir)
                    .context("Failed to copy safe pocket contents")?;

                // Create workspace file with new paths
                workspace.create_workspace_file()?;

                // Write manifest with lineage
                let manifest = Manifest::new_cloned_with_options(
                    workspace.hash.clone(),
                    workspace.core_paths.clone(),
                    selected.hash.clone(),
                    workspace.temporary,
                );
                manifest.save(&workspace.pocket_dir)?;

                // Update parent's children list
                if let Ok(Some(mut parent_manifest)) = Manifest::load(&selected.pocket_dir) {
                    parent_manifest.add_child(workspace.hash.clone());
                    let _ = parent_manifest.save(&selected.pocket_dir);
                }

                println!(
                    "{} {}",
                    "Cloned to:".bright_green(),
                    workspace.hash.bright_yellow()
                );
            } else {
                // User chose not to clone
                workspace.create()?;
            }
        } else {
            // No similar workspaces found
            workspace.create()?;
        }

        if beads_allowed {
            workspace.setup_beads()?;
        }

        open_with_merge(&workspace)?;
    } else {
        if verbose() {
            println!("{}", "Using existing workspace".dimmed());
        }

        workspace.migrate_storage_references()?;

        // Run beads setup regardless of whether the pocket is new — idempotent
        let should_setup_beads = beads_allowed
            && (beads_requested
                || Manifest::load(&workspace.pocket_dir)?
                    .map(|manifest| manifest.uses_beads)
                    .unwrap_or(false));

        if should_setup_beads {
            workspace.setup_beads()?;
        }

        // Drift detection
        let drift_result = workspace.detect_and_resolve_drift()?;

        match drift_result {
            DriftResult::AcceptFile { new_core_paths } => {
                let mut manifest = match Manifest::load(&workspace.pocket_dir)? {
                    Some(m) => m,
                    None => Manifest::new_with_options(
                        workspace.hash.clone(),
                        workspace.core_paths.clone(),
                        workspace.temporary,
                    ),
                };
                manifest.update_paths(new_core_paths, &workspace.pocket_dir)?;
                println!(
                    "{} {}",
                    "Manifest updated in place:".bright_green(),
                    manifest.hash.bright_yellow()
                );
                open_with_merge(&workspace)?;
            }
            _ => {
                open_with_merge(&workspace)?;
            }
        }
    }

    Ok(())
}

fn handle_mark(mark: MarkChoice, pocket: String) -> Result<()> {
    match mark {
        MarkChoice::Temporary => mark_temporary(pocket),
    }
}

fn mark_temporary(pocket: String) -> Result<()> {
    let workspace = resolve_workspace_reference(&pocket)?;
    let manifest_path = workspace.pocket_dir.join("manifest.json");

    let mut manifest = Manifest::load(&workspace.pocket_dir)?.ok_or_else(|| {
        anyhow!(
            "No manifest found in safe pocket: {}",
            manifest_path.display()
        )
    })?;

    if manifest.temporary {
        println!(
            "{} {}",
            "Already marked temporary:".dimmed(),
            workspace.hash.bright_yellow()
        );
        return Ok(());
    }

    let target_dir = registry::temporary_registry_dir()?.join(&workspace.hash);
    if target_dir.exists() {
        bail!(
            "Cannot mark pocket temporary because target already exists: {}",
            target_dir.display()
        );
    }

    fs::create_dir_all(target_dir.parent().unwrap_or_else(|| Path::new("/")))
        .context("Failed to create temporary registry directory")?;
    fs::rename(&workspace.pocket_dir, &target_dir).with_context(|| {
        format!(
            "Failed to move safe pocket into temporary registry: {} -> {}",
            workspace.pocket_dir.display(),
            target_dir.display()
        )
    })?;

    manifest.temporary = true;
    manifest.save(&target_dir)?;
    registry::remove_pocket(&workspace.pocket_dir)?;

    println!(
        "{} {}",
        "Marked temporary:".bright_green(),
        target_dir.display().to_string().bright_blue()
    );

    Ok(())
}

fn handle_clean(
    scope: Option<CleanScope>,
    older_than: Option<String>,
    all: bool,
    hard: bool,
    yes: bool,
) -> Result<()> {
    let cache = registry::load_cache_or_rebuild()?;
    let entries: Vec<RegistryEntry> = if let Some(scope) = scope {
        match scope {
            CleanScope::Temporary => cache
                .pockets
                .into_iter()
                .filter(|entry| entry.temporary)
                .collect(),
        }
    } else if let Some(age) = older_than {
        let cutoff = parse_age_cutoff(&age)?;
        cache
            .pockets
            .into_iter()
            .filter(|entry| entry.created_at < cutoff)
            .collect()
    } else if all {
        cache.pockets
    } else {
        bail!("Specify `temporary`, `--older-than`, or `--all`.");
    };

    if entries.is_empty() {
        println!("{}", "Nothing to clean.".dimmed());
        return Ok(());
    }

    if hard {
        confirm_hard_clean(&entries, yes)?;
    }

    let count = entries.len();
    for entry in entries {
        if hard {
            delete_pocket_dir(&entry.path)?;
        }
        registry::remove_pocket(&entry.path)?;
    }

    if hard {
        println!(
            "{} {} pocket(s)",
            "Deleted safe pockets:".bright_green(),
            count.to_string().bright_yellow()
        );
    } else {
        println!(
            "{} {} registry entr{suffix}",
            "Removed:".bright_green(),
            count.to_string().bright_yellow(),
            suffix = if count == 1 { "y" } else { "ies" }
        );
    }

    Ok(())
}

fn parse_age_cutoff(age: &str) -> Result<chrono::DateTime<Utc>> {
    if age.len() < 2 {
        bail!("Invalid age '{age}'. Use values like 7d or 3m.");
    }

    let (value, unit) = age.split_at(age.len() - 1);
    let amount: i64 = value
        .parse()
        .with_context(|| format!("Invalid age value '{value}'"))?;

    let duration = match unit {
        "d" => Duration::days(amount),
        "m" => Duration::days(amount * 30),
        _ => bail!("Invalid age unit '{unit}'. Use d or m."),
    };

    Ok(Utc::now() - duration)
}

fn confirm_hard_clean(entries: &[RegistryEntry], yes: bool) -> Result<()> {
    if yes {
        return Ok(());
    }

    println!(
        "{}",
        "Hard clean will delete safe pocket directories from ~/.safe_pocket. Project folders are preserved.".bright_yellow()
    );
    for entry in entries {
        println!("  {}", entry.path.display().to_string().bright_blue());
    }
    println!();
    print!("{} ", "Continue? [y/N]:".bright_white());
    use std::io::Write as IoWrite;
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    if input == "y" || input == "yes" {
        Ok(())
    } else {
        bail!("Aborted.");
    }
}

fn delete_pocket_dir(path: &Path) -> Result<()> {
    let registry_root = registry::registry_root()?;
    if !path.starts_with(&registry_root) {
        bail!(
            "Refusing to delete path outside ~/.safe_pocket: {}",
            path.display()
        );
    }

    if path.exists() {
        registry::move_to_unhoused(path, "clean hard delete")?;
    }

    Ok(())
}

fn handle_heal(project: String, pocket: String) -> Result<()> {
    let config = Config::load()?;
    let project_path = config.resolve_path(&project)?;
    if !project_path.exists() {
        bail!("Project path does not exist: {}", project_path.display());
    }

    let source = resolve_workspace_reference(&pocket)?;
    let target =
        Workspace::new_with_options(vec![project_path.clone()], vec![], false, source.temporary)?;

    if source.pocket_dir == target.pocket_dir {
        let mut manifest = Manifest::load(&source.pocket_dir)?.ok_or_else(|| {
            anyhow!(
                "No manifest found in safe pocket: {}",
                source.pocket_dir.display()
            )
        })?;
        manifest.update_paths(vec![project_path], &source.pocket_dir)?;
        println!(
            "{} {}",
            "Healed in place:".bright_green(),
            source.pocket_dir.display().to_string().bright_blue()
        );
        return Ok(());
    }

    if target.pocket_dir.exists() {
        registry::move_to_unhoused(&target.pocket_dir, "heal target replacement")?;
        registry::remove_pocket(&target.pocket_dir)?;
    }

    if let Some(parent) = target.pocket_dir.parent() {
        fs::create_dir_all(parent).context("Failed to create target safe pocket parent")?;
    }

    fs::rename(&source.pocket_dir, &target.pocket_dir).or_else(|_| {
        copy_dir_all(&source.pocket_dir, &target.pocket_dir)?;
        fs::remove_dir_all(&source.pocket_dir)?;
        Ok::<(), anyhow::Error>(())
    })?;

    registry::remove_pocket(&source.pocket_dir)?;

    let workspace_file = Workspace::find_workspace_file(&target.pocket_dir);
    if let Some(old_file) = workspace_file {
        let new_file = target.workspace_file_path();
        if old_file != new_file && old_file.exists() {
            fs::rename(&old_file, &new_file).with_context(|| {
                format!(
                    "Failed to rename workspace file: {} -> {}",
                    old_file.display(),
                    new_file.display()
                )
            })?;
        }
    }

    target.create_workspace_file()?;
    let mut manifest = Manifest::load(&target.pocket_dir)?.unwrap_or_else(|| {
        Manifest::new_with_options(
            target.hash.clone(),
            target.core_paths.clone(),
            target.temporary,
        )
    });
    manifest.hash = target.hash.clone();
    manifest.core_paths = target.core_paths.clone();
    manifest.temporary = target.temporary;
    manifest.save(&target.pocket_dir)?;

    println!(
        "{} {} -> {}",
        "Healed safe pocket:".bright_green(),
        source.hash.bright_yellow(),
        target.pocket_dir.display().to_string().bright_blue()
    );
    Ok(())
}

fn handle_backup(repo: String, schedule: String) -> Result<()> {
    let backup_repo = registry::registry_root()?.with_file_name(".safe_pocket_backup_repo");
    let script_path = registry::registry_root()?.join("backup.sh");
    let source_dir = registry::registry_root()?;

    fs::create_dir_all(&source_dir).context("Failed to create safe pocket registry root")?;

    if !backup_repo.exists() {
        let output = std::process::Command::new("git")
            .args(["clone", &repo, &backup_repo.to_string_lossy()])
            .output()
            .context("Failed to clone backup repository")?;
        if !output.status.success() {
            bail!(
                "git clone failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    let script = format!(
        "#!/bin/sh\nset -eu\nrsync -a --delete --exclude '.git/' --exclude 'backup.sh' '{source}/' '{backup}/'\ncd '{backup}'\ngit add .\nif ! git diff --cached --quiet; then\n  git commit -m 'Back up safe pockets'\n  git push\nfi\n",
        source = source_dir.display(),
        backup = backup_repo.display()
    );
    fs::write(&script_path, script)
        .with_context(|| format!("Failed to write backup script: {}", script_path.display()))?;

    let _ = std::process::Command::new("chmod")
        .args(["+x", &script_path.to_string_lossy()])
        .status();

    let cron_line = format!("{} {}", schedule, script_path.display());
    let current = std::process::Command::new("crontab").arg("-l").output();
    let mut cron = current
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
        .unwrap_or_default();

    cron = cron
        .lines()
        .filter(|line| !line.contains(&script_path.to_string_lossy().to_string()))
        .map(str::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    if !cron.is_empty() {
        cron.push('\n');
    }
    cron.push_str(&cron_line);
    cron.push('\n');

    let mut child = std::process::Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("Failed to update crontab")?;
    child
        .stdin
        .as_mut()
        .context("Failed to open crontab stdin")?
        .write_all(cron.as_bytes())
        .context("Failed to write crontab")?;
    let status = child.wait().context("Failed to wait for crontab")?;
    if !status.success() {
        bail!("crontab update failed");
    }

    println!(
        "{} {}",
        "Backup cron configured:".bright_green(),
        cron_line.bright_blue()
    );
    Ok(())
}

fn resolve_workspace_reference(reference: &str) -> Result<Workspace> {
    let config = Config::load()?;
    let resolved = config.resolve_path(reference)?;

    if resolved.is_dir() && registry::is_registry_pocket_dir(&resolved)? {
        let manifest = Manifest::load(&resolved)?.ok_or_else(|| {
            anyhow!(
                "No manifest found in pocket directory: {}",
                resolved.display()
            )
        })?;

        return Ok(Workspace {
            hash: resolved
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&manifest.hash)
                .to_string(),
            core_paths: manifest.core_paths.clone(),
            sidecar_paths: vec![],
            pocket_dir: resolved,
            create_readmes: false,
            temporary: manifest.temporary,
        });
    }

    if let Some(workspace) = Workspace::find_workspace_containing(&resolved)?
        .or_else(|| Workspace::find_workspace_for_cwd(&resolved).ok().flatten())
    {
        return Ok(workspace);
    }

    for entry in registry::load_cache_or_rebuild()?.pockets {
        if entry.hash == reference {
            return Ok(Workspace {
                hash: entry.hash,
                core_paths: entry.core_paths,
                sidecar_paths: vec![],
                pocket_dir: entry.path,
                create_readmes: false,
                temporary: entry.temporary,
            });
        }
    }

    Err(anyhow!("No safe pocket found for reference: {}", reference))
}

fn handle_sync(pocket: String) -> Result<()> {
    let pocket_dir = PathBuf::from(&pocket);

    if !pocket_dir.is_dir() {
        let out = serde_json::json!({
            "status": "error",
            "message": format!("Pocket directory does not exist: {}", pocket)
        });
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    // Find the workspace file
    let workspace_file = match Workspace::find_workspace_file(&pocket_dir) {
        Some(f) => f,
        None => {
            let out = serde_json::json!({
                "status": "error",
                "message": "No workspace file found in pocket directory"
            });
            println!("{}", serde_json::to_string(&out)?);
            return Ok(());
        }
    };

    let workspace_hash = pocket_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("")
        .to_string();
    let migration_workspace = Workspace {
        hash: workspace_hash,
        core_paths: vec![],
        sidecar_paths: vec![],
        pocket_dir: pocket_dir.clone(),
        create_readmes: false,
        temporary: pocket_dir.starts_with(Workspace::temporary_spocket_dir()?),
    };
    migration_workspace.migrate_storage_references()?;

    // Read current paths from workspace file
    let (_, file_paths) = Workspace::read_workspace_file(&workspace_file, &pocket_dir)?;

    // Load or backfill manifest
    let (mut manifest, manifest_paths) = match Workspace::load_manifest_or_backfill(&pocket_dir)? {
        Some(result) => result,
        None => {
            let out = serde_json::json!({
                "status": "error",
                "message": "Failed to load or create manifest"
            });
            println!("{}", serde_json::to_string(&out)?);
            return Ok(());
        }
    };

    // Compare
    let file_set: std::collections::HashSet<_> = file_paths.iter().collect();
    let manifest_set: std::collections::HashSet<_> = manifest_paths.iter().collect();

    if file_set == manifest_set {
        let out = serde_json::json!({
            "status": "unchanged",
            "hash": manifest.hash,
            "birth_hash": manifest.birth_hash(),
            "paths": manifest.core_paths,
        });
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    // Paths differ — update manifest in place
    let old_hash = manifest.hash.clone();
    manifest.update_paths(file_paths, &pocket_dir)?;

    let out = serde_json::json!({
        "status": "synced",
        "old_hash": old_hash,
        "new_hash": manifest.hash,
        "birth_hash": manifest.birth_hash(),
        "paths": manifest.core_paths,
    });
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}

fn handle_augment(add: Vec<String>, remove: Vec<String>, no_open: bool) -> Result<()> {
    if add.is_empty() && remove.is_empty() {
        return Err(anyhow!(
            "Nothing to do. Use --add or --remove to modify the workspace."
        ));
    }

    let config = Config::load()?;
    let cwd = std::env::current_dir().context("Failed to get current working directory")?;

    let workspace = Workspace::find_workspace_for_cwd(&cwd)?
        .ok_or_else(|| anyhow!(
            "No workspace found for current directory: {}\nRun this from inside a workspace directory or a pocket directory.",
            cwd.display()
        ))?;

    println!(
        "{} {}",
        "Found workspace:".bright_white(),
        workspace.hash.bright_yellow()
    );

    // Build new core_paths
    let mut new_paths: Vec<PathBuf> = workspace.core_paths.clone();

    // Process additions
    for path_str in &add {
        let resolved = config.resolve_path(path_str)?;

        if !resolved.exists() {
            return Err(anyhow!("Path does not exist: {}", resolved.display()));
        }

        if new_paths.contains(&resolved) {
            println!(
                "  {} {} (already in workspace)",
                "Skipped:".dimmed(),
                resolved.display().to_string().bright_blue()
            );
        } else {
            println!(
                "  {} {}",
                "Adding:".bright_green(),
                resolved.display().to_string().bright_blue()
            );
            new_paths.push(resolved);
        }
    }

    // Process removals
    for path_str in &remove {
        let resolved = config.resolve_path(path_str)?;
        let before_len = new_paths.len();
        new_paths.retain(|p| p != &resolved);

        if new_paths.len() < before_len {
            println!(
                "  {} {}",
                "Removing:".bright_red(),
                resolved.display().to_string().bright_blue()
            );
        } else {
            println!(
                "  {} {} (not in workspace)",
                "Skipped:".dimmed(),
                resolved.display().to_string().bright_blue()
            );
        }
    }

    // Validate
    if new_paths.is_empty() {
        return Err(anyhow!(
            "Cannot remove all directories. At least one directory must remain."
        ));
    }

    // Check if anything actually changed
    let new_paths_set: std::collections::HashSet<_> = new_paths.iter().collect();
    let old_paths_set: std::collections::HashSet<_> = workspace.core_paths.iter().collect();

    if new_paths_set == old_paths_set {
        println!("{}", "No changes to apply.".dimmed());
        return Ok(());
    }

    // Read existing workspace file for settings preservation
    let workspace_file = workspace.workspace_file_path();
    let existing_ws = if workspace_file.exists() {
        workspace.migrate_storage_references()?;
        let (ws, _) = Workspace::read_workspace_file(&workspace_file, &workspace.pocket_dir)?;
        Some(ws)
    } else {
        None
    };

    // In-place update: rewrite workspace file + manifest, pocket dir stays put
    let updated_workspace = Workspace {
        hash: workspace.hash.clone(),
        core_paths: new_paths.clone(),
        sidecar_paths: workspace.sidecar_paths.clone(),
        pocket_dir: workspace.pocket_dir.clone(),
        create_readmes: false,
        temporary: workspace.temporary,
    };
    updated_workspace.write_workspace_file_preserving(existing_ws.as_ref())?;

    // Update manifest in place
    let mut manifest = match Manifest::load(&workspace.pocket_dir)? {
        Some(m) => m,
        None => Manifest::new_with_options(
            workspace.hash.clone(),
            workspace.core_paths.clone(),
            workspace.temporary,
        ),
    };
    manifest.update_paths(new_paths, &workspace.pocket_dir)?;

    println!(
        "{} {} (pocket dir unchanged)",
        "Workspace updated in place:".bright_green(),
        manifest.hash.bright_yellow()
    );

    if !no_open {
        open_with_merge(&updated_workspace)?;
    } else {
        println!(
            "  {} {}",
            "Location:".dimmed(),
            workspace.pocket_dir.display().to_string().bright_blue()
        );
    }

    Ok(())
}

fn open_with_merge(ws: &Workspace) -> Result<()> {
    if let Ok(ctx) = build_template_context(&ws.pocket_dir) {
        if let Err(e) = template::apply_merge_at_runtime(&ws.pocket_dir, &ctx) {
            eprintln!(
                "{} {}",
                "Warning: merge-at-runtime failed:".bright_yellow(),
                e
            );
        }
    }
    ws.open()
}

fn build_template_context(pocket_dir: &std::path::Path) -> Result<template::TemplateContext> {
    let manifest = Manifest::load(pocket_dir)?
        .ok_or_else(|| anyhow!("No manifest found in pocket: {}", pocket_dir.display()))?;

    let project_root = manifest
        .core_paths
        .first()
        .cloned()
        .unwrap_or_else(|| PathBuf::from("<unknown>"));

    let spocket_name = pocket_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let global_obs = template::global_observations_dir().unwrap_or_else(|_| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join(".safe_pocket")
            .join("observations")
    });

    Ok(template::TemplateContext {
        spocket_root: pocket_dir.to_path_buf(),
        project_root,
        spocket_name,
        global_observations_path: global_obs,
        uses_beads: manifest.uses_beads,
    })
}

fn handle_merge_start(pocket: String) -> Result<()> {
    let pocket_dir = PathBuf::from(&pocket);

    if !pocket_dir.is_dir() {
        return Err(anyhow!("Pocket directory does not exist: {}", pocket));
    }

    let ctx = build_template_context(&pocket_dir)?;
    let count = template::apply_merge_at_runtime(&pocket_dir, &ctx)?;

    if count == 0 {
        println!("{}", "No runtime merge templates found.".dimmed());
    } else {
        println!(
            "{} {} file(s) runtime-merged.",
            "Merge-start complete:".bright_green(),
            count.to_string().bright_yellow()
        );
    }

    Ok(())
}

fn handle_merge_stop(pocket: String) -> Result<()> {
    let pocket_dir = PathBuf::from(&pocket);

    if !pocket_dir.is_dir() {
        return Err(anyhow!("Pocket directory does not exist: {}", pocket));
    }

    let ctx = build_template_context(&pocket_dir)?;
    let count = template::strip_merge_at_runtime(&pocket_dir, &ctx)?;

    if count == 0 {
        println!("{}", "No runtime content to strip.".dimmed());
    } else {
        println!(
            "{} {} file(s) cleaned.",
            "Merge-stop complete:".bright_green(),
            count.to_string().bright_yellow()
        );
    }

    Ok(())
}

fn handle_upgrade(path: String) -> Result<()> {
    let config = Config::load()?;
    let resolved = config.resolve_path(&path)?;

    // The path might be:
    // 1. A pocket directory directly (e.g. ~/.safe_pocket/abc123)
    // 2. A project directory that has an associated pocket
    let spocket_dir = Workspace::spocket_dir()?;
    let temporary_spocket_dir = Workspace::temporary_spocket_dir()?;

    let pocket_dir = if (resolved.starts_with(&spocket_dir)
        || resolved.starts_with(&temporary_spocket_dir))
        && resolved.is_dir()
    {
        // Direct pocket path
        resolved
    } else {
        // Try to find the pocket for this project path
        let workspace = Workspace::find_workspace_containing(&resolved)?
            .or_else(|| {
                // Also try find_workspace_for_cwd
                Workspace::find_workspace_for_cwd(&resolved).ok().flatten()
            })
            .ok_or_else(|| {
                anyhow!(
                    "No safe pocket found for path: {}\n\
                     Provide either a pocket directory or a project directory with an existing pocket.",
                    resolved.display()
                )
            })?;
        workspace.pocket_dir
    };

    template::upgrade_pocket(&pocket_dir)
}

// Helper function to copy directory contents
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    use std::fs;

    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}
