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
            "spocket-cli-behavior-{name}-{}-{unique}",
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

        let existing_path = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{existing_path}", bin_dir.display());

        Self { root, home, path }
    }

    fn project(&self, name: &str) -> PathBuf {
        let project = self.root.join(name);
        fs::create_dir_all(&project).expect("failed to create project dir");
        project
    }

    fn pocket_root(&self) -> PathBuf {
        self.home.join(".left_pocket")
    }

    fn legacy_safe_pocket_root(&self) -> PathBuf {
        self.home.join(".safe_pocket")
    }

    fn config_root(&self) -> PathBuf {
        self.home.join(".config/left_pocket")
    }

    fn run_spocket(&self, project: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_left_pocket"))
            .args(args)
            .current_dir(project)
            .env("HOME", &self.home)
            .env("PATH", &self.path)
            .env("CODE_LOG", self.code_log())
            .env("GIT_CONFIG_GLOBAL", self.root.join("gitconfig"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("failed to run pocket")
    }

    fn run_safe_pocket_legacy(&self, project: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_safe_pocket"))
            .args(args)
            .current_dir(project)
            .env("HOME", &self.home)
            .env("PATH", &self.path)
            .env("CODE_LOG", self.code_log())
            .env("GIT_CONFIG_GLOBAL", self.root.join("gitconfig"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("failed to run safe_pocket compatibility binary")
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
        let registry = self.pocket_root();
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
                    .expect("failed to read pocket registry")
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
        assert_eq!(pockets.len(), 1, "expected exactly one pocket");
        pockets.into_iter().next().expect("pocket should exist")
    }

    fn workspace_file(&self) -> PathBuf {
        let pocket = self.only_pocket();
        let name = pocket.file_name().unwrap().to_string_lossy().to_string();
        pocket.join(format!("{name}.code-workspace"))
    }

    fn registry_file(&self, name: &str) -> PathBuf {
        self.pocket_root().join(name)
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
        vec!["sync-registry", "--help"],
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
    assert_contains(&short_version, env!("CARGO_PKG_VERSION"));
    assert_contains(&short_version, "pocket");
    summary.step("Ran `spocket -v` and verified the version and logo rendered".to_string());

    let long_version = env.run_spocket(&project, &["--version"]);
    assert_success(&long_version);
    assert_contains(&long_version, env!("CARGO_PKG_VERSION"));
    summary
        .step("Ran `spocket --version` and verified version output remained reachable".to_string());

    summary.print();
}

#[test]
fn install_instructions_reference_real_vscode_extension_dir() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let install = fs::read_to_string(root.join("Install.md")).unwrap();

    assert!(
        !install.contains("vscode_extension"),
        "Install.md should not reference the old underscore directory name"
    );
    assert!(
        install.contains("{{PROJECT_ROOT}}/vscode-extension"),
        "Install.md should point to the checked-in vscode-extension directory"
    );
    assert!(
        root.join("vscode-extension/package.json").is_file(),
        "documented VS Code extension directory should exist and contain package.json"
    );
}

#[test]
fn install_default_assets_seeds_pocket_roots_even_with_legacy_dirs_present() {
    let env = TestEnv::new("install-default-assets");
    let project = env.project("project");

    fs::create_dir_all(env.home.join(".config/safe_pocket")).unwrap();
    fs::create_dir_all(env.legacy_safe_pocket_root()).unwrap();

    let output = env.run_spocket(&project, &["install-default-assets"]);
    assert_success(&output);

    assert!(env.config_root().is_dir());
    assert!(env.config_root().join("templates/AGENTS.md").is_file());
    assert!(env.config_root().join("feature_tags.yaml").is_file());
    assert!(env.pocket_root().is_dir());
    assert!(env.pocket_root().join("observations").is_dir());
    assert!(env.pocket_root().join(".git").is_dir());
}

#[test]
fn install_default_assets_migrates_renamed_root_state() {
    let env = TestEnv::new("install-migrates-renamed-root");
    let project = env.project("project");

    fs::create_dir_all(env.legacy_safe_pocket_root()).unwrap();
    assert_success(&env.run_spocket(&project, &["-i", ".", "--silent"]));

    let locate_before = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate_before);
    let before_value: serde_json::Value = serde_json::from_slice(&locate_before.stdout).unwrap();
    let legacy_pocket = PathBuf::from(before_value.get("pocket_dir").unwrap().as_str().unwrap());
    let hash = legacy_pocket
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let legacy_workspace = legacy_pocket.join(format!("{hash}.code-workspace"));

    fs::rename(env.legacy_safe_pocket_root(), env.pocket_root()).unwrap();
    let renamed_pocket = env.pocket_root().join(&hash);
    let renamed_workspace = renamed_pocket.join(format!("{hash}.code-workspace"));

    let stale_env = fs::read_to_string(project.join(".env")).unwrap();
    assert!(stale_env.contains(".safe_pocket"));
    let stale_workspace = fs::read_to_string(&renamed_workspace).unwrap();
    assert!(stale_workspace.contains(".safe_pocket"));
    let stale_cache = fs::read_to_string(env.pocket_root().join("registry_cache.json")).unwrap();
    assert!(stale_cache.contains(".safe_pocket"));
    assert!(!legacy_workspace.exists());

    let output = env.run_spocket(&project, &["install-default-assets"]);
    assert_success(&output);

    let locate_after = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate_after);
    let after_value: serde_json::Value = serde_json::from_slice(&locate_after.stdout).unwrap();
    let migrated_pocket = PathBuf::from(after_value.get("pocket_dir").unwrap().as_str().unwrap());
    assert_eq!(migrated_pocket, renamed_pocket);

    let migrated_env = fs::read_to_string(project.join(".env")).unwrap();
    assert!(migrated_env.contains(&format!("POCKET_ROOT={}", renamed_pocket.display())));
    // The rename is complete: migration rewrites LEFT_POCKET_ROOT and drops the stale
    // legacy key rather than carrying a second, contradictory root forward.
    assert!(
        !migrated_env.contains("SPOCKET_ROOT="),
        "migration should not re-emit the legacy root key, got:\n{migrated_env}"
    );

    let migrated_workspace_text = fs::read_to_string(&renamed_workspace).unwrap();
    assert!(migrated_workspace_text.contains(&renamed_pocket.display().to_string()));
    assert!(!migrated_workspace_text.contains(".safe_pocket"));

    let migrated_cache = fs::read_to_string(env.pocket_root().join("registry_cache.json")).unwrap();
    assert!(migrated_cache.contains(&renamed_pocket.display().to_string()));
    assert!(!migrated_cache.contains(".safe_pocket"));
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
    summary
        .step("Verified that the reported pocket directory matched the created pocket".to_string());
    summary.print();
}

#[test]
fn add_tool_does_not_rebind_project_when_overlapping_pocket_exists() {
    let env = TestEnv::new("overlap-pocket-binding");
    let project = env.project("project");
    let extra = env.project("extra");

    assert_success(&env.run_spocket(&project, &["-i", ".", "--silent"]));
    let primary_pocket = env.only_pocket();

    let overlap = env.run_spocket(
        &project,
        &[
            "-i",
            project.to_string_lossy().as_ref(),
            "-i",
            extra.to_string_lossy().as_ref(),
            "--new",
            "--silent",
        ],
    );
    assert_success(&overlap);

    let pockets = env.safe_pockets();
    assert_eq!(pockets.len(), 2, "expected an exact and overlapping pocket");
    let overlapping_pocket = pockets
        .iter()
        .find(|p| **p != primary_pocket)
        .expect("overlapping pocket should exist")
        .to_path_buf();

    let locate_before = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate_before);
    let value_before: serde_json::Value = serde_json::from_slice(&locate_before.stdout).unwrap();
    assert_eq!(
        value_before.get("pocket_dir").and_then(|v| v.as_str()),
        Some(primary_pocket.to_string_lossy().as_ref())
    );

    let add = env.run_spocket(&project, &["-i", ".", "--add", "memgraph", "--silent"]);
    assert_success(&add);

    let locate_after = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate_after);
    let value_after: serde_json::Value = serde_json::from_slice(&locate_after.stdout).unwrap();
    assert_eq!(
        value_after.get("pocket_dir").and_then(|v| v.as_str()),
        Some(primary_pocket.to_string_lossy().as_ref())
    );

    assert!(primary_pocket
        .join("tools/memgraph/scan-config.json")
        .is_file());
    assert!(
        !overlapping_pocket.join("tools/memgraph").exists(),
        "overlapping pocket should not be mutated by --add from the exact project pocket"
    );
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
    assert!(env.pocket_root().join("unhoused.log").is_file());
    summary.step("Verified that the selected pocket content moved into place and the displaced target pocket was recorded in `unhoused.log`".to_string());
    summary.print();
}

