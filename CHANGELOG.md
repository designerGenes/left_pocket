# Changelog

All notable changes to Corner will be documented in this file.

## [2.0.2] - 2026-07-18

### Fixed
- Stale registry caches no longer shadow fresh on-disk manifests. When
  `~/.corner/registry_cache.json` or `~/.safe_pocket/registry_cache.json`
  claims a `manifest_hash` or `core_paths` that no longer matches the corner's
  on-disk `manifest.json`, `corner locate` and `corner -i` now verify against
  disk before trusting the cache, and fall back to a full disk scan when no
  cached entry survives verification. This resolves the "lost connection"
  regression that appeared after renaming `safe_pocket` to `corner` and then
  augmenting the corner's core_paths (e.g. adding `~/.config/corner/templates`).
- Split-brain registry entries (same corner hash in multiple registry roots)
  now collapse to a single canonical entry during `load_cache_or_rebuild`.
  The survivor is chosen by, in order: the entry whose on-disk `birth_hash`
  matches the directory name (i.e. the corner that "owns" the hash), the entry
  whose cached `manifest_hash` matches the on-disk manifest (fresh cache), the
  entry in the preferred registry root, then the first entry.
- `corner augment` (and any other path that calls `Manifest::save`) now prunes
  duplicate entries from other registry roots' caches so a future lookup
  cannot resurrect a stale sibling.

### Added
- `corner sync-registry` command: rebuilds the registry cache in every known
  registry root (`~/.corner`, `~/.safe_pocket`, `~/.spocket`) directly from
  on-disk manifests, collapsing split-brain duplicates. Use this after
  manually editing a manifest, moving corner directories outside corner, or
  when `corner locate` / `corner -i` resolve to the wrong corner.
- `registry::refresh_entry_from_disk`, `registry::scan_all_roots_for_entries`,
  and `registry::rebuild_all_caches` public helpers for cache verification
  and recovery.

### Changed
- `Workspace::find_workspace_by_manifest_paths` and
  `Workspace::find_best_cached_workspace` now verify each cache hit against
  the on-disk manifest before returning it, and fall back to a full disk
  scan when no cache hit survives verification.

## [2.0.1] - 2026-07-18

### Added
- Corner now recognizes both `CORNER_ROOT` and legacy `SPOCKET_ROOT` in project `.env` files, preferring `CORNER_ROOT` when both are present.
- Template expansion now supports `{{CORNER_ROOT}}`, `{{CORNER_NAME}}`, `{{CORNER_CONFIG_ROOT}}`, and `{{CORNER_REGISTRY_ROOT}}` alongside the legacy `SPOCKET_*` placeholders.

### Changed
- Generated project and pocket `.env` files now write both `CORNER_ROOT` and `SPOCKET_ROOT` for compatibility with older tooling.
- Repository metadata, workspace README links, VS Code extension metadata, and documentation examples now point at the renamed `designerGenes/corner` repository and the `corner` command.

## [2.0.0] - 2026-07-18

### Changed
- Renamed the primary CLI and product identity from `safe_pocket` to `corner`.
- New installs now prefer `~/.corner/` and `~/.config/corner/` for pocket and config storage.
- Shell completions, generated help text, workspace labels, runtime task guidance, and the VS Code extension now present `corner` as the primary command name.

### Compatibility
- Existing `safe_pocket` and `spocket` commands continue to work as compatibility aliases.
- Runtime storage lookup now prefers Corner-named directories and falls back to legacy `~/.safe_pocket/`, `~/.spocket/`, `~/.config/safe_pocket/`, and `~/.config/spocket/` locations when the requested files or directories still live there.
- Added coverage proving the legacy `safe_pocket` binary still works and that legacy storage roots are reused until a Corner root exists.

## [1.0.0] - 2026-05-31

### Added
- **Built-in task tracker** (`spocket task`): a fast, SQLite-backed, Jira-style
  issue tracker that replaces Beads. The database lives at
  `~/.safe_pocket/global_data/tasks.db` and is shared across every project.
  - `spocket task list [--priority N] [--project <path>] [--raw]` — list open
    and in-progress tasks, sorted by priority then creation time. `--priority N`
    shows tasks at priority `N` or more urgent (lower number).
  - `spocket task create --named "…" [--description "…"] [--priority N]` — create
    a task. IDs are `<prefix>-<6 chars>`, where the prefix is derived from the
    safe pocket associated with the current directory.
  - `spocket task <ID> assign --agent "…"` — record which agent owns a task.
  - `spocket task <ID> start [--notes "…"]` — mark a task in progress.
  - `spocket task <ID> log [--notes "…"]` — append a progress note.
  - `spocket task <ID> close [--notes "…"]` — mark a task done.
  - `spocket task <ID> discard` — soft-delete (status becomes `discarded`).
  - `spocket task <ID> describe [--raw]` — show full details and history; `--raw`
    emits JSON.
  - `spocket task <ID> reprefix <new-prefix>` — rewrite a project's task IDs when
    its safe pocket name changes.
  - `heal` automatically migrates a project's tracked tasks to the renamed
    pocket's prefix, so issues stay discoverable after the directory is renamed.
  - IDs may be referenced by full id, case-insensitively, or by the bare 6-char
    suffix when run from inside the owning project.
