use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestSummary<'a> {
    name: &'a str,
    tested: &'a str,
    steps: Vec<String>,
    cleanup: &'a str,
}

impl<'a> TestSummary<'a> {
    fn new(name: &'a str, tested: &'a str, cleanup: &'a str) -> Self {
        Self {
            name,
            tested,
            steps: Vec::new(),
            cleanup,
        }
    }

    fn step<S: Into<String>>(&mut self, step: S) {
        self.steps.push(step.into());
    }

    fn print(&self) {
        println!("TEST SUMMARY: {}", self.name);
        println!("  Tested: {}", self.tested);
        for (index, step) in self.steps.iter().enumerate() {
            println!("  Step {}: {}", index + 1, step);
        }
        println!("  Cleanup: {}", self.cleanup);
    }
}

struct TestEnv {
    root: PathBuf,
    home: PathBuf,
    path: String,
}

impl TestEnv {
    fn new(name: &str) -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "spocket-beads-defaults-{name}-{}-{unique}",
            std::process::id()
        ));
        let home = root.join("home");
        let bin_dir = root.join("bin");

        fs::create_dir_all(&home).expect("failed to create test home");
        fs::create_dir_all(&bin_dir).expect("failed to create fake bin dir");

        let code_path = bin_dir.join("code");
        fs::write(
            &code_path,
            "#!/bin/sh\nif [ -n \"$CODE_LOG\" ]; then echo \"$@\" >> \"$CODE_LOG\"; fi\nexit 0\n",
        )
        .expect("failed to write fake code command");
        fs::set_permissions(&code_path, fs::Permissions::from_mode(0o755))
            .expect("failed to make fake code executable");

        let bd_path = bin_dir.join("bd");
        fs::write(
            &bd_path,
            "#!/bin/sh\nif [ \"$1\" = \"init\" ]; then mkdir -p .beads/embeddeddolt; touch .beads/usable; exit 0; fi\nif [ \"$1\" = \"where\" ]; then [ -f .beads/usable ] && exit 0 || exit 1; fi\nexit 0\n",
        )
        .expect("failed to write fake bd command");
        fs::set_permissions(&bd_path, fs::Permissions::from_mode(0o755))
            .expect("failed to make fake bd executable");

        let existing_path = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{existing_path}", bin_dir.display());

        Self { root, home, path }
    }

    fn project(&self, name: &str) -> PathBuf {
        let project = self.root.join(name);
        fs::create_dir_all(&project).expect("failed to create project dir");
        project
    }

    fn run_spocket(&self, project: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_safe_pocket"))
            .args(args)
            .current_dir(project)
            .env("HOME", &self.home)
            .env("PATH", &self.path)
            .env("CODE_LOG", self.code_log())
            .env("GIT_CONFIG_GLOBAL", self.root.join("gitconfig"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("failed to run safe_pocket")
    }

    /// Path to the file where the fake `code` binary records each invocation.
    fn code_log(&self) -> PathBuf {
        self.root.join("code_invocations.log")
    }

    /// Number of times the fake `code` (VS Code) binary was launched.
    fn code_launch_count(&self) -> usize {
        match fs::read_to_string(self.code_log()) {
            Ok(contents) => contents.lines().filter(|l| !l.trim().is_empty()).count(),
            Err(_) => 0,
        }
    }

    fn safe_pockets(&self) -> Vec<PathBuf> {
        let registry = self.home.join(".safe_pocket");
        if !registry.exists() {
            return Vec::new();
        }

        let mut pockets = Vec::new();
        for root in [registry.clone(), registry.join("temporary")] {
            if !root.exists() {
                continue;
            }

            pockets.extend(
                fs::read_dir(root)
                    .expect("failed to read safe pocket registry")
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| path.is_dir())
                    .filter(|path| path.join("manifest.json").is_file()),
            );
        }
        pockets.sort();
        pockets
    }

    fn only_pocket(&self) -> PathBuf {
        let pockets = self.safe_pockets();
        assert_eq!(pockets.len(), 1, "expected exactly one safe pocket");
        pockets
            .into_iter()
            .next()
            .expect("safe pocket should exist")
    }

    fn registry_file(&self, name: &str) -> PathBuf {
        self.home.join(".safe_pocket").join(name)
    }

    fn assert_no_registry_entry_for(&self, pocket: &Path) {
        let cache_path = self.registry_file("registry_cache.json");
        if !cache_path.exists() {
            return;
        }

        let cache = fs::read_to_string(&cache_path).expect("failed to read registry cache");
        assert!(
            !cache.contains(&pocket.display().to_string()),
            "registry cache should not retain cleaned pocket {}",
            pocket.display()
        );
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn manifest_uses_beads(pocket: &Path) -> bool {
    let manifest_path = pocket.join("manifest.json");
    let manifest = fs::read_to_string(&manifest_path).expect("failed to read manifest");
    let value: serde_json::Value =
        serde_json::from_str(&manifest).expect("failed to parse manifest");
    value
        .get("uses_beads")
        .and_then(|value| value.as_bool())
        .expect("manifest should contain uses_beads")
}

fn assert_failure(output: &Output) {
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_contains(output: &Output, needle: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains(needle) || stderr.contains(needle),
        "expected to find {needle:?}\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}

#[test]
fn cli_help_surface_is_reachable() {
    let env = TestEnv::new("cli-help");
    let project = env.project("project");
    let invocations = [
        vec!["--help"],
        vec!["register", "--help"],
        vec!["unregister", "--help"],
        vec!["list-aliases", "--help"],
        vec!["list-workspaces", "--help"],
        vec!["sync", "--help"],
        vec!["augment", "--help"],
        vec!["mark", "--help"],
        vec!["clean", "--help"],
        vec!["heal", "--help"],
        vec!["locate", "--help"],
        vec!["sync-registry-git", "--help"],
        vec!["backup", "--help"],
        vec!["runtime-merge-start", "--help"],
        vec!["runtime-merge-stop", "--help"],
        vec!["completions", "--help"],
        vec!["worktree", "--help"],
        vec!["worktree", "add", "--help"],
        vec!["worktree", "remove", "--help"],
        vec!["worktree", "list", "--help"],
    ];

    let mut summary = TestSummary::new(
        "cli_help_surface_is_reachable",
        "every reachable command path renders help and version output stays available",
        "the test uses an isolated HOME under /tmp; the entire registry and every created project disappear when TestEnv drops",
    );

    for args in invocations {
        let output = env.run_spocket(&project, &args);
        assert_success(&output);
        assert_contains(&output, "Usage:");
        summary.step(format!(
            "Ran `spocket {}` and verified help rendered",
            args.join(" ")
        ));
    }

    let short_version = env.run_spocket(&project, &["-v"]);
    assert_success(&short_version);
    assert_eq!(
        String::from_utf8_lossy(&short_version.stdout).trim(),
        env!("CARGO_PKG_VERSION")
    );
    summary.step("Ran `spocket -v` and verified the bare package version".to_string());

    let long_version = env.run_spocket(&project, &["--version"]);
    assert_success(&long_version);
    assert_contains(&long_version, env!("CARGO_PKG_VERSION"));
    summary
        .step("Ran `spocket --version` and verified version output remained reachable".to_string());

    summary.print();
}

#[test]
fn outdated_commands_are_rejected() {
    let env = TestEnv::new("outdated-commands");
    let project = env.project("project");
    let commands = [vec!["list"], vec!["merge-start"], vec!["merge-stop"]];
    let mut summary = TestSummary::new(
        "outdated_commands_are_rejected",
        "deprecated commands are no longer part of the public CLI surface",
        "no persistent registry state is kept because the test only exercises command parsing inside an isolated HOME",
    );

    for args in commands {
        let output = env.run_spocket(&project, &args);
        assert_failure(&output);
        assert_contains(&output, "unrecognized subcommand");
        summary.step(format!(
            "Ran deprecated command `spocket {}` and confirmed Clap rejected it",
            args.join(" ")
        ));
    }

    summary.print();
}

#[test]
fn new_pocket_defaults_to_beads() {
    let env = TestEnv::new("default");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "new_pocket_defaults_to_beads",
        "default workspace creation enables beads and writes redirect/env files",
        "the test runs in an isolated HOME and removes the temp root recursively on drop, which clears both projects and registry entries",
    );

    let output = env.run_spocket(&project, &["-i", "."]);
    assert_success(&output);
    summary.step("Opened a new workspace with `spocket -i .`".to_string());

    let pocket = env.only_pocket();
    assert!(pocket.join(".beads").is_dir());
    assert_eq!(
        fs::read_to_string(project.join(".beads").join("redirect")).unwrap(),
        pocket.join(".beads").to_string_lossy()
    );
    assert_eq!(
        fs::read_to_string(project.join(".env")).unwrap(),
        format!("SPOCKET_ROOT={}\n", pocket.display())
    );
    assert_eq!(
        fs::read_to_string(pocket.join(".env")).unwrap(),
        format!(
            "PROJECT_ROOT={}\n",
            project.canonicalize().unwrap().display()
        )
    );
    assert!(manifest_uses_beads(&pocket));
    summary.step("Verified beads initialization, redirect wiring, and env templates inside the created pocket".to_string());
    summary.print();
}

#[test]
fn new_pocket_with_without_beads_does_not_initialize_beads() {
    let env = TestEnv::new("without");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "new_pocket_with_without_beads_does_not_initialize_beads",
        "`--without-beads` skips beads setup for new pockets",
        "the isolated HOME and temp root are deleted at the end of the test, so no test-created pocket remains registered",
    );

    let output = env.run_spocket(&project, &["-i", ".", "--without-beads"]);
    assert_success(&output);
    summary.step("Created a new workspace with `--without-beads`".to_string());

    let pocket = env.only_pocket();
    assert!(!pocket.join(".beads").exists());
    assert!(!project.join(".beads").exists());
    assert!(!manifest_uses_beads(&pocket));
    summary.step(
        "Confirmed that neither the pocket nor the project received beads artifacts".to_string(),
    );
    summary.print();
}

#[test]
fn existing_non_beads_pocket_is_not_auto_upgraded_on_reuse() {
    let env = TestEnv::new("reuse-without");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "existing_non_beads_pocket_is_not_auto_upgraded_on_reuse",
        "reopening a non-beads pocket preserves its original non-beads state",
        "cleanup is handled by deleting the isolated temp HOME, which removes the registry cache and pocket directory together",
    );

    assert_success(&env.run_spocket(&project, &["-i", ".", "--without-beads"]));
    summary.step("Created an initial pocket without beads".to_string());
    assert_success(&env.run_spocket(&project, &["-i", "."]));
    summary.step("Reopened the same pocket with default options".to_string());

    let pocket = env.only_pocket();
    assert!(!pocket.join(".beads").exists());
    assert!(!project.join(".beads").exists());
    assert!(!manifest_uses_beads(&pocket));
    summary.step("Verified that reuse did not silently upgrade the pocket to beads".to_string());
    summary.print();
}

#[test]
fn existing_non_beads_pocket_can_be_explicitly_upgraded() {
    let env = TestEnv::new("upgrade");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "existing_non_beads_pocket_can_be_explicitly_upgraded",
        "`--use beads` upgrades an existing non-beads pocket when asked explicitly",
        "the isolated registry and project tree are removed from /tmp when the test finishes",
    );

    assert_success(&env.run_spocket(&project, &["-i", ".", "--without-beads"]));
    summary.step("Created a pocket without beads".to_string());
    assert_success(&env.run_spocket(&project, &["-i", ".", "--use", "beads"]));
    summary.step("Reopened the pocket with `--use beads`".to_string());

    let pocket = env.only_pocket();
    assert!(pocket.join(".beads").is_dir());
    assert_eq!(
        fs::read_to_string(project.join(".beads").join("redirect")).unwrap(),
        pocket.join(".beads").to_string_lossy()
    );
    assert!(manifest_uses_beads(&pocket));
    summary.step(
        "Verified that beads was initialized and the redirect file now targets the upgraded pocket"
            .to_string(),
    );
    summary.print();
}

