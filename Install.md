# How to install

1. Compile VS Code extension at {{PROJECT_ROOT}}/vscode-extension
2. Install VS Code extension that this generates
3. run {{PROJECT_ROOT}}/install.sh

You can also use `cargo install --path .` to install the Corner CLI tool, but this is not necessary if you run the install.sh script, which will do this for you. The install script installs the primary `corner` binary plus compatibility aliases for `safe_pocket` and `spocket`, and it also performs setup such as creating a default `feature_tags.yaml` file when needed.
