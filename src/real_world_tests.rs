//! Post-installation operational tests for the currently running left_pocket binary.
//!
//! The harness deliberately invokes `std::env::current_exe()` for every tested
//! operation. It does not call left_pocket's internal workspace/template functions,
//! and it does not assume a source checkout exists. The default suite runs under
//! an isolated temporary HOME. Existing-project mode performs a small set of
//! non-destructive idempotency checks after retaining a backup, then runs the
//! destructive coverage in a separate isolated HOME.

use crate::hash::hash_paths;
use crate::registry::{REAL_WORLD_TEST_BACKUPS_DIR, UPGRADE_BACKUPS_DIR};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use colored::Colorize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

const RUNTIME_START: &str = "#LEFT_POCKET_RUNTIME_CONTENT_START";
const RUNTIME_END: &str = "#LEFT_POCKET_RUNTIME_CONTENT_END";
const PLACED_EDIT: &str = "# left_pocket-real-world-placed-edit";

pub struct Options {
    pub all: bool,
    pub include: Option<PathBuf>,
    pub verbose: bool,
    pub keep: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    Skip,
}

struct Check {
    name: String,
    status: Status,
    detail: String,
}

struct Suite {
    binary: PathBuf,
    verbose: bool,
    checks: Vec<Check>,
}

impl Suite {
    fn new(binary: PathBuf, verbose: bool) -> Self {
        Self {
            binary,
            verbose,
            checks: Vec::new(),
        }
    }

    fn case<F>(&mut self, name: &str, run: F)
    where
        F: FnOnce(&mut Self) -> Result<String>,
    {
        let number = self.checks.len() + 1;
        println!();
        println!("{}", format!("[{number}] {name}").bright_white().bold());
        match run(self) {
            Ok(detail) => {
                println!("  {} {detail}", "PASS".bright_green().bold());
                self.checks.push(Check {
                    name: name.to_string(),
                    status: Status::Pass,
                    detail,
                });
            }
            Err(error) => {
                let detail = format!("{error:#}");
                println!("  {} {detail}", "FAIL".bright_red().bold());
                self.checks.push(Check {
                    name: name.to_string(),
                    status: Status::Fail,
                    detail,
                });
            }
        }
    }

    fn skip(&mut self, name: &str, detail: &str) {
        let number = self.checks.len() + 1;
        println!();
        println!("{}", format!("[{number}] {name}").bright_white().bold());
        println!("  {} {detail}", "SKIP".bright_yellow().bold());
        self.checks.push(Check {
            name: name.to_string(),
            status: Status::Skip,
            detail: detail.to_string(),
        });
    }

    fn exec(&self, env: &TestEnvironment, cwd: &Path, args: &[&str]) -> Result<Output> {
        let rendered = args
            .iter()
            .map(|arg| shell_display(arg))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "  {} {} {}",
            "RUN".bright_blue(),
            self.binary.display(),
            rendered
        );
        println!("      cwd={}", cwd.display());