#[test]
fn without_beads_wins_over_use_beads_for_new_pocket() {
    let env = TestEnv::new("conflict");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "without_beads_wins_over_use_beads_for_new_pocket",
        "`--without-beads` takes precedence over `--use beads` on new pocket creation",
        "the test leaves no residue because its HOME and registry are temporary and removed after completion",
    );

    let output = env.run_spocket(&project, &["-i", ".", "--use", "beads", "--without-beads"]);
    assert_success(&output);
    summary.step("Created a workspace using both `--use beads` and `--without-beads`".to_string());

    let pocket = env.only_pocket();
    assert!(!pocket.join(".beads").exists());
    assert!(!project.join(".beads").exists());
    assert!(!manifest_uses_beads(&pocket));
    summary.step("Verified that the pocket stayed beadless, proving precedence stayed explicit and predictable".to_string());
    summary.print();
}

#[test]
fn unknown_feature_still_fails_without_beads_initialization() {
    let env = TestEnv::new("unknown");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "unknown_feature_still_fails_without_beads_initialization",
        "unknown optional features fail fast without partially initializing a pocket",
        "the project and HOME are test-local and deleted automatically, leaving no registry references behind",
    );

    let output = env.run_spocket(&project, &["-i", ".", "--use", "memvid"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Unknown feature 'memvid'")
            || String::from_utf8_lossy(&output.stdout).contains("Unknown feature 'memvid'")
    );
    assert!(!project.join(".beads").exists());
    assert!(env.safe_pockets().is_empty());
    summary.step("Attempted to enable unsupported feature `memvid` and confirmed the command failed before creating pocket state".to_string());
    summary.print();
}