- The runtime AGENTS.md block now advertises the `spocket task` workflow to
  agents instead of Beads.

### Removed
- **Beads integration is gone.** Removed the `--use beads` / `--without-beads`
  flags, the `bd` install shim, the `.beads` redirect stubs, the `uses_beads`
  manifest field, and all `bd`-driven setup. Existing pockets are cleaned up: the
  legacy `<!-- BEGIN/END BEADS INTEGRATION -->` block is stripped and any
  `BEADS_DIR=` env lines are removed on the next runtime merge.

## [0.11.0] - 2026-05-31

### Added
- Unified agent definitions: a single source of truth for safe_pocket's AI agents
  lives in `~/.config/safe_pocket/templates/agents/`. Each agent is a Markdown
  file with tool-agnostic YAML frontmatter (`agent_name`, `description`, `notes`,
  `mode`, `model`, `can`, `cannot`). Six default agents ship with the binary:
  **builder**, **critic**, **reporter**, **documenter**, **installer**, and
  **safe_pocketer**.
- `safe_pocket sync agents`: translate the unified agent definitions into the
  places OpenCode looks for agents (`~/.config/opencode/agent/*.md`). Capabilities
  map onto OpenCode permissions (`code`/`document` → edit+write, `test`/`execute`
  → bash, `plan` → task); effective capabilities are `can` minus `cannot`.
  Non-destructive: a pre-existing, hand-authored agent file is backed up to
  `<name>.md.pre-spocket.bak` before being replaced.
- `safe_pocket sync all`: run every system-wide sync (currently agents).
- Agents are now auto-synced whenever a pocket is opened.

### Changed
- `#SPOCKET_TEMPLATE_DESTINATION` may now appear **multiple times** in a single
  template file. Each directive applies to all content beneath it until the next
  directive, letting one template file populate several destination files.
  Blocks targeting the same destination are concatenated.

## [0.10.0] - 2026-05-31

### Added
- `--simulate-runtime` flag for `safe_pocket -i`: injects runtime content into
  destination files (between `#SPOCKET_RUNTIME_CONTENT_START` /
  `#SPOCKET_RUNTIME_CONTENT_END` markers) exactly as it would appear at VS Code
  runtime, without launching the editor. Useful for testing inject-at-runtime
  behaviour headlessly.
- `--silent` flag for `safe_pocket -i`: performs every setup step but does not
  open VS Code at the end (debug aid). Inside a directory already associated
  with a safe pocket this is effectively a no-op.
- Integration tests proving a normal run launches VS Code, while `--silent` and
  `--simulate-runtime` skip the launch (and the latter still injects runtime
  markers).

## [0.9.0] - 2026-05-31

### Added
- `safe_pocket daily-feature --pocket <dir> [--new] [--subpath <path>]` subcommand
  that resolves (and creates) today's daily feature file, returning JSON for the
  VS Code extension. New files default into `FEATURES/dailies/`.
- Feature tags in `feature_tags.yaml` marked `place_automatically: true` are now
  injected at the top of newly created feature files (removed tags are not
  re-added).
- Context-dependent hotkey in the VS Code extension: `ctrl+alt+t` opens/creates
  today's daily feature when not already inside it, and opens
  `feature_tags.yaml` when the active editor is today's feature file.
