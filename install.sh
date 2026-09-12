#!/bin/bash

set -e

SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$SCRIPT_DIR"

usage() {
    printf '%s\n' 'Install left_pocket and its compatibility aliases.'
    printf '\n%s\n' 'Usage:'
    printf '%s\n' '  ./install.sh [OPTIONS]'
    printf '\n%s\n' 'Options:'
    printf '%s\n' '  -h, --help'
    printf '%s\n' '      Show this help and exit without building or installing.'
    printf '%s\n' '  --bump-version [major|minor|patch|X.Y.Z]'
    printf '%s\n' '      Update Cargo.toml before installing (default: patch).'
    printf '%s\n' '  --bump-extension-version [major|minor|patch|X.Y.Z]'
    printf '%s\n' '      Update vscode-extension/package.json, then package it (default: patch).'
    printf '%s\n' '  --set-version X.Y.Z'
    printf '%s\n' '      Set the Cargo.toml version exactly before installing.'
    printf '%s\n' '  --set-extension-version X.Y.Z'
    printf '%s\n' '      Set the extension version exactly, then package it.'
    printf '\n%s\n' 'Legacy compatibility:'
    printf '%s\n' '  bump, --app, --app(X..|.X.|..X), and'
    printf '%s\n' '  --extension(X..|.X.|..X) remain accepted.'
    printf '\n%s\n' 'Update check:'
    printf '%s\n' '  Each run fetches origin/master; if the local checkout is behind, the'
    printf '%s\n' '  script offers to pull and re-run the updated install.sh before building.'
    printf '\n%s\n' 'Environment:'
    printf '%s\n' '  INSTALL_DIR   Binary destination (default: $HOME/.local/bin).'
}

normalize_request() {
    case "$1" in
        X..) printf '%s' 'major' ;;
        .X.) printf '%s' 'minor' ;;
        ..X) printf '%s' 'patch' ;;
        major|minor|patch) printf '%s' "$1" ;;
        *) printf '%s' "$1" ;;
    esac
}

optional_request() {
    if [ "$#" -ge 1 ] && [[ "$1" != -* ]] && [ "$1" != "bump" ]; then
        normalize_request "$1"
    else
        printf '%s' 'patch'
    fi
}

APP_REQUEST=""
EXTENSION_REQUEST=""
APP_EXACT=false
EXTENSION_EXACT=false
PACKAGE_EXTENSION=false
ORIGINAL_ARGS=("$@")

while [ "$#" -gt 0 ]; do
    case "$1" in
        -h|--help)
            usage
            exit 0
            ;;
        --bump-version)
            shift
            APP_REQUEST="$(optional_request "$@")"
            APP_EXACT=false
            if [ "$#" -gt 0 ] && [[ "$1" != -* ]] && [ "$1" != "bump" ]; then shift; fi
            ;;
        --bump-version=*)
            [ -n "${1#*=}" ] || { printf '%s\n' 'Error: --bump-version requires a non-empty value after =' >&2; exit 2; }
            APP_REQUEST="$(normalize_request "${1#*=}")"
            APP_EXACT=false
            shift
            ;;
        --bump-extension-version)
            shift
            EXTENSION_REQUEST="$(optional_request "$@")"
            EXTENSION_EXACT=false
            PACKAGE_EXTENSION=true
            if [ "$#" -gt 0 ] && [[ "$1" != -* ]] && [ "$1" != "bump" ]; then shift; fi
            ;;
        --bump-extension-version=*)
            [ -n "${1#*=}" ] || { printf '%s\n' 'Error: --bump-extension-version requires a non-empty value after =' >&2; exit 2; }
            EXTENSION_REQUEST="$(normalize_request "${1#*=}")"
            EXTENSION_EXACT=false
            PACKAGE_EXTENSION=true
            shift
            ;;
        --set-version)
            [ "$#" -ge 2 ] || { printf '%s\n' 'Error: --set-version requires X.Y.Z' >&2; exit 2; }
            [[ "$2" != -* ]] || { printf '%s\n' 'Error: --set-version requires X.Y.Z' >&2; exit 2; }
            [[ "$2" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { printf '%s\n' 'Error: --set-version requires exact SemVer X.Y.Z' >&2; exit 2; }
            APP_REQUEST="$2"
            APP_EXACT=true
            shift 2
            ;;
        --set-extension-version)
            [ "$#" -ge 2 ] || { printf '%s\n' 'Error: --set-extension-version requires X.Y.Z' >&2; exit 2; }
            [[ "$2" != -* ]] || { printf '%s\n' 'Error: --set-extension-version requires X.Y.Z' >&2; exit 2; }
            [[ "$2" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { printf '%s\n' 'Error: --set-extension-version requires exact SemVer X.Y.Z' >&2; exit 2; }
            EXTENSION_REQUEST="$2"
            EXTENSION_EXACT=true
            PACKAGE_EXTENSION=true
            shift 2
            ;;
        bump)
            APP_REQUEST="patch"
            APP_EXACT=false
            shift
            ;;
        --app)
            APP_REQUEST="patch"
            APP_EXACT=false
            shift
            ;;
        --app\(*\))
            APP_REQUEST="$(normalize_request "${1#--app(}")"
            APP_REQUEST="$(normalize_request "${APP_REQUEST%)}")"
            APP_EXACT=false
            shift
            ;;
        --extension)
            # Preserve the historical behavior: legacy extension bumps also
            # bump the app when no app bump was requested.
            EXTENSION_REQUEST="patch"
            EXTENSION_EXACT=false
            PACKAGE_EXTENSION=true
            [ -n "$APP_REQUEST" ] || APP_REQUEST="patch"
            shift
            ;;
        --extension\(*\))
            EXTENSION_REQUEST="${1#--extension(}"
            EXTENSION_REQUEST="$(normalize_request "${EXTENSION_REQUEST%)}")"
            EXTENSION_EXACT=false
            PACKAGE_EXTENSION=true
            [ -n "$APP_REQUEST" ] || APP_REQUEST="patch"
            shift
            ;;
        *)
            printf 'Error: unknown option: %s\n\n' "$1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

