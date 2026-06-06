mod agents;
mod cli;
mod config;
mod event;
mod feature;
mod hash;
mod manifest;
mod registry;
mod task;
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
use std::process::Command;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

use cli::{CleanScope, Cli, Commands, MarkChoice, WorktreeAction};
use config::Config;
use manifest::Manifest;
use registry::RegistryEntry;
use workspace::{DriftResult, VSCodeWorkspace, Workspace};

static VERBOSE: OnceLock<bool> = OnceLock::new();
static SUPPRESS_OPEN: OnceLock<bool> = OnceLock::new();

pub fn verbose() -> bool {
    *VERBOSE.get().unwrap_or(&false)
}

/// Whether VS Code should be suppressed at the end of a workspace command.
///
/// Set by `--silent` (skip opening) and `--simulate-runtime` (inject runtime
/// content but do not launch the editor).
pub fn suppress_open() -> bool {
    *SUPPRESS_OPEN.get().unwrap_or(&false)
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
    let _ = SUPPRESS_OPEN.set(cli.silent || cli.simulate_runtime);

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
            let _ = event::append_registry_event(
                "alias.register",
                serde_json::json!({ "alias": name, "path": path }),
            );

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
                let _ = event::append_registry_event(
                    "alias.unregister",
                    serde_json::json!({ "alias": name }),
                );
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

        Commands::ListAliases => {
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

        Commands::Sync { target, pocket } => handle_sync(target, pocket),

        Commands::RuntimeMergeStart { pocket } => handle_merge_start(pocket),

        Commands::RuntimeMergeStop { pocket } => handle_merge_stop(pocket),

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

        Commands::Heal {
            project,
            alias,
            pocket,
        } => handle_heal(project, alias, pocket),

        Commands::Locate { path } => handle_locate(path),

        Commands::SyncRegistryGit => handle_sync_registry_git(),

        Commands::Backup { repo, schedule } => handle_backup(repo, schedule),

        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let shell: clap_complete::Shell = shell.into();
            generate(shell, &mut cmd, "safe_pocket", &mut io::stdout());
            Ok(())
        }

        Commands::CompletionSpec => handle_completion_spec(),

        Commands::Worktree { action } => handle_worktree(action),

        Commands::DailyFeature {
            pocket,
            new,
            subpath,
        } => handle_daily_feature(pocket, new, subpath),

        Commands::Task { args } => task::run_cli(args),
    }
}

fn handle_daily_feature(pocket: String, new: bool, subpath: Option<String>) -> Result<()> {
    let pocket_dir = PathBuf::from(&pocket);

    if !pocket_dir.is_dir() {
        let out = serde_json::json!({
            "status": "error",
            "message": format!("Pocket directory does not exist: {}", pocket)
        });
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    let subpath = subpath
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| feature::DEFAULT_DAILY_SUBPATH.to_string());

    let feature_tags_yaml = template::safe_pocket_config_dir()
        .unwrap_or_else(|_| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("/"))
                .join("safe_pocket")
        })
        .join("feature_tags.yaml");

    match feature::resolve_daily_feature(&pocket_dir, &subpath, new, &feature_tags_yaml) {
        Ok(outcome) => {
            let _ = event::append_pocket_event(
                &pocket_dir,
                if outcome.created {
                    "daily_feature.create"
                } else {
                    "daily_feature.open"
                },
                serde_json::json!({ "path": outcome.path, "new": new }),
            );
            let out = serde_json::json!({
                "status": "ok",
                "path": outcome.path,
                "created": outcome.created,
            });
            println!("{}", serde_json::to_string(&out)?);
        }
        Err(e) => {
            let out = serde_json::json!({
                "status": "error",
                "message": e.to_string(),
            });
            println!("{}", serde_json::to_string(&out)?);
        }
    }

    Ok(())
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

fn repair_empty_workspace_paths(
    workspace: &mut Workspace,
    fallback_paths: &[PathBuf],
) -> Result<()> {
    if workspace.core_paths.is_empty() && !fallback_paths.is_empty() {
        workspace.core_paths = fallback_paths.to_vec();
    }

    if workspace.core_paths.is_empty() {
        return Ok(());
    }

    if let Some(mut manifest) = Manifest::load(&workspace.pocket_dir)? {
        if manifest.core_paths.is_empty() {
            manifest.update_paths(workspace.core_paths.clone(), &workspace.pocket_dir)?;
        }
    }

    if let Some(workspace_file) = Workspace::find_workspace_file(&workspace.pocket_dir) {
        let (existing, file_paths) =
            Workspace::read_workspace_file(&workspace_file, &workspace.pocket_dir)
                .map(|(ws, paths)| (Some(ws), paths))
                .unwrap_or((None, Vec::new()));
        if file_paths.is_empty() {
            workspace.write_workspace_file_preserving(existing.as_ref())?;
        }
    }

    Ok(())
}