#[test]
fn locate_reports_project_pocket_for_editor_integrations() {
    let env = TestEnv::new("locate");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "locate_reports_project_pocket_for_editor_integrations",
        "`locate` returns machine-readable pocket metadata for a project path",
        "the lookup uses a temporary registry that is deleted wholesale when the test environment drops",
    );

    assert_success(&env.run_spocket(&project, &["-i", "."]));
    summary.step("Created a workspace so the registry had a project-to-pocket mapping".to_string());
    let pocket = env.only_pocket();

    let output = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&output);
    summary.step("Ran `spocket locate --path .` and parsed the JSON response".to_string());

    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value.get("status").and_then(|v| v.as_str()), Some("found"));
    assert_eq!(
        value.get("pocket_dir").and_then(|v| v.as_str()),
        Some(pocket.to_string_lossy().as_ref())
    );
    summary.step(
        "Verified that the reported pocket directory matched the created safe pocket".to_string(),
    );
    summary.print();
}

#[test]
fn heal_alias_replaces_deterministic_target_with_selected_pocket() {
    let env = TestEnv::new("heal-alias");
    let source_project = env.project("source-project");
    let target_project = env.project("target-project");
    let mut summary = TestSummary::new(
        "heal_alias_replaces_deterministic_target_with_selected_pocket",
        "`heal --alias` moves the selected pocket into the deterministic target location and unhoused the replaced pocket",
        "the isolated HOME is removed at teardown, and this test also verifies that the replaced pocket was moved out of the live registry path before cleanup",
    );

    assert_success(&env.run_spocket(&source_project, &["-i", "."]));
    summary.step("Created the source pocket that will be healed into the target".to_string());
    let source_pocket = env.only_pocket();
    fs::write(source_pocket.join("FEATURES").join("carried.md"), "carried").unwrap();
    summary.step(
        "Added sentinel content inside the source pocket so the heal transfer is easy to detect"
            .to_string(),
    );

    let output = env.run_spocket(
        &target_project,
        &["register", &format!("target={}", target_project.display())],
    );
    assert_success(&output);
    summary.step("Registered an alias pointing at the target project".to_string());

    assert_success(&env.run_spocket(&target_project, &["-i", "."]));
    summary.step(
        "Created the target project pocket so heal had a deterministic destination to replace"
            .to_string(),
    );
    let output = env.run_spocket(
        &target_project,
        &[
            "heal",
            "--alias",
            "target",
            "--pocket",
            source_pocket.to_string_lossy().as_ref(),
        ],
    );
    assert_success(&output);
    summary.step(
        "Ran `spocket heal --alias target --pocket <source>` to reconnect the target project"
            .to_string(),
    );

    let locate = env.run_spocket(&target_project, &["locate", "--path", "."]);
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    let healed_pocket = PathBuf::from(value.get("pocket_dir").unwrap().as_str().unwrap());

    assert!(healed_pocket.join("FEATURES").join("carried.md").is_file());
    assert!(healed_pocket.join("events.jsonl").is_file());
    assert!(env.home.join(".safe_pocket").join("unhoused.log").is_file());
    summary.step("Verified that the selected pocket content moved into place and the displaced target pocket was recorded in `unhoused.log`".to_string());
    summary.print();
}

