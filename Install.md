```
+-------------------+
| left_pocket       |
+-------------------+
|                   |
|                   |
|                   |
|                   |
 \                 /
  `---------------'
```

# How to install

1. Compile VS Code extension at {{PROJECT_ROOT}}/vscode-extension
2. Install VS Code extension that this generates
3. run `{{PROJECT_ROOT}}/install.sh`

You can also use `cargo install --path .` to install the left_pocket CLI tool, but this is not necessary if you run the install.sh script, which will do this for you. The install script installs the primary `left_pocket` binary plus compatibility aliases for `safe_pocket` and `spocket`, seeds default assets into `$HOME/.config/left_pocket/`, and creates the canonical left_pocket registry root at `$HOME/.left_pocket/`.

## Installer options

```bash
./install.sh --help
./install.sh --bump-version                 # patch
./install.sh --bump-version minor
./install.sh --bump-version 3.0.0            # exact version
./install.sh --bump-extension-version patch
./install.sh --set-version 3.0.0
./install.sh --set-extension-version 3.0.0
```

`--bump-version` edits `Cargo.toml`; `--bump-extension-version` edits
`vscode-extension/package.json` and packages the extension. Both accept
`major`, `minor`, `patch`, or an exact `X.Y.Z` version, and default to `patch`
when no value follows. The explicit `--set-*` forms require an exact version.
Legacy `bump`, `--app(...)`, and `--extension(...)` syntax remains supported.

The installer asks before replacing existing config templates. It also checks
`origin/master` on every run: if the local checkout is behind, it offers to
pull the latest master changes and re-run the updated `install.sh` before
building, so staying current does not require a shipped binary. After installing,
run the installed-binary operational suite:

```bash
left_pocket tests --all
```