fn handle_workspace(cli: Cli) -> Result<()> {
    let config = Config::load()?;

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

        let mut workspace = Workspace::clone_from(&source_path, &core_paths, cli.temporary)?;

        workspace.create_pocket_structure()?;

        apply_session_tools(&cli.with_tools, &mut workspace)?;
        apply_project_tools(&cli.add_tools, &workspace)?;

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

            repair_empty_workspace_paths(&mut existing, &core_paths)?;
            existing.migrate_storage_references()?;

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
            existing.sidecar_paths.extend(sidecar_paths.clone());

            apply_session_tools(&cli.with_tools, &mut existing)?;

            let drift_result = existing.detect_and_resolve_drift()?;
            if let DriftResult::AcceptFile { new_core_paths } = drift_result {
                let mut manifest = Manifest::load(&existing.pocket_dir)?.unwrap_or_else(|| {
                    Manifest::new_with_options(
                        existing.hash.clone(),
                        existing.core_paths.clone(),
                        existing.temporary,
                    )
                });
                manifest.update_paths(new_core_paths.clone(), &existing.pocket_dir)?;
                existing.core_paths = new_core_paths;
            }

            apply_project_tools(&cli.add_tools, &existing)?;
            open_with_merge(&existing)?;
            return Ok(());
        }
    }

    // Create or open workspace
    let create_readmes = !cli.no_readme;
    let workspace = Workspace::new_with_options(
        core_paths.clone(),
        sidecar_paths.clone(),
        create_readmes,
        cli.temporary,
    )?;

    if !workspace.exists() {
        // Secondary lookup: check if any existing pocket's manifest matches these paths
        // (handles pockets that evolved in-place via sync/augment)
        if let Some(mut existing) = Workspace::find_workspace_by_manifest_paths(&core_paths)? {
            if existing.temporary == cli.temporary {
                if verbose() {
                    println!(
                        "{} {} (matched by manifest)",
                        "Found existing pocket:".bright_green(),
                        existing.hash.bright_yellow()
                    );
                }

                existing.sidecar_paths.extend(sidecar_paths.clone());
                apply_session_tools(&cli.with_tools, &mut existing)?;
                apply_project_tools(&cli.add_tools, &existing)?;
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
                let mut workspace = workspace;
                apply_session_tools(&cli.with_tools, &mut workspace)?;
                apply_project_tools(&cli.add_tools, &workspace)?;
                open_with_merge(&workspace)?;
                return Ok(());
            } else {
                // User chose not to clone
                workspace.create()?;
                let mut workspace = workspace;
                apply_session_tools(&cli.with_tools, &mut workspace)?;
                apply_project_tools(&cli.add_tools, &workspace)?;
                open_with_merge(&workspace)?;
                return Ok(());
            }
        } else {
            // No similar workspaces found
            workspace.create()?;
            let mut workspace = workspace;
            apply_session_tools(&cli.with_tools, &mut workspace)?;
            apply_project_tools(&cli.add_tools, &workspace)?;
            open_with_merge(&workspace)?;
            return Ok(());
        }
    } else {
        if verbose() {
            println!("{}", "Using existing workspace".dimmed());
        }

        let mut workspace = workspace;
        repair_empty_workspace_paths(&mut workspace, &core_paths)?;
        workspace.migrate_storage_references()?;

        apply_session_tools(&cli.with_tools, &mut workspace)?;

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
                manifest.update_paths(new_core_paths.clone(), &workspace.pocket_dir)?;
                workspace.core_paths = new_core_paths;
                println!(
                    "{} {}",
                    "Manifest updated in place:".bright_green(),
                    manifest.hash.bright_yellow()
                );
                apply_project_tools(&cli.add_tools, &workspace)?;
                open_with_merge(&workspace)?;
            }
            _ => {
                apply_project_tools(&cli.add_tools, &workspace)?;
                open_with_merge(&workspace)?;
            }
        }
    }

    Ok(())
}

fn normalize_tool_name(name: &str) -> Result<String> {
    let normalized = name.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "gitleaks" | "graphify" | "memgraph" => Ok(normalized),
        _ => Err(anyhow!(
            "Unsupported tool '{name}'. Supported tools: gitleaks, graphify, memgraph."
        )),
    }
}

fn apply_session_tools(names: &[String], workspace: &mut Workspace) -> Result<()> {
    for name in names {
        let tool = normalize_tool_name(name)?;
        let dir = workspace.pocket_dir.join(".session-tools").join(&tool);
        prepare_tool_dir(&dir, &tool, workspace)?;
        if !workspace.sidecar_paths.contains(&dir) {
            workspace.sidecar_paths.push(dir);
        }
    }
    Ok(())
}

fn apply_project_tools(names: &[String], workspace: &Workspace) -> Result<()> {
    for name in names {
        let tool = normalize_tool_name(name)?;
        if project_tool_installed(&tool, workspace) {
            if verbose() {
                println!(
                    "{} {}",
                    "Tool already installed:".dimmed(),
                    tool.bright_yellow()
                );
            }
            continue;
        }
        match tool.as_str() {
            "gitleaks" => install_gitleaks(workspace)?,
            "graphify" => install_graphify(workspace)?,
            "memgraph" => install_memgraph(workspace)?,
            _ => unreachable!(),
        }
    }
    Ok(())
}