#[test]
fn heal_project_pocket_rewrites_in_place_workspace_file() {
    let env = TestEnv::new("heal-in-place-rewrite");
    let project = env.project("project");

    assert_success(&env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]));
    let pocket = env.only_pocket();
    let workspace_file = env.workspace_file();
    let name = pocket.file_name().unwrap().to_string_lossy().to_string();

    fs::write(
        &workspace_file,
        format!(
            "{{\n  \"folders\": [\n    {{\n      \"path\": \"{}\",\n      \"name\": \"[left_pocket] {name}\"\n    }}\n  ]\n}}\n",
            pocket.display()
        ),
    )
    .unwrap();

    let output = env.run_spocket(
        &project,
        &[
            "heal",
            "--project",
            project.to_string_lossy().as_ref(),
            "--pocket",
            pocket.to_string_lossy().as_ref(),
        ],
    );
    assert_success(&output);

    let workspace_text = fs::read_to_string(&workspace_file).unwrap();
    assert!(workspace_text.contains(&project.display().to_string()));
    assert!(workspace_text.contains("[left_pocket]"));

    let locate = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    assert_eq!(
        value.get("pocket_dir").and_then(|v| v.as_str()),
        Some(pocket.to_string_lossy().as_ref())
    );
}

#[test]
fn legacy_pocket_flag_remains_accepted_as_hidden_alias() {
    let env = TestEnv::new("legacy-pocket-flag");
    let project = env.project("project");

    assert_success(&env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]));
    let pocket = env.only_pocket();

    // The pre-rename flag spelling must keep working as a hidden alias.
    let output = env.run_spocket(
        &project,
        &[
            "daily-feature",
            "--pocket",
            pocket.to_string_lossy().as_ref(),
        ],
    );
    assert_success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value.get("status").and_then(|v| v.as_str()), Some("ok"));

    // The primary flag is --pocket: help advertises it, never the legacy alias.
    let help = env.run_spocket(&project, &["heal", "--help"]);
    assert_success(&help);
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("--pocket"));
    assert!(!text.contains("--left_pocket"));
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
    summary.step("Created a temporary pocket with `sleft_pocket -i . --temporary`".to_string());

    let pocket = env.only_pocket();
    assert!(pocket.starts_with(env.pocket_root().join("temporary")));
    summary
        .step("Confirmed the created pocket lives under `~/.safe_pocket/temporary/`".to_string());

    let output = env.run_spocket(&project, &["clean", "temporary", "--hard", "--yes"]);
    assert_success(&output);
    assert_contains(&output, "Deleted pockets:");
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
        "a default `sleft_pocket -i .` invocation launches VS Code at the end",
        "the isolated HOME and temp root are deleted on drop; the fake `code` binary records invocations to a temp log",
    );

    let output = env.run_spocket(&project, &["-i", ".", "--temporary"]);
    assert_success(&output);
    summary.step("Ran `sleft_pocket -i . --temporary` with a fake `code` binary".to_string());

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
fn normal_run_prints_memgraph_connection_details_when_installed() {
    let env = TestEnv::new("memgraph-connection-details");
    let project = env.project("project");

    let install = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "memgraph", "--silent"],
    );
    assert_success(&install);

    let output = env.run_spocket(&project, &["-i", ".", "--temporary"]);
    assert_success(&output);
    assert_contains(&output, "Memgraph");
    assert_contains(&output, "Lab:");
    assert_contains(&output, "Bolt:");
    assert_contains(&output, "http://127.0.0.1:");
    assert_contains(&output, "bolt://127.0.0.1:");
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
    summary.step("Ran `sleft_pocket -i . --temporary --silent`".to_string());

    // The pocket is still created (all steps ran)...
    let pocket = env.only_pocket();
    assert!(pocket.join("manifest.json").is_file());
    summary
        .step("Confirmed the pocket and manifest were still created (all steps ran)".to_string());

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
         (between #LEFT_POCKET_RUNTIME_CONTENT_START/END markers) without launching VS Code",
        "the temporary HOME, project tree, and code log are deleted on drop",
    );

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--simulate-runtime", "--silent"],
    );
    assert_success(&output);
    summary.step("Ran `sleft_pocket -i . --temporary --simulate-runtime --silent`".to_string());

    let pocket = env.only_pocket();

    // At least one destination file should carry runtime markers even though
    // VS Code was never opened.
    let candidates = [
        pocket.join("AGENTS.md"),
        pocket.join(".github").join("copilot-instructions.md"),
    ];
    let injected = candidates.iter().filter(|p| p.is_file()).any(|p| {
        let body = fs::read_to_string(p).unwrap_or_default();
        body.contains("#POCKET_RUNTIME_CONTENT_START")
            && body.contains("#POCKET_RUNTIME_CONTENT_END")
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

/// The built-in task tracker replaces Beads. A freshly created pocket must not
/// receive any `.beads` artifacts or redirect stubs.
#[test]
fn new_pocket_has_no_beads_artifacts() {
    let env = TestEnv::new("no-beads");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "new_pocket_has_no_beads_artifacts",
        "default workspace creation no longer initializes Beads anywhere",
        "the isolated HOME and temp root are removed recursively on drop",
    );

    let output = env.run_spocket(&project, &["-i", "."]);
    assert_success(&output);
    summary.step("Opened a new workspace with `sleft_pocket -i .`".to_string());

    let pocket = env.only_pocket();
    assert!(!pocket.join(".beads").exists());
    assert!(!project.join(".beads").exists());

    let manifest = fs::read_to_string(pocket.join("manifest.json")).unwrap();
    assert!(
        !manifest.contains("uses_beads"),
        "manifest should not carry the legacy uses_beads field"
    );
    summary
        .step("Confirmed no `.beads` directories and no `uses_beads` manifest field".to_string());

    // The project .env should still be written, just without a BEADS_DIR line.
    let project_env = fs::read_to_string(project.join(".env")).unwrap();
    assert!(project_env.contains("POCKET_ROOT="));
    assert!(!project_env.contains("BEADS_DIR="));
    summary.step("Verified `.env` carries LEFT_POCKET_ROOT, with no BEADS_DIR line".to_string());
    summary.print();
}

/// Both the project `.env` and the pocket `.env` must carry PROJECT_ROOT and
/// LEFT_POCKET_ROOT. The Spocket rename is complete, so the legacy `SPOCKET_ROOT`
/// key must NOT be written any more; compatibility is read-only.
#[test]
fn env_files_carry_project_and_pocket_roots() {
    let env = TestEnv::new("env-roots");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "env_files_carry_project_and_pocket_roots",
        "project/.env and pocket/.env define PROJECT_ROOT and LEFT_POCKET_ROOT, and never write legacy SPOCKET_ROOT",
        "the isolated HOME and temp root are removed recursively on drop",
    );

    let output = env.run_spocket(&project, &["-i", ".", "--silent"]);
    assert_success(&output);
    summary.step("Created a workspace with `left_pocket -i . --silent`".to_string());

    let pocket = env.only_pocket();

    let project_env = fs::read_to_string(project.join(".env")).unwrap();
    for key in ["PROJECT_ROOT=", "POCKET_ROOT="] {
        assert!(
            project_env.contains(key),
            "project .env missing {key}\n--- .env ---\n{project_env}"
        );
    }
    assert!(
        !project_env.contains("SPOCKET_ROOT="),
        "project .env must not write the legacy root key\n--- .env ---\n{project_env}"
    );
    summary.step(
        "Verified project/.env defines PROJECT_ROOT and LEFT_POCKET_ROOT with no legacy key"
            .to_string(),
    );

    let pocket_env = fs::read_to_string(pocket.join(".env")).unwrap();
    for key in ["PROJECT_ROOT=", "POCKET_ROOT="] {
        assert!(
            pocket_env.contains(key),
            "pocket .env missing {key}\n--- .env ---\n{pocket_env}"
        );
    }
    summary.step("Verified pocket/.env defines PROJECT_ROOT and LEFT_POCKET_ROOT".to_string());
    summary.print();
}

/// `install-default-assets` must not abort when a pocket's manifest references
/// a project directory that no longer exists (deleted repo, stale temp-dir
/// pocket from a previous test run, unmounted volume).
#[test]
fn install_default_assets_tolerates_missing_project_directory() {
    let env = TestEnv::new("install-missing-project");
    let project = env.project("doomed-project");
    let mut summary = TestSummary::new(
        "install_default_assets_tolerates_missing_project_directory",
        "install-default-assets skips pockets whose project directory is gone instead of failing",
        "the isolated HOME and temp root are removed recursively on drop",
    );

    let created = env.run_spocket(&project, &["-i", ".", "--silent"]);
    assert_success(&created);
    summary.step("Created a pocket for a project directory".to_string());

    // Delete the project directory, leaving the pocket's manifest pointing at
    // a path that no longer exists.
    fs::remove_dir_all(&project).unwrap();
    assert!(!project.exists());
    summary.step("Deleted the project directory, orphaning the pocket".to_string());

    // Run from an unrelated directory that still exists.
    let elsewhere = env.project("elsewhere");
    let output = env.run_spocket(&elsewhere, &["install-default-assets"]);
    assert_success(&output);
    summary
        .step("`install-default-assets` still succeeded despite the orphaned pocket".to_string());
    summary.print();
}