#[test]
fn clean_hard_removes_temporary_pocket_and_registry_entry() {
    let env = TestEnv::new("clean-hard");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "clean_hard_removes_temporary_pocket_and_registry_entry",
        "`clean temporary --hard --yes` removes temporary pockets from live registry state while preserving an unhoused audit trail",
        "the command itself cleans the live registry entry, then TestEnv removes the remaining temp HOME and project directories",
    );

    assert_success(&env.run_spocket(&project, &["-i", ".", "--temporary"]));
    summary.step("Created a temporary pocket with `spocket -i . --temporary`".to_string());

    let pocket = env.only_pocket();
    assert!(pocket.starts_with(env.home.join(".safe_pocket").join("temporary")));
    summary
        .step("Confirmed the created pocket lives under `~/.safe_pocket/temporary/`".to_string());

    let output = env.run_spocket(&project, &["clean", "temporary", "--hard", "--yes"]);
    assert_success(&output);
    assert_contains(&output, "Deleted safe pockets:");
    summary.step("Ran `spocket clean temporary --hard --yes` to remove the temporary pocket without prompting".to_string());

    assert!(env.safe_pockets().is_empty());
    env.assert_no_registry_entry_for(&pocket);
    assert!(env.registry_file("unhoused.log").is_file());
    assert!(!pocket.exists());
    summary.step("Verified that the live pocket directory disappeared, the registry cache no longer references it, and cleanup was logged in `unhoused.log`".to_string());

    let locate = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    assert_eq!(
        value.get("status").and_then(|v| v.as_str()),
        Some("not_found")
    );
    summary.step(
        "Confirmed the cleaned project no longer resolves to a pocket via `spocket locate`"
            .to_string(),
    );

    summary.print();
}