fn project_tool_installed(tool: &str, workspace: &Workspace) -> bool {
    match tool {
        "gitleaks" => {
            workspace
                .pocket_dir
                .join("tools/gitleaks/pre-commit-hook.sh")
                .is_file()
                && workspace.core_paths.iter().all(|project| {
                    project.join(".gitleaks.toml").is_file()
                        && project.join(".git/hooks/pre-commit").is_file()
                })
        }
        "graphify" => {
            workspace
                .pocket_dir
                .join("tools/graphify/README.md")
                .is_file()
                && workspace.pocket_dir.join("graphify-out").is_dir()
        }
        "memgraph" => {
            let dir = workspace.pocket_dir.join("tools/memgraph");
            dir.join("scan-config.json").is_file()
                && dir.join("docker-compose.yml").is_file()
                && dir.join("schema.cypher").is_file()
                && dir.join("scan-safe-pocket.sh").is_file()
        }
        _ => false,
    }
}

fn prepare_tool_dir(dir: &Path, tool: &str, workspace: &Workspace) -> Result<()> {
    fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create {tool} tool dir: {}", dir.display()))?;
    let readme = dir.join("README.md");
    if !readme.exists() {
        fs::write(&readme, tool_readme(tool))
            .with_context(|| format!("Failed to write tool README: {}", readme.display()))?;
    }
    if tool == "memgraph" {
        write_memgraph_config(dir, workspace)?;
    }
    Ok(())
}

fn tool_readme(tool: &str) -> String {
    match tool {
        "gitleaks" => "# gitleaks\n\nManaged by safe_pocket. Use `gitleaks detect --source <project>` to scan for secrets.\n".to_string(),
        "graphify" => "# graphify\n\nManaged by safe_pocket. Graph output is stored in the safe pocket and may be bridged into the project.\n".to_string(),
        "memgraph" => "# Memgraph relational memory\n\nManaged by safe_pocket. This directory contains a Memgraph configuration, Docker Compose file, Cypher schema, and a scanner for the safe pocket's FEATURES tree and relevant markdown context files.\n".to_string(),
        _ => format!("# {tool}\n\nManaged by safe_pocket.\n"),
    }
}

fn install_gitleaks(workspace: &Workspace) -> Result<()> {
    let tool_dir = workspace.pocket_dir.join("tools").join("gitleaks");
    fs::create_dir_all(&tool_dir)
        .with_context(|| format!("Failed to create gitleaks tool dir: {}", tool_dir.display()))?;
    let readme = tool_dir.join("README.md");
    if !readme.exists() {
        fs::write(&readme, tool_readme("gitleaks"))?;
    }
    let helper = tool_dir.join("pre-commit-hook.sh");
    fs::write(&helper, gitleaks_pre_commit_helper())?;
    set_executable(&helper)?;

    for project in &workspace.core_paths {
        let config_path = project.join(".gitleaks.toml");
        if !config_path.exists() {
            fs::write(
                &config_path,
                "title = \"safe_pocket gitleaks guard\"\n\n[extend]\nuseDefault = true\n",
            )
            .with_context(|| format!("Failed to write {}", config_path.display()))?;
        }

        if let Some(git_hooks) = git_hooks_dir(project) {
            let hook = git_hooks.join("pre-commit");
            let body = format!(
                "#!/bin/sh\nexec \"{}\" \"{}\" \"{}\"\n",
                helper.display(),
                project.display(),
                config_path.display()
            );
            fs::write(&hook, body)
                .with_context(|| format!("Failed to write {}", hook.display()))?;
            set_executable(&hook)?;
        }
    }

    add_persistent_workspace_folder(workspace, &tool_dir, "[Tool] gitleaks")
}

fn gitleaks_pre_commit_helper() -> &'static str {
    "#!/bin/sh\nset -eu\nSELF_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nPROJECT_DIR=${1:-$(pwd)}\nCONFIG_PATH=${2:-$PROJECT_DIR/.gitleaks.toml}\nLOCAL_GITLEAKS=\"$SELF_DIR/bin/gitleaks\"\ncd \"$PROJECT_DIR\"\nif [ -x \"$LOCAL_GITLEAKS\" ]; then\n  exec \"$LOCAL_GITLEAKS\" protect --staged --redact --config \"$CONFIG_PATH\"\nfi\nif command -v gitleaks >/dev/null 2>&1; then\n  exec gitleaks protect --staged --redact --config \"$CONFIG_PATH\"\nfi\nprintf '%s\\n' 'safe_pocket: gitleaks is required but was not found.' >&2\nprintf '%s\\n' 'Install gitleaks on PATH, or place the binary at:' >&2\nprintf '  %s\\n' \"$LOCAL_GITLEAKS\" >&2\nexit 1\n"
}

fn git_hooks_dir(project: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .current_dir(project)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if raw.is_empty() {
        return None;
    }
    let git_dir = PathBuf::from(raw);
    let git_dir = if git_dir.is_absolute() {
        git_dir
    } else {
        project.join(git_dir)
    };
    let hooks = git_dir.join("hooks");
    if hooks.is_dir() {
        Some(hooks)
    } else {
        None
    }
}