- `ctrl+shift+alt+t` (while inside today's feature file) creates and opens a new
  numbered daily feature file (`YYYY_MM_DD_1.md`, `YYYY_MM_DD_2.md`, ...).

### Changed
- The `spocket.dailyFeatureSubpath` extension setting now defaults to `dailies`.

## [0.8.0] - 2026-05-16

### Added
- Integration coverage for the public CLI help surface, deprecated-command rejection, and temporary-pocket cleanup behavior
- Per-test execution summaries describing the steps exercised and how each test environment cleans up after itself

### Changed
- Replaced `safe_pocket list` with `safe_pocket list-aliases`
- Renamed the extension-facing runtime merge hooks to hidden `runtime-merge-start` and `runtime-merge-stop` commands so obsolete public commands are removed from the CLI surface

## [0.4.0] - 2026-05-02

### Added
- `--temporary` pockets stored under `~/.safe_pocket/temporary/`
- `safe_pocket mark temporary <pocket>` to move an existing pocket into temporary storage
- `safe_pocket clean temporary`, `safe_pocket clean --older-than <age>`, and `safe_pocket clean --all`
- `--hard` cleanup mode for deleting pocket directories and `-y` to skip confirmation

### Changed
- Beads is now initialised by default unless `--without-beads` is passed
- Pocket manifests and registry entries now track whether a pocket is temporary
- `.env` templates now export `BEADS_DIR={{SPOCKET_ROOT}}/.beads`
- `safe_pocket` is now the primary installed binary name, with `spocket` kept as an alias

### Safety
- Clean operations only remove safe pocket registry entries and safe pocket directories; project folders are never deleted

## [0.3.0] - 2026-04-11

### Added
- **`-v`/`--version` flag** to display the application version (`spocket -v`)
- **`--verbose` flag** to enable detailed informational output

### Changed
- **Quieter default output**: Non-error, non-warning messages (e.g. "Runtime merged: ...", "Installed default template: ...") are now hidden unless `--verbose` is passed. This keeps the normal launch experience clean and uncluttered.

## [0.2.1] - 2026-02-14

### Changed
- **Removed all emojis** from user-facing output and VS Code workspace names
  - "[Safe Pocket]" instead of "🔒 Safe Pocket"
  - "[Sidecar]" instead of "🔗"
  - Cleaner, more professional CLI output
- **Simplified output** - removed decorative symbols from messages

### Added
- **README files** in safe pocket directories explaining their purpose
  - Root README explaining the safe pocket structure
  - `.github/prompts/README.md` - How to use prompt templates
  - `observations/README.md` - Purpose of the observations directory
- **`--no-readme` flag** to skip README generation if desired
- READMEs are automatically excluded when smart cloning (preserves source content)

## [0.2.0] - 2026-02-14

### Added - Features from 01.md

#### Smart Cloning
- **Automatic similarity detection**: When creating a new workspace, spocket now scans existing workspaces to find similar ones based on directory overlap
- **Interactive selection menu**: If similar workspaces are found (≥30% similarity), users are prompted to choose which one to clone from
- **Jaccard similarity scoring**: Uses intersection-over-union to calculate workspace similarity (e.g., 2 matching dirs out of 3 total = 66% similarity)
- **Multiple candidate support**: Displays all similar workspaces ranked by similarity percentage

#### Testing Infrastructure
- Added comprehensive unit tests for similarity calculations
- Tests for identical, partial, subset, and no-overlap scenarios
- Tests for edge cases (empty paths, single paths)
- All 7 tests passing in test suite

#### Improvements
- Made `create_workspace_file()` public for better reusability
- Enhanced workspace creation flow with smart cloning integration
- Better error handling with Context trait from anyhow
- Improved user experience with clear prompts and colored output

### Technical Details

**New Functions in `workspace.rs`:**
- `calculate_similarity(paths1, paths2) -> f64`: Calculates Jaccard similarity between path sets
- `find_similar_workspaces(target_paths, min_similarity) -> Vec<(Workspace, f64)>`: Finds and ranks similar workspaces
- `prompt_clone_selection(candidates) -> Option<&Workspace>`: Interactive menu for workspace selection

**Modified Functions:**
- `handle_workspace()` in `main.rs`: Integrated smart cloning before workspace creation

### What This Solves

Smart cloning addresses the key problem described in FEATURES/01.md:

> "Maybe this can benefit from the contents of D1_D2_D3_SpocketDir, even though it does not contain D2. If a workspace like this, which 'looks like' our existing workspace, is created, our safe_pocket app should recognize this and ask at launch time if we want to 'clone' in the file contents from (likely) parent safe pocket."

**Real-world scenario:**
1. You create a workspace for `[feature-a, shared-lib, tools]` with custom copilot instructions
2. Later, you need to work on just `[feature-b, shared-lib]`
3. spocket detects 33% similarity (1 of 3 dirs match)
4. You're prompted to clone the copilot instructions from the first workspace
5. You get your customizations without manual copy-paste

### Migration Notes

No breaking changes. All existing functionality preserved.

## [0.1.0] - 2026-02-14

### Initial Release

- Hash-based workspace naming (SHA256, 12-char truncation)
- Alias registration and management
- Workspace creation with safe pocket structure
- Sidecar directory support (ephemeral additions)
- Mismatch detection and warnings
- Manual safe pocket cloning with `--clone-from`
- Git initialization for each safe pocket
- VS Code workspace generation and opening
- Colored CLI output
- Configuration management in `~/.config/spocket/`
- Safe pocket storage in `~/.spocket/`
