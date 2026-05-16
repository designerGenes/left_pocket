#!/bin/bash

set -e

printf 'Building Safe Pocket...\n'
cargo build --release

BINARY="./target/release/safe_pocket"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
PRIMARY="$INSTALL_DIR/safe_pocket"
ALIAS="$INSTALL_DIR/spocket"
BD_SHIM="$INSTALL_DIR/bd"
BD_REAL="$INSTALL_DIR/bd.real"

if [ ! -f "$BINARY" ]; then
    printf 'Build failed: binary not found\n' >&2
    exit 1
fi

mkdir -p "$INSTALL_DIR"

printf 'Installing %s\n' "$PRIMARY"
cp -f "$BINARY" "$PRIMARY"
chmod +x "$PRIMARY"

printf 'Installing alias %s\n' "$ALIAS"
cp -f "$BINARY" "$ALIAS"
chmod +x "$ALIAS"

if command -v bd >/dev/null 2>&1; then
    CURRENT_BD="$(command -v bd)"

    if [ "$CURRENT_BD" != "$BD_SHIM" ]; then
        cp -f "$CURRENT_BD" "$BD_REAL"
    elif [ ! -x "$BD_REAL" ]; then
        cp -f "$BD_SHIM" "$BD_REAL"
    elif [ -x "$BD_REAL" ]; then
        :
    fi

    if [ -n "$CURRENT_BD" ] && [ -x "$BD_REAL" ]; then
        printf 'Installing Beads shim %s\n' "$BD_SHIM"
        cat > "$BD_SHIM" <<'EOF'
#!/bin/sh

REAL_BD="$HOME/.local/bin/bd.real"

if [ -n "${BEADS_DIR:-}" ]; then
    search_dir="$PWD"
    while [ "$search_dir" != "/" ]; do
        if [ -d "$search_dir/.beads" ]; then
            unset BEADS_DIR
            break
        fi
        search_dir=$(dirname "$search_dir")
    done
fi

exec "$REAL_BD" "$@"
EOF
        chmod +x "$BD_SHIM"
    fi
fi

if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    printf '\n%s is not in your PATH\n\n' "$INSTALL_DIR"
    printf 'Add this line to your shell config (~/.bashrc, ~/.zshrc, etc.):\n\n'
    printf '    export PATH="\\$PATH:%s"\n\n' "$INSTALL_DIR"
fi

printf 'Installation complete.\n\n'
printf 'Try it out:\n'
printf '  safe_pocket --help\n'
printf '  spocket --help\n'
printf '  safe_pocket register myproject="%s"\n' "$(pwd)"
