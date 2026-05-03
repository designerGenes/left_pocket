use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{SystemTime, UNIX_EPOCH};

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
        fs::write(&code_path, "#!/bin/sh\nexit 0\n").expect("failed to write fake code command");
        fs::set_permissions(&code_path, fs::Permissions::from_mode(0o755))
            .expect("failed to make fake code executable");

        let bd_path = bin_dir.join("bd");
        fs::write(
            &bd_path,
            "#!/bin/sh\nif [ \"$1\" = \"init\" ]; then mkdir -p .beads; exit 0; fi\nexit 0\n",
        )
        .expect("failed to write fake bd command");
        fs::set_permissions(&bd_path, fs::Permissions::from_mode(0o755))
            .expect("failed to make fake bd executable");

        let existing_path = std::env::var("PATH").unwrap_or_default();
        let path = format!("{}:{existing_path}", bin_dir.display());

        Self {
            root,
            home,
            path,
        }
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
            .env("GIT_CONFIG_GLOBAL", self.root.join("gitconfig"))
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .expect("failed to run safe_pocket")
    }

    fn safe_pockets(&self) -> Vec<PathBuf> {
        let registry = self.home.join(".safe_pocket");
        if !registry.exists() {
            return Vec::new();
        }

        let mut pockets = fs::read_dir(registry)
            .expect("failed to read safe pocket registry")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .filter(|path| path.join("manifest.json").is_file())
            .collect::<Vec<_>>();
        pockets.sort();
        pockets
    }

    fn only_pocket(&self) -> PathBuf {
        let pockets = self.safe_pockets();
        assert_eq!(pockets.len(), 1, "expected exactly one safe pocket");
        pockets.into_iter().next().expect("safe pocket should exist")
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
    let value: serde_json::Value = serde_json::from_str(&manifest).expect("failed to parse manifest");
    value
        .get("uses_beads")
        .and_then(|value| value.as_bool())
        .expect("manifest should contain uses_beads")
}

#[test]
fn new_pocket_defaults_to_beads() {
    let env = TestEnv::new("default");
    let project = env.project("project");

    let output = env.run_spocket(&project, &["-i", "."]);
    assert_success(&output);

    let pocket = env.only_pocket();
    assert!(pocket.join(".beads").is_dir());
    assert!(project.join(".beads").join("redirect").is_file());
    assert!(manifest_uses_beads(&pocket));
}

#[test]
fn new_pocket_with_without_beads_does_not_initialize_beads() {
    let env = TestEnv::new("without");
    let project = env.project("project");

    let output = env.run_spocket(&project, &["-i", ".", "--without-beads"]);
    assert_success(&output);

    let pocket = env.only_pocket();
    assert!(!pocket.join(".beads").exists());
    assert!(!project.join(".beads").exists());
    assert!(!manifest_uses_beads(&pocket));
}

#[test]
fn existing_non_beads_pocket_is_not_auto_upgraded_on_reuse() {
    let env = TestEnv::new("reuse-without");
    let project = env.project("project");

    assert_success(&env.run_spocket(&project, &["-i", ".", "--without-beads"]));
    assert_success(&env.run_spocket(&project, &["-i", "."]));

    let pocket = env.only_pocket();
    assert!(!pocket.join(".beads").exists());
    assert!(!project.join(".beads").exists());
    assert!(!manifest_uses_beads(&pocket));
}

#[test]
fn existing_non_beads_pocket_can_be_explicitly_upgraded() {
    let env = TestEnv::new("upgrade");
    let project = env.project("project");

    assert_success(&env.run_spocket(&project, &["-i", ".", "--without-beads"]));
    assert_success(&env.run_spocket(&project, &["-i", ".", "--use", "beads"]));

    let pocket = env.only_pocket();
    assert!(pocket.join(".beads").is_dir());
    assert!(project.join(".beads").join("redirect").is_file());
    assert!(manifest_uses_beads(&pocket));
}

#[test]
fn without_beads_wins_over_use_beads_for_new_pocket() {
    let env = TestEnv::new("conflict");
    let project = env.project("project");

    let output = env.run_spocket(&project, &["-i", ".", "--use", "beads", "--without-beads"]);
    assert_success(&output);

    let pocket = env.only_pocket();
    assert!(!pocket.join(".beads").exists());
    assert!(!project.join(".beads").exists());
    assert!(!manifest_uses_beads(&pocket));
}

#[test]
fn unknown_feature_still_fails_without_beads_initialization() {
    let env = TestEnv::new("unknown");
    let project = env.project("project");

    let output = env.run_spocket(&project, &["-i", ".", "--use", "memvid"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Unknown feature 'memvid'")
            || String::from_utf8_lossy(&output.stdout).contains("Unknown feature 'memvid'")
    );
    assert!(!project.join(".beads").exists());
    assert!(env.safe_pockets().is_empty());
}
