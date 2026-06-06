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

    fn workspace_file(&self) -> PathBuf {
        let pocket = self.only_pocket();
        let name = pocket.file_name().unwrap().to_string_lossy().to_string();
        pocket.join(format!("{name}.code-workspace"))
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
            "{{\n  \"folders\": [\n    {{\n      \"path\": \"{}\",\n      \"name\": \"[Safe Pocket] {name}\"\n    }}\n  ]\n}}\n",
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
    assert!(workspace_text.contains("[Safe Pocket]"));

    let locate = env.run_spocket(&project, &["locate", "--path", "."]);
    assert_success(&locate);
    let value: serde_json::Value = serde_json::from_slice(&locate.stdout).unwrap();
    assert_eq!(
        value.get("pocket_dir").and_then(|v| v.as_str()),
        Some(pocket.to_string_lossy().as_ref())
    );
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
    summary.step("Opened a new workspace with `spocket -i .`".to_string());

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
    assert!(project_env.contains("SPOCKET_ROOT="));
    assert!(!project_env.contains("BEADS_DIR="));
    summary.step("Verified `.env` carries SPOCKET_ROOT and no BEADS_DIR line".to_string());
    summary.print();
}

/// AGENTS.md should advertise the built-in `spocket task` tracker at runtime.
#[test]
fn runtime_agents_md_advertises_task_tracker() {
    let env = TestEnv::new("task-block");
    let project = env.project("project");
    let mut summary = TestSummary::new(
        "runtime_agents_md_advertises_task_tracker",
        "runtime merge injects the spocket task guidance block into AGENTS.md",
        "the temporary HOME and project tree are deleted on drop",
    );

    let output = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--simulate-runtime", "--silent"],
    );
    assert_success(&output);
    summary.step("Ran `spocket -i . --temporary --simulate-runtime --silent`".to_string());

    let pocket = env.only_pocket();
    let agents = fs::read_to_string(pocket.join("AGENTS.md")).unwrap();
    assert!(
        agents.contains("spocket task"),
        "AGENTS.md should mention the built-in task tracker"
    );
    assert!(
        !agents.contains("bd ready"),
        "AGENTS.md should not mention beads"
    );
    summary.step("Verified AGENTS.md mentions `spocket task` and not beads".to_string());
    summary.print();
}

#[test]
fn missing_template_destination_warning_is_verbose_only() {
    let env = TestEnv::new("verbose-template-warning");
    let project = env.project("project");
    let template_dir = env.home.join(".config/safe_pocket/templates/feature_tags");
    fs::create_dir_all(&template_dir).unwrap();
    fs::write(
        template_dir.join("conversation.feature.tag.yaml"),
        "description: this is referenced by feature_tags.yaml, not a pocket template\n",
    )
    .unwrap();

    let quiet = env.run_spocket(&project, &["-i", ".", "--temporary", "--silent"]);
    assert_success(&quiet);
    assert!(
        !String::from_utf8_lossy(&quiet.stderr).contains("missing #SPOCKET_TEMPLATE_DESTINATION"),
        "non-verbose run should not warn about referenced feature tag files"
    );

    let other = env.project("other-project");
    let verbose = env.run_spocket(&other, &["-i", ".", "--temporary", "--silent", "--verbose"]);
    assert_success(&verbose);
    assert!(
        String::from_utf8_lossy(&verbose.stderr).contains("missing #SPOCKET_TEMPLATE_DESTINATION"),
        "verbose run should surface skipped template diagnostics"
    );
}

#[test]
fn daily_feature_loads_auto_tags_from_feature_tags_yaml() {
    let env = TestEnv::new("feature-tags-yaml");
    let project = env.project("project");
    fs::create_dir_all(env.home.join(".config/safe_pocket")).unwrap();
    fs::write(
        env.home.join(".config/safe_pocket/feature_tags.yaml"),
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
        config.get("bolt_url").and_then(|v| v.as_str()),
        Some("bolt://127.0.0.1:7687")
    );

    let workspace_text = fs::read_to_string(env.workspace_file()).unwrap();
    assert!(workspace_text.contains("[Tool] memgraph"));
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
            "{{\n  \"folders\": [\n    {{\n      \"path\": \"{}\",\n      \"name\": \"[Safe Pocket] {name}\"\n    }}\n  ]\n}}\n",
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
    assert!(workspace_text.contains("[Safe Pocket]"));
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
            "{{\n  \"folders\": [\n    {{\n      \"path\": \"{}\",\n      \"name\": \"[Safe Pocket] {name}\"\n    }}\n  ]\n}}\n",
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
            "{{\n  \"folders\": [\n    {{\n      \"path\": \"{}\",\n      \"name\": \"[Safe Pocket] {name}\"\n    }}\n  ]\n}}\n",
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
    let pocket = env.only_pocket();
    let config_path = pocket.join("tools/memgraph/scan-config.json");
    fs::write(&config_path, "sentinel").unwrap();

    let second = env.run_spocket(
        &project,
        &["-i", ".", "--temporary", "--add", "memgraph", "--silent"],
    );
    assert_success(&second);
    assert_eq!(fs::read_to_string(&config_path).unwrap(), "sentinel");
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

/// When `heal` renames a safe pocket directory, the built-in task tracker must
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