# Offer to pull the latest origin/master before building, so an outdated
# checkout can re-run the updated install.sh instead of building stale code.
# Placed after argument parsing so --help and argument errors never touch the
# network; the helper is a no-op outside a git clone or when up to date.
# Exit-status protocol: 0 = nothing was re-run, keep installing; 10 = the
# updated install.sh already ran to success, so stop here (otherwise the
# whole install would run twice); anything else = the re-run failed, propagate.
UPDATE_CHECK=0
bash "$SCRIPT_DIR/scripts/offer_master_update.sh" "$SCRIPT_DIR/install.sh" "${ORIGINAL_ARGS[@]}" || UPDATE_CHECK=$?
case "$UPDATE_CHECK" in
    0) ;;
    10) exit 0 ;;
    *) exit "$UPDATE_CHECK" ;;
esac

if [ -n "$APP_REQUEST" ] || [ -n "$EXTENSION_REQUEST" ]; then
    BUMP_ARGS=(--root "$SCRIPT_DIR")
    if [ -n "$APP_REQUEST" ]; then
        if [ "$APP_EXACT" = true ]; then BUMP_ARGS+=(--set-app "$APP_REQUEST");
        else BUMP_ARGS+=(--app "$APP_REQUEST"); fi
    fi
    if [ -n "$EXTENSION_REQUEST" ]; then
        if [ "$EXTENSION_EXACT" = true ]; then BUMP_ARGS+=(--set-extension "$EXTENSION_REQUEST");
        else BUMP_ARGS+=(--extension "$EXTENSION_REQUEST"); fi
    fi
    python3 "$SCRIPT_DIR/scripts/bump_versions.py" "${BUMP_ARGS[@]}"
fi

if [ "$PACKAGE_EXTENSION" = true ]; then
    if command -v npm >/dev/null 2>&1; then
        printf 'Building and packaging VS Code Extension...\n'
        (
            cd "$SCRIPT_DIR/vscode-extension"
            npm install
            npm run package
        )
    else
        printf 'Warning: npm not found, skipping VS Code Extension compilation.\n' >&2
    fi
fi

printf 'Building left_pocket...\n'
cargo build --release

BINARY="$SCRIPT_DIR/target/release/left_pocket"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
PRIMARY="$INSTALL_DIR/left_pocket"
ALIAS_LOCKET="$INSTALL_DIR/locket"
ALIAS_CORNER="$INSTALL_DIR/corner"
ALIAS_SAFE_POCKET="$INSTALL_DIR/safe_pocket"
ALIAS_SPOCKET="$INSTALL_DIR/spocket"