        let output = Command::new(&self.binary)
            .args(args)
            .current_dir(cwd)
            .env("HOME", &env.home)
            .env("PATH", &env.path)
            .env("CODE_LOG", env.root.join("code-invocations.log"))
            .env("GIT_CONFIG_GLOBAL", env.root.join("gitconfig"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .with_context(|| format!("failed to execute {}", self.binary.display()))?;

        if self.verbose || !output.status.success() {
            print_output(&output);
        }
        if !output.status.success() {
            bail!(
                "command exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output)
    }

    fn real_exec(&self, cwd: &Path, args: &[&str]) -> Result<Output> {
        let rendered = args
            .iter()
            .map(|arg| shell_display(arg))
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "  {} {} {}",
            "RUN".bright_blue(),
            self.binary.display(),
            rendered
        );
        println!("      cwd={}", cwd.display());
        let output = Command::new(&self.binary)
            .args(args)
            .current_dir(cwd)
            .output()
            .with_context(|| format!("failed to execute {}", self.binary.display()))?;
        if self.verbose || !output.status.success() {
            print_output(&output);
        }
        if !output.status.success() {
            bail!(
                "command exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(output)
    }

    fn finish(&self) -> Result<()> {
        let passed = self
            .checks
            .iter()
            .filter(|check| check.status == Status::Pass)
            .count();
        let failed = self
            .checks
            .iter()
            .filter(|check| check.status == Status::Fail)
            .count();
        let skipped = self
            .checks
            .iter()
            .filter(|check| check.status == Status::Skip)
            .count();

        println!();
        println!("{}", "Real-world test checklist".bright_white().bold());
        for check in &self.checks {
            let label = match check.status {
                Status::Pass => "PASS".bright_green(),
                Status::Fail => "FAIL".bright_red(),
                Status::Skip => "SKIP".bright_yellow(),
            };
            println!("  [{label}] {} - {}", check.name, check.detail);
        }
        println!();
        println!(
            "{} passed, {} failed, {} skipped",
            passed.to_string().bright_green(),
            failed.to_string().bright_red(),
            skipped.to_string().bright_yellow()
        );

        if failed > 0 {
            bail!("{failed} real-world test(s) failed");
        }
        Ok(())
    }
}

struct TestEnvironment {
    root: PathBuf,
    home: PathBuf,
    path: String,
    keep: bool,
}

impl TestEnvironment {
    fn create(keep: bool) -> Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before the Unix epoch")?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("left_pocket-real-world-{}-{unique}", std::process::id()));
        let home = root.join("home");
        let bin = root.join("bin");
        fs::create_dir_all(&home)?;
        fs::create_dir_all(&bin)?;

        let code = bin.join("code");
        fs::write(
            &code,
            "#!/bin/sh\nif [ -n \"$CODE_LOG\" ]; then printf '%s\\n' \"$*\" >> \"$CODE_LOG\"; fi\nexit 0\n",
        )?;
        set_executable(&code)?;

        let old_path = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{old_path}", bin.display());
        Ok(Self {
            root,
            home,
            path,
            keep,
        })
    }

    fn project(&self, name: &str) -> Result<PathBuf> {
        let path = self.root.join("projects").join(name);
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    fn project_from_seed(&self, name: &str, seed: Option<&Path>) -> Result<PathBuf> {
        let path = self.project(name)?;
        if let Some(seed) = seed {
            copy_project_fixture(seed, &path)?;
            sanitize_fixture_env(&path.join(".env"))?;
        }
        Ok(path)
    }

    fn config_templates(&self) -> PathBuf {
        self.home.join(".config/left_pocket/templates")
    }
}

impl Drop for TestEnvironment {
    fn drop(&mut self) {
        if self.keep {
            println!(
                "{} {}",
                "Retained real-world test fixture:".bright_yellow(),
                self.root.display()
            );
        } else if self.root.exists() {
            // This is the harness-owned isolated root only. It never points at a
            // user's project, config root, or existing left_pocket.
            if let Err(error) = fs::remove_dir_all(&self.root) {
                eprintln!(
                    "Warning: could not remove test fixture {}: {error}",
                    self.root.display()
                );
            } else {
                println!(
                    "{} {}",
                    "Removed isolated test fixture:".dimmed(),
                    self.root.display()
                );
            }
        }
    }
}

pub fn run(options: Options) -> Result<()> {
    let binary = std::env::current_exe().context("failed to locate the running left_pocket binary")?;
    let mut suite = Suite::new(binary.clone(), options.verbose);

    crate::branding::print_logo();
    println!(
        "{}",
        "Installed-binary real-world tests".bright_white().bold()
    );
    println!("Binary: {}", binary.display());
    println!(
        "Coverage: {}",
        if options.all {
            "all operational checks"
        } else {
            "core operational checks (use --all for extended combinations)"
        }
    );
    println!("Safety: destructive checks run only under an isolated temporary HOME.");

    let included_project = options
        .include
        .as_deref()
        .map(Path::canonicalize)
        .transpose()
        .with_context(|| "included project does not exist")?;
    let included_left_pocket = if let Some(project) = included_project.as_deref() {
        run_existing_project_checks(&mut suite, project)?
    } else {
        None
    };

    let keep_fixture = options.keep || options.include.is_some();
    let env = TestEnvironment::create(keep_fixture)?;
    println!("Isolated HOME: {}", env.home.display());
    println!(
        "Fixture policy: {}",
        if keep_fixture {
            "retain"
        } else {
            "remove after tests"
        }
    );
    run_isolated_checks(
        &mut suite,
        &env,
        included_project.as_deref(),
        included_left_pocket.as_deref(),
        options.all,
    )?;

    suite.skip(
        "Remote backup configuration",
        "requires a real git remote and cron; intentionally excluded from an offline safety harness",
    );
    suite.skip(
        "Bulk clean commands",
        "left_pocket clean --all / --hard are intentionally never invoked by the harness",
    );
    suite.skip(
        "Interactive VS Code lifecycle",
        "runtime start/stop is tested directly; the harness uses a fake code executable and never opens the editor",
    );

    suite.finish()
}

fn run_existing_project_checks(suite: &mut Suite, project: &Path) -> Result<Option<PathBuf>> {
    println!();
    println!("{}", "Existing-project safety checks".bright_white().bold());
    println!("Project: {}", project.display());

    let templates = current_config_templates().ok();
    let templates_before = templates.as_deref().map(hash_tree).transpose()?;
    let project_before = hash_project_source(project)?;
    let locate = suite.real_exec(
        project,
        &[
            "locate",
            "--read-only",
            "--path",
            project.to_string_lossy().as_ref(),
        ],
    )?;
    let left_pocket = optional_left_pocket_from_locate(&locate)?;

    if let Some(left_pocket) = left_pocket.as_deref() {
        let backup = backup_existing_left_pocket(left_pocket, project)?;
        println!("Retained complete content backup: {}", backup.display());
        suite.case("Existing left_pocket backup", |_| {
            if !backup.join("BACKUP-MANIFEST.txt").is_file() {
                bail!("backup manifest was not written");
            }
            if hash_tree(left_pocket)? != hash_tree(&backup.join("left_pocket"))? {
                bail!("retained backup file-content hash does not match the existing left_pocket");
            }
            Ok(format!(
                "complete content backup retained at {} (file bytes and symlink targets verified)",
                backup.display()
            ))
        });
    } else {
        suite.skip(
            "Existing left_pocket backup",
            "the supplied project has no registered left_pocket; no left_pocket required backing up",
        );
    }

    suite.case("Existing project locate is read-only", |suite| {
        let output = suite.real_exec(
            project,
            &[
                "locate",
                "--read-only",
                "--path",
                project.to_string_lossy().as_ref(),
            ],
        )?;
        let located = optional_left_pocket_from_locate(&output)?;
        if located != left_pocket {
            bail!("repeated locate changed its result");
        }
        if hash_project_source(project)? != project_before {
            bail!("read-only locate changed the supplied project");
        }
        if let (Some(templates), Some(before)) = (&templates, &templates_before) {
            if hash_tree(templates)? != *before {
                bail!("read-only locate changed installed config templates");
            }
        }
        Ok(match located {
            Some(path) => format!(
                "repeated locate returned {} without changing files",
                path.display()
            ),
            None => {
                "project is unregistered; repeated locate remained not_found and changed no files"
                    .to_string()
            }
        })
    });

    suite.case("Existing .opencode ownership audit", |_| {
        let Some(left_pocket) = left_pocket.as_deref() else {
            return Ok(
                "no registered left_pocket, so there is no left_pocket-local .opencode to audit".to_string(),
            );
        };
        let opencode = left_pocket.join(".opencode");
        if !opencode.exists() {
            return Ok("no .opencode directory is present".to_string());
        }
        let total = directory_size(&opencode)?;
        let agents = directory_size(&opencode.join("agent"))?;
        let npm = directory_size(&opencode.join("node_modules"))?;
        Ok(format!(
            "{} total (left_pocket-owned agent files: {}; external OpenCode/npm node_modules: {}). Nothing removed",
            human_bytes(total),
            human_bytes(agents),
            human_bytes(npm)
        ))
    });

    suite.case("Literal template-variable artifact audit", |_| {
        let artifacts = find_literal_root_artifacts()?;
        if artifacts.is_empty() {
            Ok(
                "no active literal {{SPOCKET_CONFIG_ROOT}}/{{LEFT_POCKET_CONFIG_ROOT}} directories found (snapshots/unhoused excluded)"
                    .to_string(),
            )
        } else {
            Ok(format!(
                "found {} active artifact director{} (snapshots/unhoused excluded); report-only, nothing removed: {}",
                artifacts.len(),
                if artifacts.len() == 1 { "y" } else { "ies" },
                artifacts
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    });

    suite.skip(
        "Existing project open/upgrade/augment/heal",
        "the original is never opened or mutated; operational coverage runs against the retained isolated project copy",
    );
    Ok(left_pocket)
}

fn run_isolated_checks(
    suite: &mut Suite,
    env: &TestEnvironment,
    seed_project: Option<&Path>,
    seed_left_pocket: Option<&Path>,
    all: bool,
) -> Result<()> {
    let project = env.project_from_seed("primary", seed_project)?;
    let seeded = match seed_left_pocket {
        Some(seed_left_pocket) => Some(seed_existing_left_pocket(env, seed_left_pocket, &project)?),
        None => None,
    };
    let sidecar = env.project("augment-sidecar")?;
    let gitignore = project.join(".gitignore");
    let mut initial_gitignore = fs::read_to_string(&gitignore).unwrap_or_default();
    for line in ["custom.log", "target/", "node_modules/"] {
        if !initial_gitignore.lines().any(|existing| existing == line) {
            initial_gitignore.push_str(line);
            initial_gitignore.push('\n');
        }
    }
    fs::write(&gitignore, initial_gitignore)?;

    suite.case("Installed binary identity", |suite| {
        let output = suite.exec(env, &project, &["--version"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stdout.contains(env!("CARGO_PKG_VERSION")) {
            bail!("unexpected version output: {}", stdout.trim());
        }
        Ok(stdout.trim().to_string())
    });

    suite.case("Install default assets into isolated HOME", |suite| {
        suite.exec(env, &project, &["install-default-assets"])?;
        for required in [
            env.home.join(".config/left_pocket/templates/AGENTS.md"),
            env.home.join(".config/left_pocket/templates/project.env.md"),
            env.home.join(".config/left_pocket/directory_structure.yaml"),
        ] {
            if !required.is_file() {
                bail!("missing installed asset: {}", required.display());
            }
        }
        Ok("embedded assets installed without touching the real HOME".to_string())
    });

    suite.case("Create a normal left_pocket with left_pocket -i", |suite| {
        suite.exec(
            env,
            &project,
            &["-i", project.to_string_lossy().as_ref(), "--silent"],
        )?;
        let left_pocket = locate_left_pocket(suite, env, &project)?;
        if !left_pocket.join("manifest.json").is_file() {
            bail!("left_pocket has no manifest: {}", left_pocket.display());
        }
        Ok(format!("created {}", left_pocket.display()))
    });

    if let Some(seeded) = seeded.as_deref() {
        // Without this check the seeding step can fail silently: an orphaned
        // clone leaves `left_pocket -i` free to create a blank left_pocket, and every
        // check below still passes while testing none of the placed state the
        // seeding exists to provide.
        suite.case("Seeded existing left_pocket is adopted, not replaced", |suite| {
            let left_pocket = locate_left_pocket(suite, env, &project)?;
            if left_pocket != seeded {
                bail!(
                    "left_pocket -i created {} instead of adopting the seeded left_pocket {}",
                    left_pocket.display(),
                    seeded.display()
                );
            }
            let left_pockets = fs::read_dir(env.home.join(".left_pocket"))?
                .filter_map(Result::ok)
                .filter(|entry| entry.path().join("manifest.json").is_file())
                .count();
            if left_pockets != 1 {
                bail!("expected exactly one left_pocket in the isolated registry, found {left_pockets}");
            }
            Ok(format!(
                "the copied left_pocket was re-keyed and reused as existing placed state at {}",
                seeded.display()
            ))
        });
    }

    suite.case("Environment root placement is left_pocket-only", |suite| {
        let left_pocket = locate_left_pocket(suite, env, &project)?;
        for file in [project.join(".env"), left_pocket.join(".env")] {
            let content = fs::read_to_string(&file)
                .with_context(|| format!("failed to read {}", file.display()))?;
            for key in ["PROJECT_ROOT=", "LEFT_POCKET_ROOT="] {
                if !content.lines().any(|line| line.starts_with(key)) {
                    bail!("{} is missing {key}", file.display());
                }
            }
            // The Spocket/Safe_pocket rename is complete: left_pocket must no longer
            // emit the legacy root key. Compatibility is read-only.
            if content.lines().any(|line| line.starts_with("SPOCKET_ROOT=")) {
                bail!(
                    "{} still writes the legacy SPOCKET_ROOT key",
                    file.display()
                );
            }
        }
        Ok("project and left_pocket .env define PROJECT_ROOT and LEFT_POCKET_ROOT, with no legacy SPOCKET_ROOT".to_string())
    });

    suite.case(
        "Quiet merge preserves existing files and deduplicates",
        |suite| {
            let content = fs::read_to_string(project.join(".gitignore"))?;
            for expected in ["custom.log", "target/", "node_modules/", ".env"] {
                if !content.lines().any(|line| line == expected) {
                    bail!("quiet merge lost or failed to add {expected}");
                }
            }
            if content.lines().filter(|line| *line == ".env").count() != 1 {
                bail!(".env was duplicated in .gitignore");
            }
            suite.exec(
                env,
                &project,
                &["-i", project.to_string_lossy().as_ref(), "--silent"],
            )?;
            let reopened = fs::read_to_string(project.join(".gitignore"))?;
            if reopened != content {
                bail!("idempotent reopen changed .gitignore");
            }
            Ok("custom lines survived and .env was placed exactly once".to_string())
        },
    );

    suite.case("Placed edits never reverse-sync", |suite| {
        let config_template = env.config_templates().join("gitignore.md");
        let source_before = fs::read_to_string(&config_template)?;
        let mut placed = fs::read_to_string(project.join(".gitignore"))?;
        placed.push_str(PLACED_EDIT);
        placed.push('\n');
        fs::write(project.join(".gitignore"), &placed)?;

        suite.exec(
            env,
            &project,
            &["-i", project.to_string_lossy().as_ref(), "--silent"],
        )?;
        let placed_after = fs::read_to_string(project.join(".gitignore"))?;
        let source_after = fs::read_to_string(&config_template)?;
        if !placed_after.contains(PLACED_EDIT) {
            bail!("placed edit was lost on reopen");
        }
        if source_after != source_before || source_after.contains(PLACED_EDIT) {
            bail!("placed edit reverse-synced into config templates");
        }
        Ok("placed edit survived reopen and config template remained byte-identical".to_string())
    });

    suite.case("Runtime merge start/stop round trip", |suite| {
        let left_pocket = locate_left_pocket(suite, env, &project)?;
        let agents = left_pocket.join("AGENTS.md");
        let existing = fs::read_to_string(&agents)?;
        let outside = "user content outside runtime block\n";
        fs::write(&agents, format!("{outside}{existing}"))?;

        suite.exec(
            env,
            &project,
            &[
                "runtime-merge-stop",
                "--left_pocket",
                left_pocket.to_string_lossy().as_ref(),
            ],
        )?;
        let stopped = fs::read_to_string(&agents)?;
        if stopped.contains(RUNTIME_START) || stopped.contains(RUNTIME_END) {
            bail!("runtime markers remained after stop");
        }
        if !stopped.contains(outside.trim()) {
            bail!("user content outside markers was lost on stop");
        }

        suite.exec(
            env,
            &project,
            &[
                "runtime-merge-start",
                "--left_pocket",
                left_pocket.to_string_lossy().as_ref(),
            ],
        )?;
        let started = fs::read_to_string(&agents)?;
        if !started.contains(RUNTIME_START) || !started.contains(RUNTIME_END) {
            bail!("LOCKET runtime markers were not injected on start");
        }
        if !started.contains(outside.trim()) {
            bail!("user content outside markers was lost on start");
        }
        Ok("runtime content was stripped/reinjected while outside content survived".to_string())
    });

    suite.case("left_pocket -u upgrade semantics", |suite| {
        let left_pocket = locate_left_pocket(suite, env, &project)?;
        let prompt = left_pocket.join(".github/prompts/TalkLikeACat.md");
        fs::write(&prompt, "user-edited non-quiet prompt\n")?;
        let gitignore_before = fs::read_to_string(project.join(".gitignore"))?;

        suite.exec(env, &project, &["-u", project.to_string_lossy().as_ref()])?;
        let prompt_after = fs::read_to_string(&prompt)?;
        if prompt_after.contains("user-edited non-quiet prompt") {
            bail!("non-quiet placed template was not upgraded");
        }
        let gitignore_after = fs::read_to_string(project.join(".gitignore"))?;
        if gitignore_after != gitignore_before || !gitignore_after.contains(PLACED_EDIT) {
            bail!("upgrade destroyed quiet-merge .gitignore content");
        }
        Ok("non-quiet template upgraded; quiet-merge user content preserved".to_string())
    });

    if all {
        suite.case("Augment add/remove and idempotency", |suite| {
            suite.exec(
                env,
                &project,
                &[
                    "augment",
                    "--add",
                    sidecar.to_string_lossy().as_ref(),
                    "--no-open",
                ],
            )?;
            let left_pocket = locate_left_pocket(suite, env, &project)?;
            if !manifest_contains_path(&left_pocket, &sidecar)? {
                bail!("augmented path missing from manifest");
            }
            let duplicate = suite.exec(
                env,
                &project,
                &[
                    "augment",
                    "--add",
                    sidecar.to_string_lossy().as_ref(),
                    "--no-open",
                ],
            )?;
            if !String::from_utf8_lossy(&duplicate.stdout).contains("No changes") {
                bail!("duplicate augment did not explicitly report no changes");
            }
            suite.exec(
                env,
                &project,
                &[
                    "augment",
                    "--remove",
                    sidecar.to_string_lossy().as_ref(),
                    "--no-open",
                ],
            )?;
            if manifest_contains_path(&left_pocket, &sidecar)? {
                bail!("removed path remains in manifest");
            }
            let duplicate_remove = suite.exec(
                env,
                &project,
                &[
                    "augment",
                    "--remove",
                    sidecar.to_string_lossy().as_ref(),
                    "--no-open",
                ],
            )?;
            if !String::from_utf8_lossy(&duplicate_remove.stdout).contains("No changes") {
                bail!("duplicate remove did not explicitly report no changes");
            }
            Ok(
                "add/remove updated in place; duplicate add and remove reported no changes"
                    .to_string(),
            )
        });

        suite.case("Alias register/list/unregister", |suite| {
            let alias = format!("rw={}", project.display());
            suite.exec(env, &project, &["register", &alias])?;
            let listed = suite.exec(env, &project, &["list-aliases"])?;
            if !String::from_utf8_lossy(&listed.stdout).contains("rw") {
                bail!("registered alias was not listed");
            }
            suite.exec(env, &project, &["unregister", "rw"])?;
            let after = suite.exec(env, &project, &["list-aliases"])?;
            if String::from_utf8_lossy(&after.stdout).contains("rw ->") {
                bail!("unregistered alias is still listed");
            }
            Ok("alias lifecycle completed".to_string())
        });

        suite.case("Heal moves a left_pocket to a new project", |suite| {
            let source = env.project("heal-source")?;
            let target = env.project("heal-target")?;
            suite.exec(
                env,
                &source,
                &["-i", source.to_string_lossy().as_ref(), "--silent"],
            )?;
            let source_left_pocket = locate_left_pocket(suite, env, &source)?;
            suite.exec(
                env,
                &target,
                &[
                    "heal",
                    "--project",
                    target.to_string_lossy().as_ref(),
                    "--left_pocket",
                    source_left_pocket.to_string_lossy().as_ref(),
                ],
            )?;
            let healed = locate_left_pocket(suite, env, &target)?;
            if !manifest_contains_path(&healed, &target)? {
                bail!("healed left_pocket does not reference target project");
            }
            let old = suite.exec(
                env,
                &source,
                &["locate", "--path", source.to_string_lossy().as_ref()],
            )?;
            let json: Value = serde_json::from_slice(&old.stdout)?;
            if json.get("status").and_then(Value::as_str) != Some("not_found") {
                bail!("source project still resolves after heal");
            }
            Ok(format!(
                "healed left_pocket now resolves at {}",
                healed.display()
            ))
        });

        suite.case("Per-left_pocket OpenCode agent placement", |suite| {
            let left_pocket = locate_left_pocket(suite, env, &project)?;
            suite.exec(env, &project, &["sync", "agents"])?;
            let agent_dir = left_pocket.join(".opencode/agent");
            let count = fs::read_dir(&agent_dir)?.filter_map(Result::ok).count();
            if count == 0 {
                bail!("no OpenCode agents were rendered");
            }
            if left_pocket.join(".opencode/node_modules").exists() {
                bail!("left_pocket unexpectedly installed node_modules into .opencode");
            }
            Ok(format!(
                "rendered {count} agent files; no node_modules were installed by left_pocket"
            ))
        });

        suite.case("No literal config-root directories are created", |_| {
            let mut bad = Vec::new();
            collect_named_dirs(
                &env.home.join(".left_pocket"),
                &["{{SPOCKET_CONFIG_ROOT}}", "{{LEFT_POCKET_CONFIG_ROOT}}"],
                &mut bad,
            )?;
            if !bad.is_empty() {
                bail!(
                    "literal template-variable directories were created: {}",
                    bad.iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Ok(
                "no unresolved config-root directory names exist in the isolated registry"
                    .to_string(),
            )
        });

        suite.case("Upgrade-installation is idempotent", |suite| {
            let output = suite.exec(env, &project, &["upgrade-installation", "--dry-run"])?;
            if !String::from_utf8_lossy(&output.stdout).contains("nothing to upgrade") {
                bail!("fresh installation unexpectedly contains legacy structural tokens");
            }
            Ok("fresh installation contains no migratable legacy structural tokens".to_string())
        });
    } else {
        suite.skip(
            "Extended --all operational combinations",
            "augment, alias, heal, OpenCode sync, artifact and migration checks require --all",
        );
    }

    Ok(())
}

fn locate_left_pocket(suite: &Suite, env: &TestEnvironment, project: &Path) -> Result<PathBuf> {
    let output = suite.exec(
        env,
        project,
        &["locate", "--path", project.to_string_lossy().as_ref()],
    )?;
    left_pocket_from_locate(&output)
}

fn left_pocket_from_locate(output: &Output) -> Result<PathBuf> {
    optional_left_pocket_from_locate(output)?.ok_or_else(|| anyhow!("left_pocket locate returned not_found"))
}

fn optional_left_pocket_from_locate(output: &Output) -> Result<Option<PathBuf>> {
    let json: Value = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "invalid locate JSON: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })?;
    match json.get("status").and_then(Value::as_str) {
        Some("not_found") => return Ok(None),
        Some("found") => {}
        _ => bail!("left_pocket locate returned {}", json),
    }
    let path = json
        .get("left_pocket_dir")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("locate output has no left_pocket_dir"))?;
    Ok(Some(PathBuf::from(path)))
}

fn manifest_contains_path(left_pocket: &Path, expected: &Path) -> Result<bool> {
    let manifest: Value = serde_json::from_str(&fs::read_to_string(left_pocket.join("manifest.json"))?)?;
    let expected = expected
        .canonicalize()
        .unwrap_or_else(|_| expected.to_path_buf());
    Ok(manifest
        .get("core_paths")
        .and_then(Value::as_array)
        .map(|paths| {
            paths
                .iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .map(|path| path.canonicalize().unwrap_or(path))
                .any(|path| path == expected)
        })
        .unwrap_or(false))
}

/// Copy an existing left_pocket into the isolated registry and re-key it to the
/// fixture project.
///
/// A left_pocket is addressed by `hash_paths(core_paths)`: the directory name, the
/// `.code-workspace` filename and `manifest.hash` all have to agree, and
/// `Workspace::find_workspace_by_manifest_paths` only adopts a left_pocket whose
/// *on-disk* `manifest.hash` equals the hash of the paths being looked up.
/// Rewriting `core_paths` alone therefore produces an orphan that no lookup
/// ever resolves: `left_pocket -i` would quietly create a second, blank left_pocket and
/// every idempotency check below would run against empty state while still
/// reporting PASS. Re-keying is what makes the seeded left_pocket real.
fn seed_existing_left_pocket(
    env: &TestEnvironment,
    source_left_pocket: &Path,
    fixture_project: &Path,
) -> Result<PathBuf> {
    let project = fixture_project
        .canonicalize()
        .unwrap_or_else(|_| fixture_project.to_path_buf());
    let hash = hash_paths(std::slice::from_ref(&project));
    let destination = env.home.join(".left_pocket").join(&hash);
    copy_isolated_left_pocket_clone(source_left_pocket, &destination)?;

    let manifest_path = destination.join("manifest.json");
    let mut manifest: Value =
        serde_json::from_str(&fs::read_to_string(&manifest_path).with_context(|| {
            format!(
                "seeded left_pocket has no readable manifest: {}",
                manifest_path.display()
            )
        })?)?;
    manifest["hash"] = serde_json::json!(hash);
    manifest["core_paths"] = serde_json::json!([project]);
    manifest["worktrees"] = serde_json::json!([]);
    manifest["temporary"] = serde_json::json!(false);
    manifest["children"] = serde_json::json!([]);
    // Lineage fields reference hashes that do not exist inside the isolated
    // registry. Carrying them over leaves lookups resolving to nothing, and a
    // stale `birth_hash` disagrees with the directory the clone now lives in.
    if let Some(object) = manifest.as_object_mut() {
        for stale in ["parent_hash", "augmented_from", "birth_hash"] {
            object.remove(stale);
        }
    }
    fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;

    // The workspace file is looked up as `<dirname>.code-workspace` first, so
    // the copy has to be renamed rather than left under the source hash. The
    // folder label uses the real branding helper: seeding state that differs
    // from what `left_pocket -i` would itself write would make the very first
    // reopen look like a change and defeat the idempotency checks.
    for entry in fs::read_dir(&destination)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("code-workspace") {
            continue;
        }
        let mut workspace: Value = serde_json::from_str(&fs::read_to_string(&path)?)?;
        workspace["folders"] = serde_json::json!([{
            "path": project,
            "name": crate::branding::workspace_folder_name(&hash)
        }]);
        let renamed = destination.join(format!("{hash}.code-workspace"));
        fs::write(&renamed, serde_json::to_string_pretty(&workspace)?)?;
        if path != renamed {
            fs::remove_file(&path)?;
        }
    }

    let roots = format!(
        "PROJECT_ROOT={}\nLEFT_POCKET_ROOT={}\n",
        project.display(),
        destination.display()
    );
    fs::write(destination.join(".env"), &roots)?;

    // A genuinely pre-existing project already has correct roots in its own
    // `.env`. `sanitize_fixture_env` stripped the copied roots so the fixture
    // could never point at the real left_pocket; restore them against the *fixture*
    // paths here. Without this the adopted-left_pocket path looks like a missing
    // placement, because `left_pocket -i` deliberately does not re-place templates
    // for a left_pocket that already exists.
    let project_env = project.join(".env");
    let mut merged = fs::read_to_string(&project_env).unwrap_or_default();
    if !merged.is_empty() && !merged.ends_with('\n') {
        merged.push('\n');
    }
    merged.push_str(&roots);
    fs::write(&project_env, merged)?;

    // Never execute copied project-specific Memgraph scripts or Docker actions.
    // The clone remains available for inspection under a clearly disabled name.
    let memgraph = destination.join("tools/memgraph");
    if memgraph.is_dir() {
        fs::rename(
            &memgraph,
            destination.join("tools/memgraph.real-world-disabled"),
        )?;
    }
    Ok(destination)
}

fn backup_existing_left_pocket(left_pocket: &Path, project: &Path) -> Result<PathBuf> {
    let parent = left_pocket
        .parent()
        .ok_or_else(|| anyhow!("left_pocket has no registry parent: {}", left_pocket.display()))?;
    let registry = if parent.file_name().and_then(|name| name.to_str()) == Some("temporary") {
        parent.parent().unwrap_or(parent)
    } else {
        parent
    };
    let hash = left_pocket
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown-left_pocket");
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
    let backup = registry
        .join(REAL_WORLD_TEST_BACKUPS_DIR)
        .join(format!("{hash}-{stamp}"));
    fs::create_dir_all(&backup)?;
    copy_tree_complete(left_pocket, &backup.join("left_pocket"))?;

    let project_files = backup.join("project-files");
    fs::create_dir_all(&project_files)?;
    for name in [".env", ".gitignore"] {
        let source = project.join(name);
        if source.is_file() {
            fs::copy(&source, project_files.join(name))?;
        }
    }
    fs::write(
        backup.join("BACKUP-MANIFEST.txt"),
        format!(
            "Created: {}\nleft_pocket: {}\nProject: {}\nBackup policy: complete left_pocket copy; no left_pocket content excluded\n",
            Utc::now().to_rfc3339(),
            left_pocket.display(),
            project.display()
        ),
    )?;
    Ok(backup)
}

fn copy_tree_complete(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let destination = target.join(&name);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            copy_symlink(&path, &destination)?;
        } else if metadata.is_dir() {
            copy_tree_complete(&path, &destination)?;
        } else if metadata.is_file() {
            fs::copy(&path, &destination)?;
        }
    }
    Ok(())
}

fn copy_project_fixture(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if matches!(
            name.to_string_lossy().as_ref(),
            ".git" | "node_modules" | "target" | "graphify-out"
        ) {
            continue;
        }
        let destination = target.join(&name);
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            // Never preserve project symlinks in an operational fixture: an
            // absolute or escaping relative link could route .env/.gitignore or
            // child command writes back into the original tree.
            continue;
        } else if metadata.is_dir() {
            fs::create_dir_all(&destination)?;
            copy_project_fixture(&path, &destination)?;
        } else if metadata.is_file() {
            fs::copy(&path, &destination)?;
        }
    }
    Ok(())
}

/// Directory names never copied into the throwaway isolated left_pocket clone.
///
/// `node_modules` is external OpenCode/npm content that left_pocket does not own or
/// regenerate and that routinely runs to tens of megabytes, so copying it into
/// a fixture on every `left_pocket tests -i` run costs a great deal and proves
/// nothing. The retained backup is a separate, deliberately complete copy, so
/// no content is lost by leaving these out here.
const ISOLATED_CLONE_SKIP_DIRS: &[&str] = &["node_modules", "target", ".git"];

/// Copy a left_pocket into the isolated fixture, dropping symlinks so no write can
/// escape back into the original tree.
fn copy_isolated_left_pocket_clone(source: &Path, target: &Path) -> Result<()> {
    if !source.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let destination = target.join(&name);
        if metadata.is_dir() {
            if ISOLATED_CLONE_SKIP_DIRS.contains(&name.to_string_lossy().as_ref()) {
                continue;
            }
            copy_isolated_left_pocket_clone(&path, &destination)?;
        } else if metadata.is_file() {
            fs::copy(&path, &destination)?;
        }
    }
    Ok(())
}

fn sanitize_fixture_env(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let existing = fs::read_to_string(path)?;
    let kept = existing
        .lines()
        .filter(|line| {
            !line.starts_with("PROJECT_ROOT=")
                && !line.starts_with("LEFT_POCKET_ROOT=")
                && !line.starts_with("SPOCKET_ROOT=")
        })
        .collect::<Vec<_>>();
    let content = if kept.is_empty() {
        String::new()
    } else {
        format!("{}\n", kept.join("\n"))
    };
    fs::write(path, content)?;
    Ok(())
}

fn copy_symlink(source: &Path, target: &Path) -> Result<()> {
    let link = fs::read_link(source)?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(link, target)?;
    #[cfg(not(unix))]
    {
        let resolved = source.canonicalize()?;
        if resolved.is_dir() {
            copy_tree_complete(&resolved, target)?;
        } else {
            fs::copy(resolved, target)?;
        }
    }
    Ok(())
}

fn current_config_templates() -> Result<PathBuf> {
    let home = dirs::home_dir().context("HOME is unavailable")?;
    for candidate in [
        home.join(".config/left_pocket/templates"),
        home.join(".config/safe_pocket/templates"),
        home.join(".config/spocket/templates"),
    ] {
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    bail!("no installed config templates directory found")
}

fn hash_tree(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        hasher.update(relative.to_string_lossy().as_bytes());
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            hasher.update(b"symlink:");
            hasher.update(fs::read_link(path)?.to_string_lossy().as_bytes());
        } else {
            hasher.update(fs::read(path)?);
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

fn hash_project_source(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_project_files(root, root, &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hasher = Sha256::new();
    for (relative, path) in files {
        hasher.update(relative.to_string_lossy().as_bytes());
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            hasher.update(b"symlink:");
            hasher.update(fs::read_link(path)?.to_string_lossy().as_bytes());
        } else {
            hasher.update(fs::read(path)?);
        }
    }
    Ok(hex::encode(hasher.finalize()))
}

fn collect_project_files(
    root: &Path,
    current: &Path,
    out: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<()> {
    if !current.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        if matches!(
            name.to_string_lossy().as_ref(),
            ".git" | "node_modules" | "target" | "graphify-out"
        ) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            out.push((path.strip_prefix(root)?.to_path_buf(), path));
        } else if metadata.is_dir() {
            collect_project_files(root, &path, out)?;
        } else if metadata.is_file() {
            out.push((path.strip_prefix(root)?.to_path_buf(), path));
        }
    }
    Ok(())
}

fn collect_files(root: &Path, current: &Path, out: &mut Vec<(PathBuf, PathBuf)>) -> Result<()> {
    if !current.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            out.push((path.strip_prefix(root)?.to_path_buf(), path));
        } else if metadata.is_dir() {
            collect_files(root, &path, out)?;
        } else if metadata.is_file() {
            out.push((path.strip_prefix(root)?.to_path_buf(), path));
        }
    }
    Ok(())
}

fn find_literal_root_artifacts() -> Result<Vec<PathBuf>> {
    let home = dirs::home_dir().context("HOME is unavailable")?;
    let mut found = Vec::new();
    for registry in [
        home.join(".left_pocket"),
        home.join(".safe_pocket"),
        home.join(".spocket"),
    ] {
        collect_named_dirs(
            &registry,
            &["{{SPOCKET_CONFIG_ROOT}}", "{{LEFT_POCKET_CONFIG_ROOT}}"],
            &mut found,
        )?;
    }
    found.sort();
    found.dedup();
    Ok(found)
}

fn collect_named_dirs(root: &Path, names: &[&str], out: &mut Vec<PathBuf>) -> Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        } {
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name();
            if names.iter().any(|candidate| name == *candidate) {
                out.push(path);
                continue;
            }
            let name = name.to_string_lossy();
            // The two retained backup trees hold verbatim copies of left_pocket
            // content, artifact directories included. Descending into them
            // would re-report every copy, so the reported count would climb on
            // each test run and backup copies would be offered up for cleanup.
            if matches!(
                name.as_ref(),
                ".git" | "node_modules" | "target" | "snapshots" | "unhoused"
            ) || name == UPGRADE_BACKUPS_DIR
                || name == REAL_WORLD_TEST_BACKUPS_DIR
            {
                continue;
            }
            stack.push(path);
        }
    }
    Ok(())
}