fn install_graphify(workspace: &Workspace) -> Result<()> {
    let tool_dir = workspace.pocket_dir.join("tools").join("graphify");
    let graph_dir = workspace.pocket_dir.join("graphify-out");
    fs::create_dir_all(&tool_dir)
        .with_context(|| format!("Failed to create graphify tool dir: {}", tool_dir.display()))?;
    fs::create_dir_all(&graph_dir).with_context(|| {
        format!(
            "Failed to create graphify output dir: {}",
            graph_dir.display()
        )
    })?;
    let readme = tool_dir.join("README.md");
    if !readme.exists() {
        fs::write(&readme, tool_readme("graphify"))?;
    }
    fs::write(
        graph_dir.join(".graphify_root"),
        workspace
            .core_paths
            .first()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )?;

    for project in &workspace.core_paths {
        bridge_graphify_dir(project, &graph_dir)?;
    }

    add_persistent_workspace_folder(workspace, &tool_dir, "[Tool] graphify")
}

fn install_memgraph(workspace: &Workspace) -> Result<()> {
    let tool_dir = workspace.pocket_dir.join("tools").join("memgraph");
    prepare_tool_dir(&tool_dir, "memgraph", workspace)?;
    add_persistent_workspace_folder(workspace, &tool_dir, "[Tool] memgraph")
}

fn write_memgraph_config(tool_dir: &Path, workspace: &Workspace) -> Result<()> {
    fs::create_dir_all(tool_dir.join("data"))?;
    fs::create_dir_all(tool_dir.join("logs"))?;
    fs::create_dir_all(tool_dir.join("import"))?;

    let scan_paths = memgraph_scan_paths(workspace);
    let scan_values = scan_paths
        .iter()
        .map(|p| serde_json::json!(p.to_string_lossy().to_string()))
        .collect::<Vec<_>>();
    let config = serde_json::json!({
        "engine": "memgraph",
        "protocol": "bolt",
        "bolt_url": "bolt://127.0.0.1:7687",
        "query_language": "cypher",
        "ontology": {
            "nodes": ["Observation", "Feature", "File", "Concept"],
            "edges": ["MODIFIES", "RESOLVES_ISSUE_IN", "REQUIRES", "IMPLEMENTS"]
        },
        "scan": {
            "mode": "markdown",
            "recursive": true,
            "paths": scan_values,
            "output_jsonl": tool_dir.join("import/markdown-files.jsonl").to_string_lossy().to_string()
        }
    });
    fs::write(
        tool_dir.join("scan-config.json"),
        serde_json::to_string_pretty(&config)?,
    )?;

    fs::write(tool_dir.join("schema.cypher"), memgraph_schema_cypher())?;
    fs::write(tool_dir.join("docker-compose.yml"), memgraph_compose_yaml())?;
    let scanner = tool_dir.join("scan-safe-pocket.sh");
    fs::write(&scanner, memgraph_scanner_script())?;
    set_executable(&scanner)?;
    Ok(())
}

fn memgraph_scan_paths(workspace: &Workspace) -> Vec<PathBuf> {
    let mut paths = vec![workspace.pocket_dir.join("FEATURES")];
    for name in ["AGENTS.md", "GEMINI.md", "README.md", "Install.md"] {
        let candidate = workspace.pocket_dir.join(name);
        if candidate.exists() || name == "AGENTS.md" {
            paths.push(candidate);
        }
    }
    paths
}

fn memgraph_schema_cypher() -> &'static str {
    "CREATE INDEX ON :Observation(id);\nCREATE INDEX ON :Feature(name);\nCREATE INDEX ON :File(path);\nCREATE INDEX ON :Concept(name);\n"
}

fn memgraph_compose_yaml() -> &'static str {
    "services:\n  memgraph:\n    image: memgraph/memgraph-mage:latest\n    command: [\"--bolt-address=0.0.0.0\", \"--bolt-port=7687\"]\n    ports:\n      - \"7687:7687\"\n      - \"7444:7444\"\n    volumes:\n      - ./data:/var/lib/memgraph\n      - ./logs:/var/log/memgraph\n      - ./import:/var/opt/memgraph/import\n"
}

fn memgraph_scanner_script() -> &'static str {
    "#!/bin/sh\nset -eu\nCONFIG_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nOUT=\"$CONFIG_DIR/import/markdown-files.jsonl\"\n: > \"$OUT\"\npython3 - \"$CONFIG_DIR/scan-config.json\" \"$OUT\" <<'PY'\nimport json, sys\nfrom pathlib import Path\nconfig = json.loads(Path(sys.argv[1]).read_text())\nout = Path(sys.argv[2])\nwith out.open('a', encoding='utf-8') as fh:\n    for raw in config['scan']['paths']:\n        p = Path(raw).expanduser()\n        files = sorted(p.rglob('*.md')) if p.is_dir() else ([p] if p.is_file() and p.suffix.lower() == '.md' else [])\n        for md in files:\n            text = md.read_text(encoding='utf-8', errors='replace')\n            title = next((line.lstrip('#').strip() for line in text.splitlines() if line.startswith('#')), md.stem)\n            fh.write(json.dumps({'path': str(md), 'title': title, 'bytes': len(text.encode('utf-8'))}) + '\\n')\nPY\nprintf 'Wrote %s\\n' \"$OUT\"\n"
}

