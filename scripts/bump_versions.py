#!/usr/bin/env python3
"""Update Corner application and VS Code extension versions.

This helper is intentionally separate from install.sh so the installer contains
no heredoc and the version editing can be tested without compiling/installing.
"""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path


SEMVER = re.compile(r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$")
LEVEL_ALIASES = {
    "X..": "major",
    ".X.": "minor",
    "..X": "patch",
}


def next_version(current: str, request: str) -> str:
    match = SEMVER.fullmatch(current)
    if not match:
        raise ValueError(f"unsupported current version: {current!r}")
    request = LEVEL_ALIASES.get(request, request).lower()
    if SEMVER.fullmatch(request):
        return request

    major, minor, patch = (int(part) for part in match.groups())
    if request == "major":
        return f"{major + 1}.0.0"
    if request == "minor":
        return f"{major}.{minor + 1}.0"
    if request == "patch":
        return f"{major}.{minor}.{patch + 1}"
    raise ValueError(
        f"invalid version request {request!r}; use major, minor, patch, or X.Y.Z"
    )


def prepare_cargo_toml(path: Path, request: str, exact: bool) -> tuple[str, str, str]:
    text = path.read_text(encoding="utf-8")
    match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', text)
    if not match:
        raise ValueError(f"could not find package version in {path}")
    old = match.group(1)
    if exact and not SEMVER.fullmatch(request):
        raise ValueError(f"exact app version must be X.Y.Z, got {request!r}")
    new = request if exact else next_version(old, request)
    updated = text[: match.start(1)] + new + text[match.end(1) :]
    return old, new, updated


def prepare_package_json(path: Path, request: str, exact: bool) -> tuple[str, str, str]:
    data = json.loads(path.read_text(encoding="utf-8"))
    old = data.get("version")
    if not isinstance(old, str):
        raise ValueError(f"could not find version in {path}")
    if exact and not SEMVER.fullmatch(request):
        raise ValueError(f"exact extension version must be X.Y.Z, got {request!r}")
    new = request if exact else next_version(old, request)
    data["version"] = new
    return old, new, json.dumps(data, indent=2) + "\n"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--app")
    parser.add_argument("--extension")
    parser.add_argument("--set-app")
    parser.add_argument("--set-extension")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.app and args.set_app:
        raise SystemExit("use either --app or --set-app, not both")
    if args.extension and args.set_extension:
        raise SystemExit("use either --extension or --set-extension, not both")
    if not any((args.app, args.extension, args.set_app, args.set_extension)):
        raise SystemExit("at least one version operation is required")

    # Prepare and validate every requested update before writing either file.
    # A bad extension request must never leave Cargo.toml partially updated.
    updates: list[tuple[Path, str, str, str, str]] = []
    if args.app:
        path = args.root / "Cargo.toml"
        old, new, text = prepare_cargo_toml(path, args.app, exact=False)
        updates.append((path, old, new, text, path.read_text(encoding="utf-8")))
    if args.set_app:
        path = args.root / "Cargo.toml"
        old, new, text = prepare_cargo_toml(path, args.set_app, exact=True)
        updates.append((path, old, new, text, path.read_text(encoding="utf-8")))
    if args.extension:
        path = args.root / "vscode-extension" / "package.json"
        old, new, text = prepare_package_json(path, args.extension, exact=False)
        updates.append((path, old, new, text, path.read_text(encoding="utf-8")))
    if args.set_extension:
        path = args.root / "vscode-extension" / "package.json"
        old, new, text = prepare_package_json(path, args.set_extension, exact=True)
        updates.append((path, old, new, text, path.read_text(encoding="utf-8")))

    # Refuse to overwrite files that changed after preparation.
    for path, _, _, _, original in updates:
        if path.read_text(encoding="utf-8") != original:
            raise RuntimeError(f"{path} changed while versions were being prepared")

    replaced: list[tuple[Path, str]] = []
    try:
        for index, (path, old, new, text, original) in enumerate(updates, start=1):
            atomic_write(path, text)
            replaced.append((path, original))
            # Test-only failure injection verifies rollback after the first
            # replacement without relying on filesystem permission behavior.
            if os.environ.get("CORNER_BUMP_FAIL_AFTER") == str(index):
                raise OSError(f"injected failure after update {index}")
            label = "app" if path.name == "Cargo.toml" else "extension"
            print(f"Updated {label} version in {path.name} from {old} to {new}")
    except Exception:
        for path, original in reversed(replaced):
            atomic_write(path, original)
        raise
    return 0


def atomic_write(path: Path, text: str) -> None:
    temporary = path.with_name(f".{path.name}.corner-bump-{os.getpid()}")
    try:
        with temporary.open("w", encoding="utf-8") as handle:
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, path)
    finally:
        temporary.unlink(missing_ok=True)


if __name__ == "__main__":
    raise SystemExit(main())
