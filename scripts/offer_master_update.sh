#!/bin/bash

# offer_master_update.sh <install.sh-path> [original install.sh args...]
#
# Best-effort "is this checkout behind origin/master?" check for install.sh.
# When the local checkout is behind and the user agrees, this pulls the latest
# master (--ff-only) and re-runs the (possibly updated) install.sh with its
# original arguments, so users stay up to date without shipping a compiled
# binary. Any failure (not a git repo, no origin, offline, diverged/dirty
# tree) is non-fatal: the script returns 0 and the current checkout installs
# as before.
#
# Exit status protocol for the calling install.sh:
#   0  — nothing was re-run; continue installing the current checkout.
#   10 — the updated install.sh already ran to success; the caller must exit
#        0 immediately instead of installing a second time. (The helper runs
#        as a subprocess, so `exec` inside it could never stop the parent —
#        without this protocol the whole install ran twice.)
#   *  — the re-run install.sh failed; the exit status is propagated.
#
# LEFT_POCKET_INSTALL_UPDATED=1 is set for the re-run so the check happens at
# most once per install.

set -e

TARGET="$1"
shift

if [ "${LEFT_POCKET_INSTALL_UPDATED:-}" = "1" ]; then
    exit 0
fi

REPO_DIR="$(CDPATH= cd -- "$(dirname -- "$TARGET")" && pwd)"

if ! git -C "$REPO_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    exit 0
fi
if ! git -C "$REPO_DIR" remote get-url origin >/dev/null 2>&1; then
    exit 0
fi
# Best-effort fetch: offline or unreachable remotes must never block installs.
if ! git -C "$REPO_DIR" fetch --quiet origin master >/dev/null 2>&1; then
    exit 0
fi

BEHIND="$(git -C "$REPO_DIR" rev-list --count HEAD..origin/master 2>/dev/null || printf '0')"
if [ "$BEHIND" = "0" ]; then
    exit 0
fi

printf 'Your local left_pocket checkout is %s commit(s) behind origin/master.\n' "$BEHIND"
if [ -t 0 ]; then
    printf '%s' 'Pull the latest master changes and run the updated install.sh? [y/N]: '
fi
# `set -e` is on: a bare `read` at EOF (non-interactive stdin) returns
# non-zero; fall back to the safe default instead of aborting.
read -r PULL_CHOICE || PULL_CHOICE="n"

case "$PULL_CHOICE" in
    y|Y)
        if git -C "$REPO_DIR" pull --ff-only origin master; then
            printf '%s\n' 'Re-running the updated install.sh...'
            UPDATE_STATUS=0
            LEFT_POCKET_INSTALL_UPDATED=1 bash "$TARGET" "$@" || UPDATE_STATUS=$?
            if [ "$UPDATE_STATUS" -eq 0 ]; then
                # The updated install completed; the caller must not install
                # a second time.
                exit 10
            fi
            exit "$UPDATE_STATUS"
        else
            printf '%s\n' 'Pull failed (diverged or dirty tree); continuing with the current checkout.' >&2
        fi
        ;;
    *)
        printf '%s\n' 'Continuing with the current checkout.'
        ;;
esac

exit 0