/// `pocket upgrade-installation` rewrites legacy `#SPOCKET_*` directives and
/// runtime markers to their `#POCKET_*` equivalents across the installed roots,
/// leaves user-facing feature-tag names alone, and honours `--dry-run`.
#[test]
fn upgrade_installation_rewrites_legacy_spocket_tokens() {
    let env = TestEnv::new("upgrade-installation");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "upgrade_installation_rewrites_legacy_spocket_tokens",
        "`pocket upgrade-installation` migrates legacy #SPOCKET_*/#LEFT_POCKET_* tokens to #POCKET_* in place",
        "the isolated HOME and temp root are removed recursively on drop",
    );

    let created = env.run_spocket(&project, &["-i", ".", "--silent"]);
    assert_success(&created);
    let pocket = env.only_pocket();

    // Plant a file carrying every legacy token, plus a feature-tag name that
    // must survive untouched.
    let legacy = pocket.join("legacy-notes.md");
    let legacy_body = "#LEFT_POCKET_TEMPLATE_DESTINATION: x.md\n\
                       #LEFT_POCKET_QUIET_MERGE\n\
                       #LEFT_POCKET_MERGE_AT_RUNTIME\n\
                       #LEFT_POCKET_INSTALL_DESTINATION: y.yaml\n\
                       #LEFT_POCKET_RUNTIME_CONTENT_START\n\
                       body\n\
                       #LEFT_POCKET_RUNTIME_CONTENT_END\n\
                       <!-- BEGIN LEFT_POCKET TASK INTEGRATION -->\n\
                       <!-- END LEFT_POCKET TASK INTEGRATION -->\n\
                       #SPOCKET_MUST_INSTALL\n";
    fs::write(&legacy, legacy_body).unwrap();
    summary.step("Planted a file containing every legacy #SPOCKET_* token".to_string());

    // A dry run must report findings without touching the file.
    let dry = env.run_spocket(&project, &["upgrade-installation", "--dry-run"]);
    assert_success(&dry);
    let dry_stdout = String::from_utf8_lossy(&dry.stdout);
    assert!(
        dry_stdout.contains("Dry run only"),
        "dry run should say so:\n{dry_stdout}"
    );
    assert_eq!(
        fs::read_to_string(&legacy).unwrap(),
        legacy_body,
        "dry run must not modify any file"
    );
    summary.step("Verified `--dry-run` reported findings without modifying files".to_string());

    // The real run rewrites the structural tokens.
    let applied = env.run_spocket(&project, &["upgrade-installation", "--yes"]);
    assert_success(&applied);
    let after = fs::read_to_string(&legacy).unwrap();

    for token in [
        "#POCKET_TEMPLATE_DESTINATION",
        "#POCKET_QUIET_MERGE",
        "#POCKET_MERGE_AT_RUNTIME",
        "#POCKET_INSTALL_DESTINATION",
        "#POCKET_RUNTIME_CONTENT_START",
        "#POCKET_RUNTIME_CONTENT_END",
        "<!-- BEGIN POCKET TASK INTEGRATION -->",
        "<!-- END POCKET TASK INTEGRATION -->",
    ] {
        assert!(
            after.contains(token),
            "expected {token} after upgrade\n--- file ---\n{after}"
        );
    }
    assert!(
        !after.contains("LEFT_POCKET"),
        "legacy #LEFT_POCKET_* tokens must be rewritten\n--- file ---\n{after}"
    );
    summary.step("Verified all structural directives/markers became #POCKET_*".to_string());

    assert!(
        after.contains("#SPOCKET_MUST_INSTALL"),
        "user-facing feature-tag names must NOT be rewritten\n--- file ---\n{after}"
    );
    summary.step("Verified the #SPOCKET_MUST_INSTALL feature tag was left intact".to_string());

    // Running again is a no-op.
    let again = env.run_spocket(&project, &["upgrade-installation", "--dry-run"]);
    assert_success(&again);
    assert!(
        String::from_utf8_lossy(&again.stdout).contains("nothing to upgrade"),
        "a second pass should find nothing left to migrate"
    );
    summary.step("Verified the migration is idempotent".to_string());
    summary.print();
}

/// AGENTS.md should advertise the built-in `pocket task` tracker at runtime.
#[test]
fn runtime_agents_md_advertises_task_tracker() {
    let env = TestEnv::new("task-block");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "runtime_agents_md_advertises_task_tracker",
        "runtime merge injects the pocket task guidance block into AGENTS.md",
        "the temporary HOME and project tree are deleted on drop",
    );

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--simulate-runtime", "--silent"],
    );
    assert_success(&output);
    summary.step("Ran `sleft_pocket -i . --temporary --simulate-runtime --silent`".to_string());

    let pocket = env.only_pocket();
    let agents = fs::read_to_string(pocket.join("AGENTS.md")).unwrap();
    assert!(
        agents.contains("pocket task"),
        "AGENTS.md should mention the built-in task tracker"
    );
    assert!(
        !agents.contains("bd ready"),
        "AGENTS.md should not mention beads"
    );
    summary.step("Verified AGENTS.md mentions `pocket task` and not beads".to_string());
    summary.print();
}

#[test]
fn missing_template_destination_warning_is_verbose_only() {
    let env = TestEnv::new("verbose-template-warning");
    let project = env.project("project");
    let template_dir = env.config_root().join("templates/feature_tags");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(
        template_dir.join("conversation.feature.tag.yaml"),
        "description: this is referenced by feature_tags.yaml, not a pocket template\n",
    )
    .unwrap();

    let quiet = env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]);
    assert_success(&quiet);
    assert!(
        !String::from_utf8_lossy(&quiet.stderr).contains("missing #POCKET_TEMPLATE_DESTINATION"),
        "non-verbose run should not warn about referenced feature tag files"
    );

    let other = env.project("other-project");
    let verbose = env.run_spocket(&other, &["-i", ".", "--temporary", "--silent", "--verbose"]);
    assert_success(&verbose);
    assert!(
        String::from_utf8_lossy(&verbose.stderr).contains("missing #POCKET_TEMPLATE_DESTINATION"),
        "verbose run should surface skipped template diagnostics"
    );
}

#[test]
fn daily_feature_loads_auto_tags_from_feature_tags_yaml() {
    let env = TestEnv::new("feature-tags-yaml");
    let project = env.project("project");
    fs::create_dir_all(env.config_root()).unwrap();
    fs::write(
        env.config_root().join("feature_tags.yaml"),
        "SPOCKET_COUNT_JELLYBEANS:\n  description: /tmp/count.jellybeans.feature.tag.yaml\n  place_automatically: true\n  type: done hook\n",
    )
    .unwrap();

    assert_success(&env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]));
    let pocket = env.only_pocket();
    let daily = env.run_spocket(
        &project,
        &[
            "daily-feature",
            "--pocket",
            pocket.to_string_lossy().as_ref(),
            "--new",
        ],
    );
    assert_success(&daily);
    let value: serde_json::Value = serde_json::from_slice(&daily.stdout).unwrap();
    let path = PathBuf::from(value.get("path").unwrap().as_str().unwrap());
    let content = fs::read_to_string(path).unwrap();
    assert!(
        content.starts_with("#SPOCKET_COUNT_JELLYBEANS\n"),
        "daily feature should use auto tags from feature_tags.yaml"
    );
}

#[test]
fn with_tool_is_session_sidecar_only_and_add_tool_persists() {
    let env = TestEnv::new("tool-flags");
    let project = env.project("project");

    let with_output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--with", "graphify", "--silent"],
    );
    assert_success(&with_output);
    let workspace_file = env.workspace_file();
    let workspace_text = fs::read_to_string(&workspace_file).unwrap();
    assert!(
        !workspace_text.contains("[Tool] graphify"),
        "--with should not persist tool folders into the workspace file"
    );

    let add_output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "graphify", "--silent"],
    );
    assert_success(&add_output);
    let pocket = env.only_pocket();
    assert!(pocket.join("graphify-out").is_dir());
    assert!(project.join("graphify-out").exists());
    let workspace_text = fs::read_to_string(&workspace_file).unwrap();
    assert!(workspace_text.contains("[Tool] graphify"));
}