fn bridge_graphify_dir(project: &Path, graph_dir: &Path) -> Result<()> {
    let link = project.join("graphify-out");
    if link.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(graph_dir, &link).with_context(|| {
            format!(
                "Failed to link {} -> {}",
                link.display(),
                graph_dir.display()
            )
        })?;
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(&link)
            .with_context(|| format!("Failed to create graphify bridge dir: {}", link.display()))?;
    }
    Ok(())
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path)?.permissions();
        perms.set_mode(perms.mode() | 0o755);
        fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn add_persistent_workspace_folder(workspace: &Workspace, path: &Path, name: &str) -> Result<()> {
    let workspace_file = workspace.workspace_file_path();
    if !workspace_file.exists() {
        return Ok(());
    }
    let mut text = fs::read_to_string(&workspace_file)
        .with_context(|| format!("Failed to read {}", workspace_file.display()))?;
    let existing_workspace: VSCodeWorkspace = serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse {}", workspace_file.display()))?;
    let (_, file_paths) = Workspace::read_workspace_file(&workspace_file, &workspace.pocket_dir)?;
    if file_paths.is_empty() && !workspace.core_paths.is_empty() {
        workspace.write_workspace_file_preserving(Some(&existing_workspace))?;
        text = fs::read_to_string(&workspace_file)
            .with_context(|| format!("Failed to read {}", workspace_file.display()))?;
    }
    let mut doc: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("Failed to parse {}", workspace_file.display()))?;
    let folders = doc
        .get_mut("folders")
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| {
            anyhow!(
                "Workspace file has no folders array: {}",
                workspace_file.display()
            )
        })?;
    let path_text = path.to_string_lossy().to_string();
    if folders
        .iter()
        .any(|f| f.get("path").and_then(|p| p.as_str()) == Some(path_text.as_str()))
    {
        return Ok(());
    }
    folders.push(serde_json::json!({ "path": path_text, "name": name }));
    fs::write(&workspace_file, serde_json::to_string_pretty(&doc)?)
        .with_context(|| format!("Failed to write {}", workspace_file.display()))?;
    Ok(())
}

