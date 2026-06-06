# How to install

1. Compile VS Code extension at {{PROJECT_ROOT}}/vscode-extension
2. Install VS Code extension that this generates
3. run {{PROJECT_ROOT}}/install.sh

You can also use `cargo install --path .` to install the spocket CLI tool, but this is not necessary if you run the install.sh script, which will do this for you.  The install.sh script also does some additional setup, such as creating a default feature_tags.yaml file if one does not already exist.