#[test]
fn with_memgraph_lives_in_safe_pocket_session_tools_only() {
    let env = TestEnv::new("memgraph-with");
    let project = env.project("project");

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--with", "memgraph", "--silent"],
    );
    assert_success(&output);

    let pocket = env.only_pocket();
    let session_tool = pocket.join(".session-tools/memgraph");
    assert!(session_tool.join("scan-config.json").is_file());
    assert!(session_tool.join("docker-compose.yml").is_file());
    assert!(session_tool.join("schema.cypher").is_file());
    assert!(session_tool.join("scan-safe-pocket.sh").is_file());
    assert!(
        !pocket.join("tools/memgraph").exists(),
        "--with memgraph should not persist into tools/"
    );
    let workspace_text = fs::read_to_string(env.workspace_file()).unwrap();
    assert!(
        !workspace_text.contains("[Tool] memgraph"),
        "--with memgraph should not persist in the workspace file"
    );
}

#[test]
fn add_memgraph_configures_safe_pocket_markdown_scan() {
    let env = TestEnv::new("memgraph-add");
    let project = env.project("project");

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "memgraph", "--silent"],
    );
    assert_success(&output);

    let pocket = env.only_pocket();
    let tool = pocket.join("tools/memgraph");
    assert!(tool.join("docker-compose.yml").is_file());
    let compose = fs::read_to_string(tool.join("docker-compose.yml")).unwrap();
    assert!(compose.contains("memgraph/memgraph-mage:latest"));
    assert!(compose.contains("image: memgraph/lab:latest"));
    assert!(compose.contains("QUICK_CONNECT_MG_HOST=memgraph"));
    assert!(compose.contains("QUICK_CONNECT_MG_PORT=7687"));
    assert!(compose.contains(":3000\""));
    assert!(tool.join("import-into-memgraph.sh").is_file());
    assert!(tool.join("schema.cypher").is_file());
    assert!(tool.join("scan-safe-pocket.sh").is_file());
    assert!(tool.join("data").is_dir());
    assert!(tool.join("logs").is_dir());
    assert!(tool.join("import").is_dir());

    let config: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(tool.join("scan-config.json")).unwrap()).unwrap();
    let paths = config
        .get("scan")
        .and_then(|s| s.get("paths"))
        .and_then(|p| p.as_array())
        .expect("scan paths should be an array");
    assert!(paths
        .iter()
        .any(|p| p.as_str().unwrap().ends_with("/FEATURES")));
    assert!(paths
        .iter()
        .any(|p| p.as_str().unwrap().ends_with("/AGENTS.md")));
    assert_eq!(
        config.get("protocol").and_then(|v| v.as_str()),
        Some("bolt")
    );
    assert_eq!(
        config
            .get("bolt_url")
            .and_then(|v| v.as_str())
            .map(|s| s.starts_with("bolt://127.0.0.1:")),
        Some(true)
    );
    assert_eq!(
        config
            .get("lab_url")
            .and_then(|v| v.as_str())
            .map(|s| s.starts_with("http://127.0.0.1:")),
        Some(true)
    );
    assert_eq!(
        config
            .get("runtime")
            .and_then(|v| v.get("compose_project"))
            .and_then(|v| v.as_str())
            .map(|s| s.starts_with("spocket-")),
        Some(true)
    );

    let workspace_text = fs::read_to_string(env.workspace_file()).unwrap();
    assert!(workspace_text.contains("[Tool] memgraph"));
}

#[test]
fn installed_memgraph_runs_scanner_when_opened() {
    let env = TestEnv::new("memgraph-open-scan");
    let project = env.project("project");

    let first = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "memgraph", "--silent"],
    );
    assert_success(&first);
    let pocket = env.only_pocket();
    let import = pocket.join("tools/memgraph/import/markdown-files.jsonl");
    assert!(import.is_file());
    let initial = fs::read_to_string(&import).unwrap();
    assert!(initial.contains("AGENTS.md") || initial.contains("00.md"));

    fs::write(pocket.join("FEATURES/new-note.md"), "# New Note\n").unwrap();
    let second = env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]);
    assert_success(&second);
    let updated = fs::read_to_string(&import).unwrap();
    assert!(updated.contains("new-note.md"));

    let state = pocket.join("tools/memgraph/.safe_pocket_scan_state.json");
    assert!(state.is_file());
    let before = fs::read_to_string(&state).unwrap();
    let third = env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]);
    assert_success(&third);
    assert_eq!(fs::read_to_string(&state).unwrap(), before);
}

#[test]
fn runtime_merge_stop_handles_installed_memgraph() {
    let env = TestEnv::new("memgraph-stop");
    let project = env.project("project");

    assert_success(&env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "memgraph", "--silent"],
    ));
    let pocket = env.only_pocket();
    let output = env.run_spocket(
        &project,
        &[
            "runtime-merge-stop",
            "--pocket",
            pocket.to_string_lossy().as_ref(),
        ],
    );
    assert_success(&output);
}

#[test]
fn add_memgraph_repairs_safe_pocket_only_workspace_file() {
    let env = TestEnv::new("memgraph-repair");
    let project = env.project("project");

    assert_success(&env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]));
    let pocket = env.only_pocket();
    let workspace_file = env.workspace_file();
    let name = pocket.file_name().unwrap().to_string_lossy().to_string();

    fs::write(
        &workspace_file,
        format!(
            "{{\n  \"folders\": [\n    {{\n      \"path\": \"{}\",\n      \"name\": \"[left_pocket] {name}\"\n    }}\n  ]\n}}\n",
            pocket.display()
        ),
    )
    .unwrap();
    fs::write(
        pocket.join("manifest.json"),
        format!(
            "{{\n  \"hash\": \"e3b0c44298fc\",\n  \"core_paths\": [],\n  \"created_at\": \"2026-06-06T16:17:52.221616Z\",\n  \"temporary\": true,\n  \"children\": [],\n  \"augmented_from\": \"{name}\",\n  \"version\": 1,\n  \"birth_hash\": \"{name}\"\n}}\n"
        ),
    )
    .unwrap();

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "memgraph", "--silent"],
    );
    assert_success(&output);

    let workspace_text = fs::read_to_string(&workspace_file).unwrap();
    assert!(
        workspace_text.contains(&project.display().to_string()),
        "workspace file should recover the project folder"
    );
    assert!(workspace_text.contains("[left_pocket]"));
    assert!(pocket.join("tools/memgraph/scan-config.json").is_file());

    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(pocket.join("manifest.json")).unwrap()).unwrap();
    let core_paths = manifest
        .get("core_paths")
        .and_then(|v| v.as_array())
        .unwrap();
    assert_eq!(core_paths.len(), 1);
    let restored = PathBuf::from(core_paths[0].as_str().unwrap())
        .canonicalize()
        .unwrap();
    assert_eq!(restored, project.canonicalize().unwrap());
}

#[test]
fn add_memgraph_repairs_workspace_file_when_manifest_is_already_good() {
    let env = TestEnv::new("memgraph-repair-workspace-only");
    let project = env.project("project");

    assert_success(&env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]));
    let pocket = env.only_pocket();
    let workspace_file = env.workspace_file();
    let name = pocket.file_name().unwrap().to_string_lossy().to_string();

    fs::write(
        &workspace_file,
        format!(
            "{{\n  \"folders\": [\n    {{\n      \"path\": \"{}\",\n      \"name\": \"[left_pocket] {name}\"\n    }}\n  ]\n}}\n",
            pocket.display()
        ),
    )
    .unwrap();

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "memgraph", "--silent"],
    );
    assert_success(&output);

    let workspace_text = fs::read_to_string(&workspace_file).unwrap();
    assert!(
        workspace_text.contains(&project.display().to_string()),
        "workspace file should recover the project folder even when manifest is already correct"
    );
    assert!(pocket.join("tools/memgraph/scan-config.json").is_file());
}

#[test]
fn sync_pocket_repairs_workspace_file_from_manifest_paths() {
    let env = TestEnv::new("sync-repair-workspace");
    let project = env.project("project");

    assert_success(&env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]));
    let pocket = env.only_pocket();
    let workspace_file = env.workspace_file();
    let name = pocket.file_name().unwrap().to_string_lossy().to_string();

    fs::write(
        &workspace_file,
        format!(
            "{{\n  \"folders\": [\n    {{\n      \"path\": \"{}\",\n      \"name\": \"[left_pocket] {name}\"\n    }}\n  ]\n}}\n",
            pocket.display()
        ),
    )
    .unwrap();

    let output = env.run_spocket(
        &project,
        &["sync", "--pocket", pocket.to_string_lossy().as_ref()],
    );
    assert_success(&output);
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result.get("status").and_then(|v| v.as_str()),
        Some("unchanged")
    );

    let workspace_text = fs::read_to_string(&workspace_file).unwrap();
    assert!(workspace_text.contains(&project.display().to_string()));
}