fn directory_size(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    if path.is_file() {
        return Ok(fs::metadata(path)?.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        total += directory_size(&entry.path())?;
    }
    Ok(total)
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let value = bytes as f64;
    if value >= GB {
        format!("{:.1} GiB", value / GB)
    } else if value >= MB {
        format!("{:.1} MiB", value / MB)
    } else if value >= KB {
        format!("{:.1} KiB", value / KB)
    } else {
        format!("{bytes} B")
    }
}

fn print_output(output: &Output) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stdout.trim().is_empty() {
        for line in stdout.lines() {
            println!("      stdout: {line}");
        }
    }
    if !stderr.trim().is_empty() {
        for line in stderr.lines() {
            println!("      stderr: {line}");
        }
    }
}

fn shell_display(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || "-_=./:".contains(character))
    {
        value.to_string()
    } else {
        format!("{:?}", value)
    }
}

fn set_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_formats_units() {
        assert_eq!(human_bytes(12), "12 B");
        assert_eq!(human_bytes(2048), "2.0 KiB");
        assert_eq!(human_bytes(2 * 1024 * 1024), "2.0 MiB");
    }

    #[test]
    fn named_directory_scan_finds_literal_roots_and_skips_snapshots() {
        let root =
            std::env::temp_dir().join(format!("left_pocket-real-world-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("abc/{{SPOCKET_CONFIG_ROOT}}")).unwrap();
        fs::create_dir_all(root.join("snapshots/{{SPOCKET_CONFIG_ROOT}}")).unwrap();
        let mut found = Vec::new();
        collect_named_dirs(
            &root,
            &["{{SPOCKET_CONFIG_ROOT}}", "{{LEFT_POCKET_CONFIG_ROOT}}"],
            &mut found,
        )
        .unwrap();
        assert_eq!(found, vec![root.join("abc/{{SPOCKET_CONFIG_ROOT}}")]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tree_hash_changes_with_content() {
        let root =
            std::env::temp_dir().join(format!("left_pocket-real-world-hash-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("a.txt"), "one").unwrap();
        let before = hash_tree(&root).unwrap();
        fs::write(root.join("a.txt"), "two").unwrap();
        let after = hash_tree(&root).unwrap();
        assert_ne!(before, after);
        let _ = fs::remove_dir_all(root);
    }
}