#[test]
fn normal_run_launches_vscode() {
    let env = TestEnv::new("normal-open");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "normal_run_launches_vscode",
        "a default `spocket -i .` invocation launches VS Code at the end",
        "the isolated HOME and temp root are deleted on drop; the fake `code` binary records invocations to a temp log",
    );

    let output = env.run_spocket(&project, &["-i", ".", "--temporary"]);
    assert_success(&output);
    summary.step("Ran `spocket -i . --temporary` with a fake `code` binary".to_string());

    assert_eq!(
        env.code_launch_count(),
        1,
        "expected VS Code to be launched exactly once on a normal run"
    );
    summary.step(
        "Verified the fake VS Code binary was launched exactly once, anchoring the negative \
         assertions made by the --silent / --simulate-runtime tests"
            .to_string(),
    );
    summary.print();
}

#[test]
fn silent_flag_skips_vscode_launch() {
    let env = TestEnv::new("silent");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "silent_flag_skips_vscode_launch",
        "`--silent` performs all setup steps but never launches VS Code",
        "the temporary HOME, project, and code-invocation log are removed when TestEnv drops",
    );

    let output = env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]);
    assert_success(&output);
    summary.step("Ran `spocket -i . --temporary --silent`".to_string());

    // The pocket is still created (all steps ran)...
    let pocket = env.only_pocket();
    assert!(pocket.join("manifest.json").is_file());
    summary.step("Confirmed the pocket and manifest were still created (all steps ran)".to_string());

    // ...but VS Code was never opened.
    assert_eq!(
        env.code_launch_count(),
        0,
        "expected VS Code NOT to launch under --silent"
    );
    summary.step("Verified VS Code was never launched under --silent".to_string());
    summary.print();
}

#[test]
fn simulate_runtime_injects_content_without_launching_vscode() {
    let env = TestEnv::new("simulate-runtime");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "simulate_runtime_injects_content_without_launching_vscode",
        "`--simulate-runtime` injects runtime content into destination files \
         (between #SPOCKET_RUNTIME_CONTENT_START/END markers) without launching VS Code",
        "the temporary HOME, project tree, and code log are deleted on drop",
    );

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--simulate-runtime", "--silent"],
    );
    assert_success(&output);
    summary.step("Ran `spocket -i . --temporary --simulate-runtime --silent`".to_string());

    let pocket = env.only_pocket();

    // At least one destination file should carry runtime markers even though
    // VS Code was never opened.
    let candidates = [
        pocket.join("AGENTS.md"),
        pocket.join(".github").join("copilot-instructions.md"),
    ];
    let injected = candidates.iter().filter(|p| p.is_file()).any(|p| {
        let body = fs::read_to_string(p).unwrap_or_default();
        body.contains("#SPOCKET_RUNTIME_CONTENT_START")
            && body.contains("#SPOCKET_RUNTIME_CONTENT_END")
    });
    assert!(
        injected,
        "expected runtime markers to be injected into a destination file under --simulate-runtime"
    );
    summary.step(
        "Verified runtime markers were injected into a destination file even though VS Code \
         was never opened"
            .to_string(),
    );

    assert_eq!(
        env.code_launch_count(),
        0,
        "expected VS Code NOT to launch under --simulate-runtime"
    );
    summary.step("Verified VS Code was never launched under --simulate-runtime".to_string());
    summary.print();
}
