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
printf '  corner register myproject="%s"\n' "$(pwd)"
