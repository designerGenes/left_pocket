use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn temp_root(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("left_pocket-install-test-{name}-{unique}"))
}

#[test]
fn install_help_exits_without_building() {
    let output = Command::new("bash")
        .arg(project_root().join("install.sh"))
        .arg("--help")
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--bump-version"));
    assert!(stdout.contains("--bump-extension-version"));
    assert!(stdout.contains("--set-version"));
    assert!(!stdout.contains("Building left_pocket"));
}

#[test]
fn version_helper_bumps_app_and_extension_independently() {
    let root = temp_root("bumps");
    fs::create_dir_all(root.join("vscode-extension")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"left_pocket-test\"\nversion = \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        root.join("vscode-extension/package.json"),
        "{\"name\":\"left_pocket-test\",\"version\":\"4.5.6\"}\n",
    )
    .unwrap();

    let output = Command::new("python3")
        .arg(project_root().join("scripts/bump_versions.py"))
        .args(["--root", root.to_string_lossy().as_ref()])
        .args(["--app", "minor"])
        .args(["--extension", "patch"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fs::read_to_string(root.join("Cargo.toml"))
        .unwrap()
        .contains("version = \"1.3.0\""));
    assert!(
        fs::read_to_string(root.join("vscode-extension/package.json"))
            .unwrap()
            .contains("\"version\": \"4.5.7\"")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn version_helper_accepts_exact_semver() {
    let root = temp_root("exact");
    fs::create_dir_all(root.join("vscode-extension")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"left_pocket-test\"\nversion = \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        root.join("vscode-extension/package.json"),
        "{\"name\":\"left_pocket-test\",\"version\":\"4.5.6\"}\n",
    )
    .unwrap();

    let output = Command::new("python3")
        .arg(project_root().join("scripts/bump_versions.py"))
        .args(["--root", root.to_string_lossy().as_ref()])
        .args(["--set-app", "9.8.7"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(fs::read_to_string(root.join("Cargo.toml"))
        .unwrap()
        .contains("version = \"9.8.7\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn set_flags_reject_non_semver_before_building() {
    let cargo = project_root().join("Cargo.toml");
    let before = fs::read_to_string(&cargo).unwrap();
    let output = Command::new("bash")
        .arg(project_root().join("install.sh"))
        .args(["--set-version", "patch"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("exact SemVer"));
    assert_eq!(fs::read_to_string(cargo).unwrap(), before);
}

#[test]
fn help_after_optional_bump_does_not_mutate_or_build() {
    let cargo = project_root().join("Cargo.toml");
    let before = fs::read_to_string(&cargo).unwrap();
    let output = Command::new("bash")
        .arg(project_root().join("install.sh"))
        .args(["--bump-version", "-h"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Building left_pocket"));
    assert_eq!(fs::read_to_string(cargo).unwrap(), before);
}

#[test]
fn invalid_extension_request_does_not_partially_update_app() {
    let root = temp_root("atomic");
    fs::create_dir_all(root.join("vscode-extension")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"left_pocket-test\"\nversion = \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        root.join("vscode-extension/package.json"),
        "{\"name\":\"left_pocket-test\",\"version\":\"4.5.6\"}\n",
    )
    .unwrap();

    let output = Command::new("python3")
        .arg(project_root().join("scripts/bump_versions.py"))
        .args(["--root", root.to_string_lossy().as_ref()])
        .args(["--app", "minor"])
        .args(["--extension", "not-a-version"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(fs::read_to_string(root.join("Cargo.toml"))
        .unwrap()
        .contains("version = \"1.2.3\""));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn atomic_updates_roll_back_after_injected_second_step_failure() {
    let root = temp_root("rollback");
    fs::create_dir_all(root.join("vscode-extension")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"left_pocket-test\"\nversion = \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        root.join("vscode-extension/package.json"),
        "{\"name\":\"left_pocket-test\",\"version\":\"4.5.6\"}\n",
    )
    .unwrap();

    let output = Command::new("python3")
        .arg(project_root().join("scripts/bump_versions.py"))
        .args(["--root", root.to_string_lossy().as_ref()])
        .args(["--app", "minor"])
        .args(["--extension", "patch"])
        .env("LEFT_POCKET_BUMP_FAIL_AFTER", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(fs::read_to_string(root.join("Cargo.toml"))
        .unwrap()
        .contains("version = \"1.2.3\""));
    assert!(
        fs::read_to_string(root.join("vscode-extension/package.json"))
            .unwrap()
            .contains("\"version\":\"4.5.6\"")
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn empty_equals_bump_values_are_rejected_before_building() {
    for option in ["--bump-version=", "--bump-extension-version="] {
        let output = Command::new("bash")
            .arg(project_root().join("install.sh"))
            .arg(option)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("non-empty"));
    }
}

// ── origin/master update check ───────────────────────────────────────────────

fn git(repo: &Path, args: &[&str], config_global: &Path) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", config_global)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .unwrap()
}

fn git_ok(repo: &Path, args: &[&str], config_global: &Path) {
    let output = git(repo, args, config_global);
    assert!(
        output.status.success(),
        "git {:?} failed in {}\nstdout:\n{}\nstderr:\n{}",
        args,
        repo.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn write_stub_install(repo: &Path, marker: &str) {
    fs::write(
        repo.join("install.sh"),
        format!("#!/bin/bash\nprintf 'stub-install-{marker} args:%s\\n' \"$*\"\n"),
    )
    .unwrap();
}

fn write_stub_install_failing(repo: &Path, marker: &str, code: u32) {
    fs::write(
        repo.join("install.sh"),
        format!("#!/bin/bash\nprintf 'stub-install-{marker} args:%s\\n' \"$*\"\nexit {code}\n"),
    )
    .unwrap();
}

/// Build a local "GitHub" stand-in: a bare origin plus a clone whose
/// scripts/offer_master_update.sh is the real helper. The clone ends up one
/// commit behind origin/master because a second clone ("pusher") advances the
/// stub install.sh to v2 after the first clone was taken.
fn update_check_repos(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = temp_root(name);
    let config_global = root.join("gitconfig");
    let origin = root.join("origin.git");
    let pusher = root.join("pusher");
    let clone = root.join("clone");
    fs::create_dir_all(&root).unwrap();

    git_ok(&root, &["init", "--bare", origin.to_string_lossy().as_ref()], &config_global);
    git_ok(&root, &["clone", origin.to_string_lossy().as_ref(), pusher.to_string_lossy().as_ref()], &config_global);

    fs::create_dir_all(pusher.join("scripts")).unwrap();
    write_stub_install(&pusher, "v1");
    fs::copy(
        project_root().join("scripts/offer_master_update.sh"),
        pusher.join("scripts/offer_master_update.sh"),
    )
    .unwrap();
    git_ok(&pusher, &["add", "-A"], &config_global);
    git_ok(
        &pusher,
        &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-m", "v1"],
        &config_global,
    );
    git_ok(&pusher, &["push", "origin", "master"], &config_global);

    git_ok(&root, &["clone", origin.to_string_lossy().as_ref(), clone.to_string_lossy().as_ref()], &config_global);

    // Advance origin so the clone falls one commit behind.
    write_stub_install(&pusher, "v2");
    git_ok(&pusher, &["add", "-A"], &config_global);
    git_ok(
        &pusher,
        &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-m", "v2"],
        &config_global,
    );
    git_ok(&pusher, &["push", "origin", "master"], &config_global);

    (origin, clone, config_global)
}

fn run_update_check(clone: &Path, config_global: &Path, stdin_data: Option<&str>) -> Output {
    let mut command = Command::new("bash");
    command
        .arg(clone.join("scripts/offer_master_update.sh"))
        .arg(clone.join("install.sh"))
        .args(["--bump-version", "minor"])
        .env("GIT_CONFIG_GLOBAL", config_global)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin_data.is_some() {
        command.stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command.spawn().unwrap();
    if let Some(data) = stdin_data {
        child.stdin.as_mut().unwrap().write_all(data.as_bytes()).unwrap();
    }
    child.wait_with_output().unwrap()
}

#[test]
fn update_check_pulls_master_and_reexecs_updated_install() {
    let (origin, clone, config_global) = update_check_repos("accept");

    let output = run_update_check(&clone, &config_global, Some("y\n"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(10),
        "a successful re-run must report the sentinel 10 so the caller stops:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("behind origin/master"), "{stdout}");
    assert!(stdout.contains("Re-running the updated install.sh"), "{stdout}");
    assert!(
        stdout.contains("stub-install-v2 args:--bump-version minor"),
        "the re-run must run the pulled install.sh with the original args:\n{stdout}"
    );

    let head = git(&clone, &["rev-parse", "HEAD"], &config_global);
    let remote = git(&origin, &["rev-parse", "master"], &config_global);
    assert_eq!(
        String::from_utf8_lossy(&head.stdout).trim(),
        String::from_utf8_lossy(&remote.stdout).trim(),
        "accepting the prompt must fast-forward the clone to origin/master"
    );

    let _ = fs::remove_dir_all(clone.parent().unwrap());
}

#[test]
fn update_check_propagates_rerun_failure() {
    let (_origin, clone, config_global) = update_check_repos("rerun-failure");
    let pusher = clone.parent().unwrap().join("pusher");
    // Make the pulled install.sh fail; the helper must propagate its status
    // instead of reporting the success sentinel.
    write_stub_install_failing(&pusher, "v2", 3);
    git_ok(&pusher, &["add", "-A"], &config_global);
    git_ok(
        &pusher,
        &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-m", "v2 fails"],
        &config_global,
    );
    git_ok(&pusher, &["push", "origin", "master"], &config_global);

    let output = run_update_check(&clone, &config_global, Some("y\n"));
    assert_eq!(
        output.status.code(),
        Some(3),
        "a failing re-run must propagate its exit status:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(clone.parent().unwrap());
}

#[test]
fn update_check_decline_keeps_current_checkout() {
    let (_origin, clone, config_global) = update_check_repos("decline");

    let output = run_update_check(&clone, &config_global, Some("n\n"));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("behind origin/master"), "{stdout}");
    assert!(!stdout.contains("stub-install"), "{stdout}");

    let behind = git(&clone, &["rev-list", "--count", "HEAD..origin/master"], &config_global);
    assert_eq!(
        String::from_utf8_lossy(&behind.stdout).trim(),
        "1",
        "declining the prompt must leave the clone behind"
    );

    let _ = fs::remove_dir_all(clone.parent().unwrap());
}

#[test]
fn update_check_non_interactive_stdin_defaults_to_decline() {
    let (_origin, clone, config_global) = update_check_repos("noninteractive");

    let output = run_update_check(&clone, &config_global, None);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("behind origin/master"), "{stdout}");
    assert!(!stdout.contains("stub-install"), "{stdout}");

    let _ = fs::remove_dir_all(clone.parent().unwrap());
}

#[test]
fn install_runs_exactly_once_after_accepted_master_pull() {
    // End-to-end regression for the reported bug: after the update check pulls
    // origin/master and re-runs install.sh, the original install.sh must stop
    // instead of installing a second time (the template prompt appeared twice).
    let root = temp_root("e2e");
    let config_global = root.join("gitconfig");
    let home = root.join("home");
    let install_dir = root.join("install");
    let fake_bin = root.join("bin");
    let origin = root.join("origin.git");
    let pusher = root.join("pusher");
    let clone = root.join("clone");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&fake_bin).unwrap();

    // Fake cargo: "build" a stub left_pocket binary that accepts any args.
    fs::write(
        fake_bin.join("cargo"),
        "#!/bin/bash\nmkdir -p \"$PWD/target/release\"\nprintf '#!/bin/bash\\nexit 0\\n' > \"$PWD/target/release/left_pocket\"\nchmod +x \"$PWD/target/release/left_pocket\"\n",
    )
    .unwrap();
    fs::set_permissions(fake_bin.join("cargo"), fs::Permissions::from_mode(0o755)).unwrap();

    git_ok(&root, &["init", "--bare", origin.to_string_lossy().as_ref()], &config_global);
    git_ok(&root, &["clone", origin.to_string_lossy().as_ref(), pusher.to_string_lossy().as_ref()], &config_global);

    // The clone carries the REAL install.sh and helper.
    fs::create_dir_all(pusher.join("scripts")).unwrap();
    fs::copy(project_root().join("install.sh"), pusher.join("install.sh")).unwrap();
    fs::copy(
        project_root().join("scripts/offer_master_update.sh"),
        pusher.join("scripts/offer_master_update.sh"),
    )
    .unwrap();
    git_ok(&pusher, &["add", "-A"], &config_global);
    git_ok(
        &pusher,
        &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-m", "v1"],
        &config_global,
    );
    git_ok(&pusher, &["push", "origin", "master"], &config_global);

    git_ok(&root, &["clone", origin.to_string_lossy().as_ref(), clone.to_string_lossy().as_ref()], &config_global);

    // Advance origin by one innocuous commit so the clone falls behind.
    fs::write(pusher.join("MARKER"), "v2\n").unwrap();
    git_ok(&pusher, &["add", "-A"], &config_global);
    git_ok(
        &pusher,
        &["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-m", "v2"],
        &config_global,
    );
    git_ok(&pusher, &["push", "origin", "master"], &config_global);

    let existing_path = std::env::var("PATH").unwrap_or_default();
    let mut child = Command::new("bash")
        .arg(clone.join("install.sh"))
        .current_dir(&clone)
        .env("HOME", &home)
        .env("INSTALL_DIR", &install_dir)
        .env("PATH", format!("{}:{existing_path}", fake_bin.display()))
        .env("GIT_CONFIG_GLOBAL", &config_global)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(b"y\n").unwrap();
    let output = child.wait_with_output().unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "install.sh failed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Re-running the updated install.sh"), "{stdout}");
    assert_eq!(
        stdout.matches("Building left_pocket").count(),
        1,
        "the build must run exactly once, not once per install.sh generation:\n{stdout}"
    );
    assert_eq!(
        stdout.matches("Installation complete.").count(),
        1,
        "the install must complete exactly once (the double-install bug printed this twice):\n{stdout}"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn update_check_is_noop_when_up_to_date_or_not_a_repo() {    let root = temp_root("noop");
    let config_global = root.join("gitconfig");
    fs::create_dir_all(root.join("scripts")).unwrap();
    fs::copy(
        project_root().join("scripts/offer_master_update.sh"),
        root.join("scripts/offer_master_update.sh"),
    )
    .unwrap();

    // Not a git repo: exits cleanly and silently.
    let output = run_update_check(&root, &config_global, None);
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "{}", String::from_utf8_lossy(&output.stdout));

    // Up to date: exits cleanly and silently.
    let (_origin, clone, clone_config) = update_check_repos("noop-uptodate");
    git_ok(&clone, &["pull", "--ff-only", "origin", "master"], &clone_config);
    let output = run_update_check(&clone, &clone_config, None);
    assert!(output.status.success());
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("behind"),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(clone.parent().unwrap());
}
