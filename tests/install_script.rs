use std::fs;
use std::path::PathBuf;
use std::process::Command;
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