fn handle_completion_spec() -> Result<()> {
    let spec = serde_json::json!({
        "commands": {
            "top_level_flags": ["--include", "--sidecar", "--with", "--add", "--clone-from", "--temporary", "--no-readme", "--upgrade", "--new", "--verbose", "--simulate-runtime", "--silent"],
            "tools": ["gitleaks", "graphify", "memgraph"],
            "subcommands": {
                "task": ["list", "create", "assign", "start", "log", "close", "discard", "describe", "reprefix"],
                "worktree": ["add", "remove", "list"],
                "completions": ["bash", "zsh", "fish", "powershell", "elvish"]
            }
        }
    });
    println!("{}", serde_json::to_string_pretty(&spec)?);
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

fn handle_heal(
    project: Option<String>,
    alias: Option<String>,
    pocket: Option<String>,
) -> Result<()> {
    let config = Config::load()?;
    let project_ref = match (project, alias) {
        (Some(_), Some(_)) => bail!("Use either --project or --alias, not both."),
        (Some(project), None) => project,
        (None, Some(alias)) => config
            .aliases
            .get(&alias)
            .cloned()
            .ok_or_else(|| anyhow!("Alias not found: {alias}"))?,
        (None, None) => std::env::current_dir()
            .context("Failed to get current directory")?
            .to_string_lossy()
            .to_string(),
    };
    let project_path = config.resolve_path(&project_ref)?;
    if !project_path.exists() {
        bail!("Project path does not exist: {}", project_path.display());
    }

    let pocket_ref = match pocket {
        Some(pocket) => pocket,
        None => prompt_heal_pocket(&project_path)?,
    };
    let source = resolve_workspace_reference(&pocket_ref)?;
    let target =
        Workspace::new_with_options(vec![project_path.clone()], vec![], false, source.temporary)?;

    if source.pocket_dir == target.pocket_dir {
        let existing_ws =
            Workspace::find_workspace_file(&source.pocket_dir).and_then(|workspace_file| {
                Workspace::read_workspace_file(&workspace_file, &source.pocket_dir)
                    .ok()
                    .map(|(ws, _)| ws)
            });
        target.write_workspace_file_preserving(existing_ws.as_ref())?;

        let mut manifest = Manifest::load(&source.pocket_dir)?.ok_or_else(|| {
            anyhow!(
                "No manifest found in safe pocket: {}",
                source.pocket_dir.display()
            )
        })?;
        manifest.hash = target.hash.clone();
        manifest.temporary = target.temporary;
        manifest.update_paths(vec![project_path], &source.pocket_dir)?;
        let _ = event::append_pocket_event(
            &source.pocket_dir,
            "heal.in_place",
            serde_json::json!({ "core_paths": manifest.core_paths }),
        );
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

    // The pocket directory name is the task prefix; migrate any tracked tasks
    // from the old name to the new one so the built-in task tracker keeps
    // working after the rename.
    match task::reprefix_global(&source.hash, &target.hash) {
        Ok(n) if n > 0 => {
            println!(
                "{} {} task(s): {} -> {}",
                "Reprefixed".bright_green(),
                n.to_string().bright_yellow(),
                source.hash.bright_yellow(),
                target.hash.bright_yellow()
            );
        }
        Ok(_) => {}
        Err(err) => {
            eprintln!("Warning: failed to reprefix tasks after heal: {err}");
        }
    }

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
    let _ = event::append_pocket_event(
        &target.pocket_dir,
        "heal.replace",
        serde_json::json!({
            "source_hash": source.hash,
            "target_hash": target.hash,
            "project": project_path,
        }),
    );

    println!(
        "{} {} -> {}",
        "Healed safe pocket:".bright_green(),
        source.hash.bright_yellow(),
        target.pocket_dir.display().to_string().bright_blue()
    );
    Ok(())
}

fn prompt_heal_pocket(project_path: &Path) -> Result<String> {
    let mut candidates = Workspace::rank_heal_candidates(project_path)?;
    if candidates.is_empty() {
        bail!("No safe pockets found to heal from. Use --pocket <id-or-path>.");
    }

    println!(
        "{}",
        "Safe pockets available for healing:".bright_white().bold()
    );
    for (index, (workspace, score)) in candidates.iter().enumerate() {
        println!(
            "  {}. {} {} {}",
            (index + 1).to_string().bright_yellow(),
            workspace.hash.bright_blue(),
            format!("score {:.2}", score).dimmed(),
            workspace.pocket_dir.display().to_string().dimmed()
        );
        for path in &workspace.core_paths {
            println!("     - {}", path.display().to_string().dimmed());
        }
    }
    println!(
        "  {}. {}",
        "0".bright_yellow(),
        "Enter an id/path manually".dimmed()
    );
    print!("{} ", "Select pocket to use [0-N]:".bright_white());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if input.is_empty() || input == "0" {
        print!("{} ", "Pocket id/path:".bright_white());
        io::stdout().flush()?;
        let mut manual = String::new();
        io::stdin().read_line(&mut manual)?;
        let manual = manual.trim();
        if manual.is_empty() {
            bail!("No pocket selected.");
        }
        return Ok(manual.to_string());
    }

    if let Ok(selection) = input.parse::<usize>() {
        if selection > 0 && selection <= candidates.len() {
            return Ok(candidates.remove(selection - 1).0.hash);
        }
    }

    bail!("Invalid heal selection: {input}")
}

fn handle_locate(path: String) -> Result<()> {
    let config = Config::load()?;
    let resolved = config.resolve_path(&path)?;
    let workspace = Workspace::find_workspace_for_cwd(&resolved)?.or_else(|| {
        Workspace::find_workspace_containing(&resolved)
            .ok()
            .flatten()
    });

    match workspace {
        Some(workspace) => {
            let out = serde_json::json!({
                "status": "found",
                "hash": workspace.hash,
                "pocket_dir": workspace.pocket_dir,
                "core_paths": workspace.core_paths,
                "temporary": workspace.temporary,
            });
            println!("{}", serde_json::to_string(&out)?);
        }
        None => {
            let out = serde_json::json!({
                "status": "not_found",
                "path": resolved,
            });
            println!("{}", serde_json::to_string(&out)?);
        }
    }

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
    let _ = event::append_registry_event(
        "backup.configure",
        serde_json::json!({ "repo": repo, "schedule": schedule, "script": script_path }),
    );
    Ok(())
}

fn handle_sync_registry_git() -> Result<()> {
    let count = registry::sync_registry_git_state()?;
    println!(
        "{} {} pocket snapshot(s)",
        "Registry git snapshot refreshed:".bright_green(),
        count.to_string().bright_yellow()
    );
    let _ = event::append_registry_event(
        "registry.snapshot.sync",
        serde_json::json!({ "pockets": count }),
    );
    Ok(())
}

fn resolve_workspace_reference(reference: &str) -> Result<Workspace> {
    let direct_path = PathBuf::from(reference);
    let config = Config::load()?;
    let resolved = if direct_path.is_absolute() && direct_path.exists() {
        direct_path
    } else {
        config.resolve_path(reference)?
    };

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

fn handle_sync(target: Option<String>, pocket: Option<String>) -> Result<()> {
    // Dispatch to system-wide sync targets when a TARGET is given.
    if let Some(target) = target.as_deref() {
        match target.to_ascii_lowercase().as_str() {
            "agents" => return handle_sync_agents(),
            "all" => return handle_sync_all(),
            other => {
                return Err(anyhow!(
                    "Unknown sync target '{other}'. Valid targets: agents, all.\n\
                     (Omit the target and pass --pocket for the manifest sync.)"
                ));
            }
        }
    }

    let pocket = pocket.ok_or_else(|| {
        anyhow!(
            "`sync` requires either a TARGET (agents, all) or --pocket <PATH> for the manifest sync."
        )
    })?;
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
        hash: workspace_hash.clone(),
        core_paths: vec![],
        sidecar_paths: vec![],
        pocket_dir: pocket_dir.clone(),
        create_readmes: false,
        temporary: pocket_dir.starts_with(Workspace::temporary_spocket_dir()?),
    };
    migration_workspace.migrate_storage_references()?;

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

    // Read current paths from workspace file. If the editor reports a transient
    // pocket-only workspace but the manifest still knows the project folders,
    // repair the workspace file instead of attempting to erase the manifest.
    let (existing_workspace, mut file_paths) =
        Workspace::read_workspace_file(&workspace_file, &pocket_dir)?;
    if file_paths.is_empty() && !manifest_paths.is_empty() {
        let repair_workspace = Workspace {
            hash: workspace_hash.clone(),
            core_paths: manifest_paths.clone(),
            sidecar_paths: vec![],
            pocket_dir: pocket_dir.clone(),
            create_readmes: false,
            temporary: manifest.temporary,
        };
        repair_workspace.write_workspace_file_preserving(Some(&existing_workspace))?;
        file_paths = manifest_paths.clone();
    }

    if file_paths.is_empty() {
        let out = serde_json::json!({
            "status": "error",
            "message": "Refusing to sync workspace with zero project folders",
            "hash": manifest.hash,
            "birth_hash": manifest.birth_hash(),
            "paths": manifest.core_paths,
        });
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

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

/// Synchronize the unified agent definitions into the pocket for the current
/// working directory (rendered OpenCode agents under `<pocket>/.opencode/agent`).
fn handle_sync_agents() -> Result<()> {
    template::ensure_default_assets()?;

    let cwd = std::env::current_dir().context("Failed to get current working directory")?;
    let workspace = Workspace::find_workspace_for_cwd(&cwd)?.ok_or_else(|| {
        anyhow!(
            "No safe pocket found for the current directory: {}\n\
             Agents are now installed per-project. Run this from inside a workspace \
             directory (or its pocket) so the agents can be written to \
             `<pocket>/.opencode/agent`.",
            cwd.display()
        )
    })?;

    let target = agents::pocket_agent_dir(&workspace.pocket_dir);
    let report = agents::sync_agents_into(&target)?;
    print_agents_sync_report(&report, &target);

    // Agents are per-project now: retire any legacy global agent files we
    // previously installed (backed up first). Best-effort.
    match agents::remove_global_agents() {
        Ok(removed) if !removed.is_empty() => {
            println!(
                "  {} retired {} legacy global agent file(s) (backed up): {}",
                "note:".bright_blue(),
                removed.len(),
                removed.join(", ").dimmed()
            );
        }
        Ok(_) => {}
        Err(e) => {
            if verbose() {
                eprintln!(
                    "{} {}",
                    "Warning: could not clean global agents:".bright_yellow(),
                    e
                );
            }
        }
    }
    Ok(())
}

/// Run every system-wide sync. Today this is just the agents.
fn handle_sync_all() -> Result<()> {
    println!(
        "{}",
        "Syncing all system-wide safe_pocket assets…"
            .bright_white()
            .bold()
    );
    handle_sync_agents()?;
    Ok(())
}

fn print_agents_sync_report(report: &agents::SyncReport, target_dir: &Path) {
    let target = target_dir.display().to_string();

    if report.created.is_empty() && report.updated.is_empty() {
        println!(
            "{} {} ({})",
            "Agents already up to date:".bright_green(),
            report.unchanged.join(", ").dimmed(),
            target.dimmed()
        );
        return;
    }

    println!("{} {}", "Synced agents →".bright_green(), target.dimmed());
    for name in &report.created {
        println!("  {} {}", "created".bright_green(), name);
    }
    for name in &report.updated {
        println!("  {} {}", "updated".bright_yellow(), name);
    }
    if !report.unchanged.is_empty() {
        println!(
            "  {} {}",
            "unchanged".dimmed(),
            report.unchanged.join(", ").dimmed()
        );
    }
    for backup in &report.backed_up {
        println!(
            "  {} backed up existing file to {}",
            "note:".bright_blue(),
            backup.dimmed()
        );
    }
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
    // Keep the unified agents in sync inside the pocket whenever it is opened.
    // Best-effort: a failure here must never block opening the workspace.
    match agents::sync_agents_into_pocket(&ws.pocket_dir) {
        Ok(report) if report.changed() => {
            if verbose() {
                print_agents_sync_report(&report, &agents::pocket_agent_dir(&ws.pocket_dir));
            }
        }
        Ok(_) => {}
        Err(e) => {
            if verbose() {
                eprintln!("{} {}", "Warning: agent sync failed:".bright_yellow(), e);
            }
        }
    }

    if let Ok(ctx) = build_template_context(&ws.pocket_dir) {
        if let Err(e) = template::apply_merge_at_runtime(&ws.pocket_dir, &ctx) {
            eprintln!(
                "{} {}",
                "Warning: merge-at-runtime failed:".bright_yellow(),
                e
            );
        }
    }

    // --silent / --simulate-runtime: every step has run (including runtime
    // content injection above) but we intentionally do not launch VS Code.
    if suppress_open() {
        run_memgraph_standard_operation(ws);
        if verbose() {
            println!(
                "{}",
                "Skipping VS Code launch (--silent/--simulate-runtime).".dimmed()
            );
        }
        return Ok(());
    }

    ws.open()?;
    run_memgraph_standard_operation(ws);
    Ok(())
}

fn run_memgraph_standard_operation(ws: &Workspace) {
    let tool_dir = ws.pocket_dir.join("tools/memgraph");
    if !tool_dir.is_dir() {
        return;
    }

    let _ = fs::write(tool_dir.join("docker-compose.yml"), memgraph_compose_yaml());

    let inputs_changed = memgraph_inputs_changed(&tool_dir, &ws.pocket_dir);
    let scanner = tool_dir.join("scan-safe-pocket.sh");
    if inputs_changed && scanner.is_file() {
        match Command::new("sh")
            .arg(&scanner)
            .current_dir(&tool_dir)
            .output()
        {
            Ok(out) if out.status.success() => {
                let _ = fs::write(
                    tool_dir.join(".safe_pocket_state.json"),
                    serde_json::json!({ "last_input_mtime_ns": memgraph_latest_input_mtime(&ws.pocket_dir) }).to_string(),
                );
            }
            Ok(out) => eprintln!(
                "{} {}",
                "Warning: Memgraph scan failed:".bright_yellow(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(err) => eprintln!(
                "{} {}",
                "Warning: Memgraph scan failed:".bright_yellow(),
                err
            ),
        }
    }

    if Command::new("docker")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
    {
        stop_legacy_memgraph_compose_project(&tool_dir);
        let output = Command::new("docker")
            .args(["compose", "-p", &memgraph_compose_project(ws), "up", "-d"])
            .current_dir(&tool_dir)
            .output();
        match output {
            Ok(out) if out.status.success() => {}
            Ok(out) => eprintln!(
                "{} {}",
                "Warning: Memgraph Docker Compose did not start:".bright_yellow(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
            Err(err) => eprintln!(
                "{} {}",
                "Warning: Memgraph Docker Compose could not run:".bright_yellow(),
                err
            ),
        }
    } else {
        eprintln!(
            "{} Docker is not available; Memgraph was not started automatically.",
            "Warning:".bright_yellow()
        );
    }
}

fn stop_legacy_memgraph_compose_project(tool_dir: &Path) {
    let _ = Command::new("docker")
        .args(["compose", "down"])
        .current_dir(tool_dir)
        .output();
}

fn stop_memgraph_standard_operation(pocket_dir: &Path) {
    let tool_dir = pocket_dir.join("tools/memgraph");
    if !tool_dir.join("docker-compose.yml").is_file() {
        return;
    }
    let hash = pocket_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("safe-pocket");
    let _ = Command::new("docker")
        .args(["compose", "-p", &format!("spocket-{hash}"), "down"])
        .current_dir(&tool_dir)
        .output();
}

fn memgraph_compose_project(ws: &Workspace) -> String {
    format!("spocket-{}", ws.hash)
}

fn memgraph_inputs_changed(tool_dir: &Path, pocket_dir: &Path) -> bool {
    let latest = memgraph_latest_input_mtime(pocket_dir);
    let state_path = tool_dir.join(".safe_pocket_state.json");
    let previous = fs::read_to_string(state_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|v| {
            v.get("last_input_mtime_ns")
                .and_then(|mtime| mtime.as_u64())
        });
    previous != Some(latest)
}

fn memgraph_latest_input_mtime(pocket_dir: &Path) -> u64 {
    let mut latest = 0;
    for path in memgraph_scan_paths_for_pocket(pocket_dir) {
        collect_latest_markdown_mtime(&path, &mut latest);
    }
    latest
}

fn memgraph_scan_paths_for_pocket(pocket_dir: &Path) -> Vec<PathBuf> {
    let mut paths = vec![pocket_dir.join("FEATURES")];
    for name in ["AGENTS.md", "GEMINI.md", "README.md", "Install.md"] {
        let candidate = pocket_dir.join(name);
        if candidate.exists() || name == "AGENTS.md" {
            paths.push(candidate);
        }
    }
    paths
}

fn collect_latest_markdown_mtime(path: &Path, latest: &mut u64) {
    if path.is_dir() {
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                collect_latest_markdown_mtime(&entry.path(), latest);
            }
        }
        return;
    }
    if path.extension().and_then(|e| e.to_str()) != Some("md") {
        return;
    }
    if let Ok(modified) = path.metadata().and_then(|m| m.modified()) {
        if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
            *latest = (*latest).max(duration.as_nanos() as u64);
        }
    }
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

    let config_root = template::safe_pocket_config_dir().unwrap_or_else(|_| {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("/"))
            .join("safe_pocket")
    });

    Ok(template::TemplateContext {
        spocket_root: pocket_dir.to_path_buf(),
        project_root,
        spocket_name,
        global_observations_path: global_obs,
        config_root,
    })
}

fn handle_merge_start(pocket: String) -> Result<()> {
    let pocket_dir = PathBuf::from(&pocket);

    if !pocket_dir.is_dir() {
        return Err(anyhow!("Pocket directory does not exist: {}", pocket));
    }

    let ctx = build_template_context(&pocket_dir)?;
    let count = template::apply_merge_at_runtime(&pocket_dir, &ctx)?;
    let _ = event::append_pocket_event(
        &pocket_dir,
        "merge.start",
        serde_json::json!({ "files_changed": count }),
    );

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
    stop_memgraph_standard_operation(&pocket_dir);
    let _ = event::append_pocket_event(
        &pocket_dir,
        "merge.stop",
        serde_json::json!({ "files_changed": count }),
    );

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

    let result = template::upgrade_pocket(&pocket_dir);
    if result.is_ok() {
        let _ = event::append_pocket_event(
            &pocket_dir,
            "pocket.upgrade",
            serde_json::json!({ "path": path }),
        );
    }
    result
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