#[test]
fn add_memgraph_is_idempotent_when_already_installed() {
    let env = TestEnv::new("memgraph-idempotent");
    let project = env.project("project");

    assert_success(&env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "memgraph", "--silent"],
    ));
    let second = env.run_spocket(
        &project,
        &[
            "-i",
            ".",
            "--temporary",
            "--add",
            "memgraph",
            "--silent",
            "--verbose",
        ],
    );
    assert_success(&second);
    assert_contains(&second, "Tool already installed:");
}

#[test]
fn add_gitleaks_writes_project_guard_files() {
    let env = TestEnv::new("gitleaks-add");
    let project = env.project("project");
    let git_init = Command::new("git")
        .arg("init")
        .current_dir(&project)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git init should run");
    assert_success(&git_init);

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "gitleaks", "--silent"],
    );
    assert_success(&output);
    assert!(project.join(".gitleaks.toml").is_file());
    let hook = project.join(".git/hooks/pre-commit");
    assert!(hook.is_file());
    let hook_text = fs::read_to_string(&hook).unwrap();
    assert!(hook_text.contains("tools/gitleaks/pre-commit-hook.sh"));
    assert!(hook_text.contains(&project.display().to_string()));
    let helper = env.only_pocket().join("tools/gitleaks/pre-commit-hook.sh");
    assert!(helper.is_file());
    assert!(!fs::read_to_string(&helper)
        .unwrap()
        .contains("skipping secret scan"));

    let hook_run = Command::new(&hook)
        .current_dir(&project)
        .env("PATH", "")
        .output()
        .expect("pre-commit hook should run");
    assert_failure(&hook_run);
    assert_contains(&hook_run, "gitleaks is required but was not found");

    let workspace_text = fs::read_to_string(env.workspace_file()).unwrap();
    assert!(workspace_text.contains("[Tool] gitleaks"));
}

#[test]
fn add_gitleaks_installs_hook_for_nested_git_project() {
    let env = TestEnv::new("gitleaks-nested");
    let repo = env.project("repo");
    let nested = repo.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let git_init = Command::new("git")
        .arg("init")
        .current_dir(&repo)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("git init should run");
    assert_success(&git_init);

    let output = env.run_spocket(
        &nested,
        &["-i", ".", "--temporary", "--add", "gitleaks", "--silent"],
    );
    assert_success(&output);

    let hook = repo.join(".git/hooks/pre-commit");
    assert!(hook.is_file());
    let hook_text = fs::read_to_string(&hook).unwrap();
    assert!(hook_text.contains("tools/gitleaks/pre-commit-hook.sh"));
    assert!(hook_text.contains(&nested.display().to_string()));
    assert!(nested.join(".gitleaks.toml").is_file());
}

#[test]
fn task_list_bridges_project_and_safe_pocket_directories() {
    let env = TestEnv::new("task-bridge");
    let project = env.project("project");
    assert_success(&env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]));
    let pocket = env.only_pocket();
    assert_success(&env.run_spocket(
        &project,
        &["task", "create", "--named", "Bridge me", "--priority", "1"],
    ));

    let from_project = env.run_spocket(&project, &["task", "list", "--raw"]);
    let from_pocket = env.run_spocket(&pocket, &["task", "list", "--raw"]);
    assert_success(&from_project);
    assert_success(&from_pocket);
    assert_eq!(from_project.stdout, from_pocket.stdout);
}

#[test]
fn completion_spec_exposes_nested_commands_and_tools() {
    let env = TestEnv::new("completion-spec");
    let project = env.project("project");
    let output = env.run_spocket(&project, &["completion-spec"]);
    assert_success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value.to_string().contains("--with"));
    assert!(value.to_string().contains("graphify"));
    assert!(value.to_string().contains("memgraph"));
    assert!(value.to_string().contains("worktree"));
}

/// When `heal` renames a pocket directory, the built-in task tracker must
/// migrate that project's tasks from the old directory-name prefix to the new
/// one so they remain discoverable from the project.
#[test]
fn heal_reprefixes_tracked_tasks() {
    let env = TestEnv::new("heal-reprefix");
    let source_project = env.project("source-project");
    let target_project = env.project("target-project");
    let mut summary = TestSummary::new(
        "heal_reprefixes_tracked_tasks",
        "`heal` migrates a project's tracked tasks to the renamed pocket's prefix",
        "the temporary HOME (including the global tasks.db under it) is removed on drop",
    );

    // Create the source pocket and a task owned by its prefix.
    assert_success(&env.run_spocket(&source_project, &["-i", "."]));
    let source_pocket = env.only_pocket();
    let source_prefix = source_pocket
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    summary.step(format!(
        "Created the source pocket with prefix `{source_prefix}`"
    ));

    let created = env.run_spocket(
        &source_project,
        &[
            "task",
            "create",
            "--named",
            "Carry me across the heal",
            "--priority",
            "1",
        ],
    );
    assert_success(&created);
    let created_out = String::from_utf8_lossy(&created.stdout);
    assert!(
        created_out.contains(&format!("{source_prefix}-")),
        "new task id should carry the source prefix, got: {created_out}"
    );
    summary.step("Created a task whose id is prefixed by the source pocket name".to_string());

    // Register an alias and create the deterministic target pocket.
    assert_success(&env.run_spocket(
        &target_project,
        &["register", &format!("target={}", target_project.display())],
    ));
    assert_success(&env.run_spocket(&target_project, &["-i", "."]));
    summary.step("Registered an alias and created the deterministic target pocket".to_string());

    // Heal the source pocket into the target's deterministic location.
    let heal = env.run_spocket(
        &target_project,
        &[
            "heal",
            "--alias",
            "target",
            "--pocket",
            source_pocket.to_string_lossy().as_ref(),
        ],
    );
    assert_success(&heal);
    assert_contains(&heal, "Reprefixed");
    summary.step("Ran heal and saw the reprefix notice in its output".to_string());

    // The healed pocket's name is the new prefix.
    let locate = env.run_spocket(&target_project, &["locate", "--path", "."]);
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    let healed_pocket = PathBuf::from(value.get("pocket_dir").unwrap().as_str().unwrap());
    let new_prefix = healed_pocket
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_ne!(source_prefix, new_prefix, "heal should change the prefix");

    // Listing tasks from the target project should surface the migrated task,
    // now carrying the new prefix.
    let list = env.run_spocket(&target_project, &["task", "list", "--raw"]);
    assert_success(&list);
    let tasks: serde_json::Value = serde_json::from_slice(&list.stdout).unwrap();
    let arr = tasks
        .as_array()
        .expect("task list --raw should be an array");
    assert_eq!(arr.len(), 1, "expected exactly one migrated task");
    let id = arr[0].get("id").and_then(|v| v.as_str()).unwrap();
    assert!(
        id.starts_with(&format!("{new_prefix}-")),
        "migrated task id should carry the new prefix `{new_prefix}`, got `{id}`"
    );
    assert_eq!(
        arr[0].get("prefix").and_then(|v| v.as_str()),
        Some(new_prefix.as_str())
    );
    summary.step(
        "Verified the task survived the heal and now carries the new pocket prefix".to_string(),
    );
    summary.print();
}

#[test]
fn legacy_safe_pocket_binary_remains_usable() {
    let env = TestEnv::new("legacy-safe-pocket-binary");
    let project = env.project("project");

    let output = env.run_safe_pocket_legacy(&project, &["--version"]);
    assert_success(&output);
    assert_contains(&output, env!("CARGO_PKG_VERSION"));
}

#[test]
fn legacy_safe_pocket_registry_root_is_reused_when_pocket_root_is_absent() {
    let env = TestEnv::new("legacy-root-fallback");
    let project = env.project("project");
    fs::create_dir_all(env.legacy_safe_pocket_root()).unwrap();

    assert_success(&env.run_spocket(&project, &["-i", ".", "--silent"]));

    let locate = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    let pocket_dir = PathBuf::from(value.get("pocket_dir").unwrap().as_str().unwrap());

    assert!(
        pocket_dir.starts_with(env.legacy_safe_pocket_root()),
        "expected legacy root fallback, got {}",
        pocket_dir.display()
    );
}

#[test]
fn pocket_registry_root_takes_precedence_when_both_roots_exist() {
    let env = TestEnv::new("pocket-root-precedence");
    let project = env.project("project");
    fs::create_dir_all(env.legacy_safe_pocket_root()).unwrap();
    fs::create_dir_all(env.pocket_root()).unwrap();

    assert_success(&env.run_spocket(&project, &["-i", ".", "--silent"]));

    let locate = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    let pocket_dir = PathBuf::from(value.get("pocket_dir").unwrap().as_str().unwrap());

    assert!(
        pocket_dir.starts_with(env.pocket_root()),
        "expected primary pocket root to win, got {}",
        pocket_dir.display()
    );
}

