
#CORNER_TEMPLATE_DESTINATION: {{CORNER_ROOT}}/.github/copilot-instructions.md
#CORNER_MERGE_AT_RUNTIME

All coding projects should include a top-level install.sh script, which handles the full process of installing the compiled project to a location that can be called from any terminal.  Usually this is $HOME/.local/bin.  The install process should use the standard install process for the language used (for example, for a Rust project we should use cargo install, and for a python project we should use uv).

The install script should accept an optional --bump-version flag or manual --set-version flag which should be followed with a specific version number.  If present, these should update the version inside the project's Cargo.toml or similar file before installing. 
