#!/bin/bash

set -e

printf 'Building Safe Pocket...\n'
cargo build --release

BINARY="./target/release/safe_pocket"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
PRIMARY="$INSTALL_DIR/safe_pocket"
ALIAS="$INSTALL_DIR/spocket"

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
