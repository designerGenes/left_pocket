#!/bin/bash

set -e

# Parse arguments to see if a bump command or bump flags are present
BUMP_REQUESTED=false
for arg in "$@"; do
    if [ "$arg" = "bump" ] || [[ "$arg" == --app* ]] || [[ "$arg" == --extension* ]]; then
        BUMP_REQUESTED=true
    fi
done

if [ "$BUMP_REQUESTED" = true ]; then
    # Run the Python script to bump version files
    BUMP_OUTPUT=$(python3 - "$@" <<'EOF'
import sys
import re
from pathlib import Path

def parse_pattern(arg_val):
    match = re.search(r'\(([^)]+)\)', arg_val)
    if not match:
        return "..X"
    pat = match.group(1)
    if pat in ("X..", ".X.", "..X"):
        return pat
    return "..X"

def bump_version_string(version_str, pattern):
    parts = version_str.split('.')
    if len(parts) != 3:
        return version_str
    major, minor, patch = int(parts[0]), int(parts[1]), int(parts[2])
    if pattern == "X..":
        major += 1
    elif pattern == ".X.":
        minor += 1
    elif pattern == "..X":
        patch += 1
    return f"{major}.{minor}.{patch}"

def main():
    args = sys.argv[1:]
    # Check if we should bump app and/or extension
    bump_app = False
    bump_ext = False
    app_pattern = "..X"
    ext_pattern = "..X"
    
    for arg in args:
        if arg == "bump":
            continue
        if arg.startswith("--app"):
            bump_app = True
            app_pattern = parse_pattern(arg)
        elif arg.startswith("--extension"):
            bump_ext = True
            ext_pattern = parse_pattern(arg)
            
    if not bump_app and not bump_ext:
        bump_app = True
        
    if bump_ext and not bump_app:
        bump_app = True
        
    project_root = Path.cwd()
    
    if bump_app:
        cargo_toml_path = project_root / "Cargo.toml"
        if cargo_toml_path.exists():
            content = cargo_toml_path.read_text(encoding='utf-8')
            match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', content)
            if match:
                old_version = match.group(1)
                new_version = bump_version_string(old_version, app_pattern)
                content = content.replace(f'version = "{old_version}"', f'version = "{new_version}"', 1)
                cargo_toml_path.write_text(content, encoding='utf-8')
                print(f"Bumped app version in Cargo.toml from {old_version} to {new_version} (pattern: {app_pattern})")
            else:
                print("Warning: could not find version field in Cargo.toml")
                
    if bump_ext:
        package_json_path = project_root / "vscode-extension" / "package.json"
        if package_json_path.exists():
            content = package_json_path.read_text(encoding='utf-8')
            match = re.search(r'"version"\s*:\s*"([^"]+)"', content)
            if match:
                old_version = match.group(1)
                new_version = bump_version_string(old_version, ext_pattern)
                content = content.replace(f'"version": "{old_version}"', f'"version": "{new_version}"', 1)
                package_json_path.write_text(content, encoding='utf-8')
                print(f"Bumped extension version in package.json from {old_version} to {new_version} (pattern: {ext_pattern})")
            else:
                print("Warning: could not find version field in package.json")
                
    if bump_ext:
        print("__BUMP_EXTENSION__")

if __name__ == "__main__":
    main()
EOF
)
    # Print python script output to stdout (except our internal marker)
    echo "$BUMP_OUTPUT" | grep -v "__BUMP_EXTENSION__" || true
    
    # If the extension was bumped, build/package the extension
    if [[ "$BUMP_OUTPUT" == *"__BUMP_EXTENSION__"* ]]; then
        if command -v npm >/dev/null 2>&1; then
            printf 'Building and packaging VS Code Extension...\n'
            cd vscode-extension
            npm install
            npm run package
            cd ..
        else
            printf 'Warning: npm not found, skipping VS Code Extension compilation.\n' >&2
        fi
    fi
fi

printf 'Building Corner...\n'
cargo build --release

BINARY="./target/release/corner"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
PRIMARY="$INSTALL_DIR/corner"
LEGACY_SAFE_POCKET="$INSTALL_DIR/safe_pocket"
LEGACY_SPOCKET="$INSTALL_DIR/spocket"