/// Write a pocket manifest.json + .code-workspace file pair directly.
///
/// Used by the split-brain tests to set up pockets with specific manifest
/// shapes without going through the `pocket` binary (which would dedupe).
/// `core_paths` are canonicalized so the synthetic manifest matches what a
/// real `pocket -i` invocation would have stored (pocket canonicalizes
/// paths before hashing).
fn write_synthetic_pocket(
    pocket_dir: &Path,
    manifest_hash: &str,
    core_paths: &[PathBuf],
    birth_hash: Option<&str>,
    augmented_from: Option<&str>,
) {
    fs::create_dir_all(pocket_dir).unwrap();
    let dir_name = pocket_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("pocket")
        .to_string();

    let canonical_core_paths: Vec<PathBuf> = core_paths
        .iter()
        .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
        .collect();

    let mut manifest = serde_json::json!({
        "hash": manifest_hash,
        "core_paths": canonical_core_paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
        "created_at": "2026-07-18T16:36:18.181290443Z",
        "temporary": false,
        "children": [],
        "version": 1,
    });
    if let Some(bh) = birth_hash {
        manifest["birth_hash"] = serde_json::json!(bh);
    }
    if let Some(af) = augmented_from {
        manifest["augmented_from"] = serde_json::json!(af);
    }
    fs::write(
        pocket_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let mut folders: Vec<serde_json::Value> = canonical_core_paths
        .iter()
        .map(|p| serde_json::json!({ "path": p.to_string_lossy().to_string() }))
        .collect();
    folders.push(serde_json::json!({
        "path": pocket_dir.to_string_lossy().to_string(),
        "name": format!("[left_pocket] {dir_name}")
    }));
    let workspace = serde_json::json!({ "folders": folders });
    fs::write(
        pocket_dir.join(format!("{dir_name}.code-workspace")),
        serde_json::to_string_pretty(&workspace).unwrap(),
    )
    .unwrap();
}

/// Write a registry_cache.json with the given entries.
///
/// `entries` is a list of `(hash, manifest_hash, path, core_paths)` tuples.
/// `core_paths` are canonicalized to match what `write_synthetic_pocket`
/// stores in the manifest.
fn write_synthetic_cache(root: &Path, entries: &[(&str, &str, &Path, &[PathBuf])]) {
    fs::create_dir_all(root).unwrap();
    let cache = serde_json::json!({
        "version": 1,
        "generated_at": "2026-07-18T16:36:58.547104Z",
        "pockets": entries.iter().map(|(hash, manifest_hash, path, core_paths)| {
            let canonical: Vec<PathBuf> = core_paths.iter().map(|p| p.canonicalize().unwrap_or_else(|_| p.clone())).collect();
            serde_json::json!({
                "hash": hash,
                "manifest_hash": manifest_hash,
                "path": path.to_string_lossy().to_string(),
                "created_at": "2026-07-18T16:36:18.181290443Z",
                "temporary": false,
                "core_paths": canonical.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>(),
                "manifest_version": 1,
            })
        }).collect::<Vec<_>>(),
    });
    fs::write(
        root.join("registry_cache.json"),
        serde_json::to_string_pretty(&cache).unwrap(),
    )
    .unwrap();
}

/// Canonicalize a path for test assertions (macOS `/tmp` -> `/private/tmp`).
fn canon(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Split-brain: the same pocket hash lives in both `~/.left_pocket` and
/// `~/.safe_pocket` with different manifests. The pocket with MORE USER
/// CONTENT (FEATURES, observations, etc.) wins every lookup, because that's
/// the "real" working pocket. An accidentally created duplicate with
/// near-empty content directories loses even if its birth_hash matches the
/// directory name.
#[test]
fn split_brain_registry_dedupes_by_content_count() {
    let env = TestEnv::new("split-brain-dedupe");
    let project = env.project("project");
    let extra = env.project("extra");

    fs::create_dir_all(env.pocket_root()).unwrap();
    fs::create_dir_all(env.legacy_safe_pocket_root()).unwrap();

    let hash = "abc123def456";
    let legacy_pocket = env.legacy_safe_pocket_root().join(hash);
    let primary_pocket = env.pocket_root().join(hash);

    // Rich pocket in the primary root: lots of user content, birth_hash does
    // NOT match the dir name (it was migrated here from an earlier hash).
    write_synthetic_pocket(
        &primary_pocket,
        "richhash00000",
        &[project.clone(), extra.clone()],
        Some("different0000"),
        Some(hash),
    );
    // Add user content so this pocket wins the content-count tiebreaker.
    fs::create_dir_all(primary_pocket.join("FEATURES/dailies")).unwrap();
    for i in 0..10 {
        fs::write(
            primary_pocket
                .join("FEATURES/dailies")
                .join(format!("2026_07_{i:02}.md")),
            "daily note",
        )
        .unwrap();
    }
    fs::create_dir_all(primary_pocket.join("observations")).unwrap();
    fs::write(primary_pocket.join("observations").join("note.md"), "obs").unwrap();

    // Empty pocket in the legacy root: birth_hash MATCHES the dir name (it was
    // freshly created here), but has almost no user content.
    write_synthetic_pocket(
        &legacy_pocket,
        "emptyhash00000",
        &[project.clone()],
        Some(hash),
        Some(hash),
    );
    fs::create_dir_all(legacy_pocket.join("FEATURES")).unwrap();
    fs::write(legacy_pocket.join("FEATURES/00.md"), "minimal").unwrap();

    // Stale caches that don't reflect either on-disk manifest.
    write_synthetic_cache(
        &env.legacy_safe_pocket_root(),
        &[(hash, hash, &legacy_pocket, &[])],
    );
    write_synthetic_cache(
        &env.pocket_root(),
        &[(
            hash,
            "richhash00000",
            &primary_pocket,
            &[project.clone(), extra.clone()],
        )],
    );

    // `pocket locate --path project` must resolve to the rich primary pocket
    // because it has far more user content.
    let locate = env.run_spocket(
        &project,
        &["locate", "--path", project.to_string_lossy().as_ref()],
    );
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    let pocket_dir = PathBuf::from(value.get("pocket_dir").unwrap().as_str().unwrap());
    assert_eq!(
        pocket_dir,
        primary_pocket,
        "expected rich primary pocket (more user content) to win, got {}",
        pocket_dir.display()
    );

    let core_paths = value.get("core_paths").unwrap().as_array().unwrap();
    assert_eq!(
        core_paths.len(),
        2,
        "expected fresh manifest's core_paths, got {core_paths:?}"
    );
}

/// A stale cache entry whose `manifest_hash` accidentally matches a target
/// hash must NOT shadow a fresh on-disk manifest with a different hash.
///
/// This reproduces the original "lost connection" bug: the cache said
/// `manifest_hash = <dir-name>` (the pocket's pre-augment hash), which
/// happened to equal `hash_paths([project])`, so `locate` returned the
/// pocket with stale (empty) `core_paths` instead of falling through to the
/// fresh on-disk manifest.
#[test]
fn stale_cache_manifest_hash_does_not_shadow_fresh_disk_manifest() {
    let env = TestEnv::new("stale-cache-no-shadow");
    let project = env.project("project");
    let templates = env.project("templates");

    fs::create_dir_all(env.pocket_root()).unwrap();
    fs::create_dir_all(env.legacy_safe_pocket_root()).unwrap();

    // The pocket's directory name is the hash of `[project]` alone, which is
    // also the pocket's pre-augment manifest hash. The current on-disk
    // manifest has been augmented to include `templates` and now has a
    // different hash.
    let pre_augment_hash = "feedfacefeed";
    let pocket_dir = env.legacy_safe_pocket_root().join(pre_augment_hash);
    write_synthetic_pocket(
        &pocket_dir,
        "postaugmenthash",
        &[project.clone(), templates.clone()],
        Some(pre_augment_hash),
        Some(pre_augment_hash),
    );

    // Stale cache in the legacy root: claims manifest_hash == dir name (the
    // pre-augment hash) and empty core_paths.
    write_synthetic_cache(
        &env.legacy_safe_pocket_root(),
        &[(pre_augment_hash, pre_augment_hash, &pocket_dir, &[])],
    );

    // Empty cache in the primary root so the legacy root is the only one that
    // knows about this pocket.
    write_synthetic_cache(&env.pocket_root(), &[]);

    // `locate --path project` computes `hash_paths([project])` and asks the
    // cache for a matching manifest_hash. The stale cache claims a match, but
    // the on-disk manifest disagrees. The lookup must verify against disk and
    // skip the stale entry, then find the pocket via the path-containing
    // fallback with the FRESH core_paths.
    let locate = env.run_spocket(
        &project,
        &["locate", "--path", project.to_string_lossy().as_ref()],
    );
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    let pocket_dir_resolved = PathBuf::from(value.get("pocket_dir").unwrap().as_str().unwrap());
    assert_eq!(pocket_dir_resolved, pocket_dir);

    let core_paths = value.get("core_paths").unwrap().as_array().unwrap();
    let resolved_paths: Vec<PathBuf> = core_paths
        .iter()
        .map(|v| PathBuf::from(v.as_str().unwrap()))
        .collect();
    assert!(
        resolved_paths.contains(&canon(&project)),
        "expected fresh core_paths to include project, got {resolved_paths:?}"
    );
    assert!(
        resolved_paths.contains(&canon(&templates)),
        "expected fresh core_paths to include templates, got {resolved_paths:?}"
    );
    assert_eq!(
        resolved_paths.len(),
        2,
        "expected fresh manifest's core_paths, got {resolved_paths:?}"
    );
}

/// `pocket sync-registry` rewrites every registry root's cache from the
/// on-disk manifests, collapsing split-brain duplicates into a single
/// canonical entry per hash. The pocket with more user content wins.
#[test]
fn sync_registry_rebuilds_caches_and_collapses_split_brain() {
    let env = TestEnv::new("sync-registry-rebuild");
    let project = env.project("project");

    fs::create_dir_all(env.pocket_root()).unwrap();
    fs::create_dir_all(env.legacy_safe_pocket_root()).unwrap();

    let hash = "deadbeefdead";
    let legacy_pocket = env.legacy_safe_pocket_root().join(hash);
    let primary_pocket = env.pocket_root().join(hash);

    // Rich pocket in the primary root with lots of user content.
    write_synthetic_pocket(
        &primary_pocket,
        "richhash00000",
        &[project.clone()],
        Some("other00000000"),
        Some(hash),
    );
    fs::create_dir_all(primary_pocket.join("FEATURES/dailies")).unwrap();
    for i in 0..5 {
        fs::write(
            primary_pocket
                .join("FEATURES/dailies")
                .join(format!("day_{i}.md")),
            "content",
        )
        .unwrap();
    }

    // Empty pocket in the legacy root with birth_hash matching the dir name
    // but almost no content.
    write_synthetic_pocket(
        &legacy_pocket,
        "emptyhash00000",
        &[project.clone()],
        Some(hash),
        Some(hash),
    );
    fs::create_dir_all(legacy_pocket.join("FEATURES")).unwrap();
    fs::write(legacy_pocket.join("FEATURES/00.md"), "minimal").unwrap();

    // Both caches start with stale entries.
    write_synthetic_cache(
        &env.legacy_safe_pocket_root(),
        &[(hash, hash, &legacy_pocket, &[])],
    );
    write_synthetic_cache(
        &env.pocket_root(),
        &[(hash, "richhash00000", &primary_pocket, &[project.clone()])],
    );

    let output = env.run_spocket(&project, &["sync-registry"]);
    assert_success(&output);

    // After rebuild, only ONE entry for `hash` should survive across both
    // caches: the rich primary pocket (more user content).
    let primary_cache = fs::read_to_string(env.pocket_root().join("registry_cache.json")).unwrap();
    let primary_value: serde_json::Value = serde_json::from_str(&primary_cache).unwrap();
    let primary_entries = primary_value
        .get("pockets")
        .and_then(|v| v.as_array())
        .unwrap();
    let primary_matches: Vec<&serde_json::Value> = primary_entries
        .iter()
        .filter(|v| v.get("hash").and_then(|h| h.as_str()) == Some(hash))
        .collect();
    assert!(
        primary_matches.len() <= 1,
        "expected at most one entry for {hash} in primary cache, got {}",
        primary_matches.len()
    );

    let legacy_cache =
        fs::read_to_string(env.legacy_safe_pocket_root().join("registry_cache.json")).unwrap();
    let legacy_value: serde_json::Value = serde_json::from_str(&legacy_cache).unwrap();
    let legacy_entries = legacy_value
        .get("pockets")
        .and_then(|v| v.as_array())
        .unwrap();
    let legacy_matches: Vec<&serde_json::Value> = legacy_entries
        .iter()
        .filter(|v| v.get("hash").and_then(|h| h.as_str()) == Some(hash))
        .collect();
    assert!(
        legacy_matches.len() <= 1,
        "expected at most one entry for {hash} in legacy cache, got {}",
        legacy_matches.len()
    );

    // locate should now resolve to the rich primary pocket.
    let locate = env.run_spocket(
        &project,
        &["locate", "--path", project.to_string_lossy().as_ref()],
    );
    assert_success(&locate);
    let locate_value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    let resolved = PathBuf::from(locate_value.get("pocket_dir").unwrap().as_str().unwrap());
    assert_eq!(resolved, primary_pocket);
    let core_paths = locate_value.get("core_paths").unwrap().as_array().unwrap();
    assert_eq!(core_paths.len(), 1);
    assert_eq!(
        PathBuf::from(core_paths[0].as_str().unwrap()),
        canon(&project)
    );
}

/// When `pocket augment` updates a pocket, the other registry root's cache
/// must not keep a stale duplicate entry for the same hash. This is the
/// split-brain propagation path: the augment prunes the duplicate so future
/// lookups cannot pick the wrong pocket. The pocket with more user content
/// wins the dedupe.
#[test]
fn augment_in_legacy_root_prunes_duplicate_from_primary_cache() {
    let env = TestEnv::new("augment-prunes-duplicate");
    let project = env.project("project");
    let extra = env.project("extra");

    // Both roots exist; the primary root takes precedence for new pockets.
    fs::create_dir_all(env.pocket_root()).unwrap();
    fs::create_dir_all(env.legacy_safe_pocket_root()).unwrap();

    // Seed a split-brain: same hash in both roots. The primary pocket has
    // more user content, so it wins the dedupe.
    let hash = "cafebabecafe";
    let legacy_pocket = env.legacy_safe_pocket_root().join(hash);
    let primary_pocket = env.pocket_root().join(hash);

    // Primary pocket: rich (more user content).
    write_synthetic_pocket(
        &primary_pocket,
        "primaryhash0",
        &[project.clone()],
        Some("other00000000"),
        Some(hash),
    );
    fs::create_dir_all(primary_pocket.join("FEATURES/dailies")).unwrap();
    for i in 0..5 {
        fs::write(
            primary_pocket
                .join("FEATURES/dailies")
                .join(format!("day_{i}.md")),
            "content",
        )
        .unwrap();
    }

    // Legacy pocket: empty (less user content), birth_hash matches dir name.
    write_synthetic_pocket(
        &legacy_pocket,
        "legacyhash000",
        &[project.clone()],
        Some(hash),
        Some(hash),
    );
    fs::create_dir_all(legacy_pocket.join("FEATURES")).unwrap();
    fs::write(legacy_pocket.join("FEATURES/00.md"), "minimal").unwrap();

    write_synthetic_cache(
        &env.legacy_safe_pocket_root(),
        &[(hash, "legacyhash000", &legacy_pocket, &[project.clone()])],
    );
    write_synthetic_cache(
        &env.pocket_root(),
        &[(hash, "primaryhash0", &primary_pocket, &[project.clone()])],
    );

    // Run `pocket augment --add extra` from the project directory. The lookup
    // should resolve to the primary pocket (more user content), augment it in
    // place, and prune the duplicate entry from the legacy root's cache.
    let augment = env.run_spocket(
        &project,
        &[
            "augment",
            "--add",
            extra.to_string_lossy().as_ref(),
            "--no-open",
        ],
    );
    assert_success(&augment);

    // The augment upserts to the preferred root (primary), so the primary
    // cache KEEPS an entry for this hash — but it points at the primary
    // pocket with the AUGMENTED core_paths, not the stale pre-augment entry.
    let primary_cache = fs::read_to_string(env.pocket_root().join("registry_cache.json")).unwrap();
    assert!(
        !primary_cache
            .contains(&format!("\"path\": \"{}\"", legacy_pocket.display())),
        "expected primary cache to have no entry pointing at the legacy pocket, got:\n{primary_cache}"
    );

    // The legacy root's cache should be pruned of the duplicate hash so a
    // future `load_cache_or_rebuild` cannot resurrect the stale entry.
    let legacy_cache =
        fs::read_to_string(env.legacy_safe_pocket_root().join("registry_cache.json")).unwrap();
    assert!(
        !legacy_cache.contains(&format!("\"hash\": \"{hash}\"")),
        "expected legacy cache to be pruned of hash {hash} after augment upsert, got:\n{legacy_cache}"
    );

    // locate should resolve to the primary pocket with the augmented core_paths.
    let locate = env.run_spocket(
        &project,
        &["locate", "--path", project.to_string_lossy().as_ref()],
    );
    assert_success(&locate);
    let locate_value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    let resolved = PathBuf::from(locate_value.get("pocket_dir").unwrap().as_str().unwrap());
    assert_eq!(resolved, primary_pocket);
    let core_paths = locate_value.get("core_paths").unwrap().as_array().unwrap();
    let resolved_paths: Vec<PathBuf> = core_paths
        .iter()
        .map(|v| PathBuf::from(v.as_str().unwrap()))
        .collect();
    assert!(resolved_paths.contains(&canon(&project)));
    assert!(resolved_paths.contains(&canon(&extra)));
}

#[test]
fn installed_real_world_harness_passes_in_isolation() {
    let env = TestEnv::new("real-world-harness");
    let cwd = env.project("command-cwd");

    let output = env.run_spocket(&cwd, &["tests", "--all"]);
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Installed-binary real-world tests"));
    assert!(stdout.contains("14"));
    assert!(stdout.contains("passed,"));
    assert!(stdout.contains("0"));
    assert!(stdout.contains("failed,"));
    assert!(stdout.contains("3"));
    assert!(stdout.contains("skipped"));
    assert!(stdout.contains("Placed edits never reverse-sync"));
    assert!(stdout.contains("pocket -u upgrade semantics"));
    assert!(stdout.contains("Bulk clean commands"));
    assert!(stdout.contains("intentionally never invoked"));
    assert!(stdout.contains("Removed isolated test fixture"));
}

#[test]
fn literal_root_artifact_cleanup_backs_up_and_removes_only_safe_shape() {
    let env = TestEnv::new("literal-root-cleanup");
    let cwd = env.project("command-cwd");
    let safe = env.pocket_root().join("abc123/{{SPOCKET_CONFIG_ROOT}}");
    let unsafe_dir = env.pocket_root().join("def456/{{SPOCKET_CONFIG_ROOT}}");
    fs::create_dir_all(&safe).unwrap();
    fs::create_dir_all(&unsafe_dir).unwrap();
    fs::write(safe.join("feature_tags.yaml"), "safe: true\n").unwrap();
    fs::write(unsafe_dir.join("feature_tags.yaml"), "safe: false\n").unwrap();
    fs::write(unsafe_dir.join("do-not-delete.txt"), "important\n").unwrap();

    let output = env.run_spocket(
        &cwd,
        &[
            "upgrade-installation",
            "--clean-literal-root-artifacts",
            "--yes",
        ],
    );
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("unexpected contents; left untouched"));
    assert!(!safe.exists(), "safe artifact directory should be removed");
    assert!(unsafe_dir.is_dir(), "unsafe artifact directory must remain");
    assert!(unsafe_dir.join("do-not-delete.txt").is_file());

    let backups = env.pocket_root().join("upgrade-backups");
    let backup_file_count = fs::read_dir(backups)
        .unwrap()
        .filter_map(Result::ok)
        .flat_map(|entry| fs::read_dir(entry.path()).unwrap().filter_map(Result::ok))
        .filter(|entry| entry.path().join("feature_tags.yaml").is_file())
        .count();
    assert_eq!(backup_file_count, 1, "safe artifact must be backed up once");
}

// NOTE: unregistered-project coverage for `locate --read-only` lives in
// `read_only_locate_creates_no_config_or_registry_state` below, which also
// asserts the `read_only` flag in the JSON payload.

#[test]
fn real_world_include_supports_unregistered_project_without_mutating_it() {
    let env = TestEnv::new("real-world-unregistered-include");
    let project = env.project("unregistered-project");
    fs::write(project.join("keep.txt"), "unchanged\n").unwrap();

    let output = env.run_spocket(
        &project,
        &["tests", "--all", "-i", project.to_string_lossy().as_ref()],
    );
    assert_success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("supplied project has no registered pocket"));
    assert!(stdout.contains("project is unregistered"));
    assert_eq!(
        fs::read_to_string(project.join("keep.txt")).unwrap(),
        "unchanged\n"
    );
    assert!(!project.join(".env").exists());

    // `-i` intentionally retains the harness-owned fixture for users. This test
    // removes only that isolated fixture after proving the retention behavior.
    if let Some(line) = stdout
        .lines()
        .find(|line| line.contains("Retained real-world test fixture:"))
    {
        if let Some(path) = line.split_whitespace().last() {
            let path = PathBuf::from(path);
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with("left_pocket-real-world-"))
                .unwrap_or(false)
            {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
}

/// `locate --read-only` exists so audits and the post-install harness can ask
/// "which pocket owns this?" without the question itself mutating state. An
/// unregistered project is the sharpest version of that promise: nothing about
/// it should cause a config root, a registry root, or a cache to spring into
/// existence.
#[test]
fn read_only_locate_creates_no_config_or_registry_state() {
    let env = TestEnv::new("locate-read-only");
    let project = env.project("unregistered");
    let mut summary = TestSummary::new(
        "read_only_locate_creates_no_config_or_registry_state",
        "`locate --read-only` audits an unregistered project without creating config, templates, or registry state",
        "the temporary HOME is deleted wholesale when the test environment drops",
    );

    let output = env.run_spocket(&project, &["locate", "--read-only", "--path", "."]);
    assert_success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value.get("status").and_then(|v| v.as_str()),
        Some("not_found")
    );
    assert_eq!(value.get("read_only").and_then(|v| v.as_bool()), Some(true));
    summary.step(
        "Ran `pocket locate --read-only` against a project with no registered pocket".to_string(),
    );

    assert!(
        !env.config_root().exists(),
        "read-only locate created a config root at {}",
        env.config_root().display()
    );
    assert!(
        !env.pocket_root().exists(),
        "read-only locate created a registry root at {}",
        env.pocket_root().display()
    );
    summary.step("Verified neither the config root nor the registry root was created".to_string());
    summary.print();
}

/// The read-only path must not rebuild the registry cache even when one is
/// missing. Deleting the cache first and contrasting the two code paths is what
/// makes this meaningful: the normal path is still allowed to rebuild.
/// Regression: `locate --read-only` used to return the first manifest whose
/// project path was a *prefix* of the requested path, so a pocket registered for
/// `~/dev/bin` won over the pocket for `~/dev/bin/app`. Because `pocket tests -i`
/// uses this to decide what to back up and clone, resolving an ancestor meant
/// auditing the wrong pocket. The most specific match must win.
#[test]
fn read_only_locate_prefers_the_most_specific_project_match() {
    let env = TestEnv::new("locate-read-only-specific");
    let parent = env.project("workspace");
    let child = parent.join("nested-app");
    fs::create_dir_all(&child).unwrap();

    // Register the ancestor first so it is the older/earlier candidate.
    assert_success(&env.run_spocket(&parent, &["-i", ".", "--silent"]));
    assert_success(&env.run_spocket(&child, &["-i", ".", "--silent"]));

    let normal = env.run_spocket(&child, &["locate", "--path", "."]);
    assert_success(&normal);
    let normal_value: serde_json::Value = serde_json::from_slice(&normal.stdout).unwrap();
    let expected = normal_value.get("pocket_dir").unwrap().as_str().unwrap();

    let read_only = env.run_spocket(&child, &["locate", "--read-only", "--path", "."]);
    assert_success(&read_only);
    let value: serde_json::Value = serde_json::from_slice(&read_only.stdout).unwrap();
    assert_eq!(
        value.get("pocket_dir").and_then(|v| v.as_str()),
        Some(expected),
        "read-only locate must agree with normal locate, not resolve the ancestor project"
    );
}

#[test]
fn read_only_locate_does_not_rebuild_the_registry_cache() {
    let env = TestEnv::new("locate-read-only-cache");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "read_only_locate_does_not_rebuild_the_registry_cache",
        "`locate --read-only` resolves a registered pocket without writing a registry cache",
        "the temporary HOME is deleted wholesale when the test environment drops",
    );

    assert_success(&env.run_spocket(&project, &["-i", ".", "--silent"]));
    let pocket = env.only_pocket();
    let cache = env.registry_file("registry_cache.json");
    if cache.exists() {
        fs::remove_file(&cache).expect("failed to remove registry cache");
    }
    summary.step("Created a pocket, then deleted the registry cache".to_string());

    let output = env.run_spocket(&project, &["locate", "--read-only", "--path", "."]);
    assert_success(&output);
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value.get("pocket_dir").and_then(|v| v.as_str()),
        Some(pocket.to_string_lossy().as_ref())
    );
    assert!(
        !cache.exists(),
        "read-only locate rebuilt the registry cache at {}",
        cache.display()
    );
    summary.step(
        "Read-only locate resolved the pocket straight from the manifest, writing no cache"
            .to_string(),
    );

    assert_success(&env.run_spocket(&project, &["locate", "--path", "."]));
    assert!(
        cache.exists(),
        "the normal locate path should still rebuild the registry cache"
    );
    summary
        .step("Confirmed the contrast: the normal locate path does rebuild the cache".to_string());
    summary.print();
}