if [ ! -f "$BINARY" ]; then
    printf 'Build failed: binary not found\n' >&2
    exit 1
fi

mkdir -p "$INSTALL_DIR"

printf 'Installing %s\n' "$PRIMARY"
cp -f "$BINARY" "$PRIMARY"
chmod +x "$PRIMARY"

for ALIAS in "$ALIAS_LOCKET" "$ALIAS_CORNER" "$ALIAS_SAFE_POCKET" "$ALIAS_SPOCKET"; do
    printf 'Installing compatibility alias %s\n' "$ALIAS"
    cp -f "$BINARY" "$ALIAS"
    chmod +x "$ALIAS"
done

printf 'Seeding left_pocket assets under %s and %s\n' "$HOME/.config/left_pocket" "$HOME/.left_pocket"

CONFIG_TEMPLATES_DIR="$HOME/.config/left_pocket/templates"
REPLACE_FLAG=""
if [ -d "$CONFIG_TEMPLATES_DIR" ]; then
    printf '\n%s\n' "Found existing templates in $CONFIG_TEMPLATES_DIR"
    printf '%s\n' "  r) Replace them with the new built-in templates (overwrites customisations"
    printf '%s\n' "     and removes legacy-named files like safe_pocket.env.md)"
    printf '%s\n' "  s) Skip - keep the existing templates unchanged (recommended if you have"
    printf '%s\n' "     local customisations you want to preserve)"
    printf '%s\n' "  d) Dry-run upgrade-installation instead (rewrite legacy #CORNER_*/#SPOCKET_* references"
    printf '%s\n' "     to #POCKET_* in-place without replacing files)"
    # `set -e` is on: a bare `read` at EOF returns non-zero and aborts the
    # script here, after the binaries are already copied but before assets are
    # seeded, leaving a half-install. Non-interactive runs take the safe
    # default instead.
    if [ -t 0 ]; then
        printf '%s' "Choose [r/s/d]: "
        read -r ASSET_CHOICE || ASSET_CHOICE="s"
    else
        ASSET_CHOICE="s"
        printf '%s\n' "Non-interactive stdin: keeping existing templates unchanged."
    fi
    case "$ASSET_CHOICE" in
        r|R)
            REPLACE_FLAG="--replace"
            printf '%s\n' "Replacing existing templates with the new built-in set."
            ;;
        d|D)
            printf '%s\n' "Running upgrade-installation in dry-run mode..."
            "$PRIMARY" upgrade-installation --dry-run || true
            ;;
        *)
            printf '%s\n' "Keeping existing templates unchanged."
            ;;
    esac
fi

"$PRIMARY" install-default-assets $REPLACE_FLAG >/dev/null

if "$PRIMARY" upgrade-installation --dry-run 2>/dev/null | grep -q "Found"; then
    printf '\n%s\n' "Legacy #CORNER_*/#SPOCKET_* references were detected in installed files."
    if [ -t 0 ]; then
        printf '%s' "Run left_pocket upgrade-installation now to rewrite them? [y/N]: "
        read -r UPGRADE_CHOICE || UPGRADE_CHOICE="n"
    else
        UPGRADE_CHOICE="n"
    fi
    case "$UPGRADE_CHOICE" in
        y|Y) "$PRIMARY" upgrade-installation --yes || true ;;
        *) printf '%s\n' "Skipped. You can run left_pocket upgrade-installation later." ;;
    esac
fi

if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    printf '\n%s is not in your PATH\n\n' "$INSTALL_DIR"
    printf 'Add this line to your shell config (~/.bashrc, ~/.zshrc, etc.):\n\n'
    printf '    export PATH="\$PATH:%s"\n\n' "$INSTALL_DIR"
fi

printf 'Installation complete.\n\n'
printf 'Try it out:\n'
printf '  left_pocket --help\n'
printf '  left_pocket tests --all\n'
printf '  locket --help\n'
printf '  corner --help\n'
printf '  safe_pocket --help\n'
printf '  spocket --help\n'
printf '  left_pocket register myproject="%s"\n\n' "$SCRIPT_DIR"
printf 'To refresh an existing project placed templates after upgrading:\n'
printf '  left_pocket -u <project-path>\n'