if [ ! -f "$BINARY" ]; then
    printf 'Build failed: binary not found\n' >&2
    exit 1
fi

mkdir -p "$INSTALL_DIR"

printf 'Installing %s\n' "$PRIMARY"
cp -f "$BINARY" "$PRIMARY"
chmod +x "$PRIMARY"

printf 'Installing compatibility alias %s\n' "$LEGACY_SAFE_POCKET"
cp -f "$BINARY" "$LEGACY_SAFE_POCKET"
chmod +x "$LEGACY_SAFE_POCKET"

printf 'Installing compatibility alias %s\n' "$LEGACY_SPOCKET"
cp -f "$BINARY" "$LEGACY_SPOCKET"
chmod +x "$LEGACY_SPOCKET"

printf 'Seeding Corner assets under %s and %s\n' "$HOME/.config/corner" "$HOME/.corner"

# Detect an existing config templates directory and ask whether to replace it
# with the freshly built embedded templates (overwriting stale/legacy files
# such as safe_pocket.env.md) or skip and leave the user's current templates
# in place.
CONFIG_TEMPLATES_DIR="$HOME/.config/corner/templates"
REPLACE_FLAG=""
if [ -d "$CONFIG_TEMPLATES_DIR" ]; then
    printf '\n%s\n' "Found existing templates in $CONFIG_TEMPLATES_DIR"
    printf '%s\n' "  r) Replace them with the new built-in templates (overwrites customisations"
    printf '%s\n' "     and removes legacy-named files like safe_pocket.env.md)"
    printf '%s\n' "  s) Skip — keep the existing templates unchanged (recommended if you have"
    printf '%s\n' "     local customisations you want to preserve)"
    printf '%s\n' "  d) Dry-run upgrade-installation instead (rewrite legacy #SPOCKET_* references"
    printf '%s\n' "     to #CORNER_* in-place without replacing files)"
    printf '%s' "Choose [r/s/d]: "
    read -r ASSET_CHOICE
    case "$ASSET_CHOICE" in
        r|R)
            REPLACE_FLAG="--replace"
            printf '%s\n' "Replacing existing templates with the new built-in set."
            ;;
        d|D)
            printf '%s\n' "Running upgrade-installation in dry-run mode..."
            "$PRIMARY" upgrade-installation --dry-run || true
            REPLACE_FLAG=""
            ;;
        s|S|*)
            printf '%s\n' "Keeping existing templates unchanged."
            REPLACE_FLAG=""
            ;;
    esac
fi

"$PRIMARY" install-default-assets $REPLACE_FLAG >/dev/null

# Offer to rewrite legacy #SPOCKET_* references in already-placed files across
# all known Corner/Safe_pocket roots. This is a no-op if there is nothing to
# migrate, so it is safe to run unconditionally.
if "$PRIMARY" upgrade-installation --dry-run 2>/dev/null | grep -q "Found"; then
    printf '\n%s\n' "Legacy #SPOCKET_* references were detected in installed files."
    printf '%s' "Run `corner upgrade-installation` now to rewrite them? [y/N]: "
    read -r UPGRADE_CHOICE
    case "$UPGRADE_CHOICE" in
        y|Y)
            "$PRIMARY" upgrade-installation --yes || true
            ;;
        *)
            printf '%s\n' "Skipped. You can run \`corner upgrade-installation\` later."
            ;;
    esac
fi

if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    printf '\n%s is not in your PATH\n\n' "$INSTALL_DIR"
    printf 'Add this line to your shell config (~/.bashrc, ~/.zshrc, etc.):\n\n'
    printf '    export PATH="\\$PATH:%s"\n\n' "$INSTALL_DIR"
fi

printf 'Installation complete.\n\n'
printf 'Try it out:\n'
printf '  corner --help\n'
printf '  safe_pocket --help\n'
printf '  spocket --help\n'
printf '  corner register myproject="%s"\n\n' "$(pwd)"
printf 'To refresh an existing project placed templates after upgrading:\n'
printf '  corner -u <project-path>\n'
