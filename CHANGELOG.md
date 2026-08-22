# Changelog

All notable changes to left_pocket will be documented in this file.

## [Unreleased]

### Changed — pockets are "pockets", `left_pocket` names only the application
- **`left_pocket` now refers exclusively to the application.** The workspaces
  it generates and manages are called *pockets* everywhere else: CLI flags,
  identifiers, JSON keys, docs, and user-facing strings.
- CLI flags `--left_pocket` renamed to `--pocket` across `sync`, `heal`,
  `mark` (value name), `runtime-merge-start`, `runtime-merge-stop`, and
  `daily-feature`. The old `--left_pocket` spelling keeps working as a hidden
  alias.
- Env key `LEFT_POCKET_ROOT` renamed to `POCKET_ROOT` (emitted into `.env`
  files); `LEFT_POCKET_ROOT`, `LOCKET_ROOT`, `CORNER_ROOT`, and `SPOCKET_ROOT`
  are still honoured when reading.
- Directives and markers renamed to `#POCKET_TEMPLATE_DESTINATION`,
  `#POCKET_INSTALL_DESTINATION`, `#POCKET_QUIET_MERGE`,
  `#POCKET_MERGE_AT_RUNTIME`, and `#POCKET_RUNTIME_CONTENT_START` /
  `#POCKET_RUNTIME_CONTENT_END`. Legacy `#LEFT_POCKET_*`, `#CORNER_*`, and
  `#SPOCKET_*` forms are still recognised when reading.
- Template variables renamed to `{{POCKET_ROOT}}`, `{{POCKET_NAME}}`,
  `{{POCKET_CONFIG_ROOT}}`, and `{{POCKET_REGISTRY_ROOT}}`; all legacy
  spellings still expand.
- JSON output keys renamed: `locate` now emits `pocket_dir`, `sync-registry`
  emits `pockets`, and the memgraph bridge uses `pocket_hash` / `pocket_path`
  and the `:pocket` node label.
- Internal Rust identifiers renamed (`pocket_dir`, `pockets`, `pocket_root`,
  `dedupe_pockets`, …) and the VS Code extension was updated to match.
- Template file names deliberately keep `left_pocket` (e.g.
  `left_pocket.env.md`, `left_pocket.gitignore.md`, agent `left_pocketer`).

### Added
- **Dynamic pocket logo.** The new pocket-shaped ASCII logo shows the current
  pocket's ID centred inside the pocket whenever left_pocket opens a project
  (`left_pocket -i …`); help and version output show the plain logo without an
  ID.

### Fixed
- `build.rs` now reads `CARGO_MANIFEST_DIR` at runtime, so a stale build
  script binary from before a project-directory rename can no longer embed an
  empty template table.
- Repaired the quiet-merge directive parser and the `upgrade-installation`
  token table, both of which had been reduced to no-ops by the earlier bulk
  rename (legacy `#LEFT_POCKET_QUIET_MERGE` / `#LEFT_POCKET_MERGE_AT_RUNTIME`
  files now upgrade correctly).
- README no longer documents unimplemented `#LOCKET_*` tokens; the template
  reference now matches the implemented `#POCKET_*` scheme.

## [3.0.0] - 2026-08-22

### Changed — full rename: Corner → left_pocket ("locket")
- **The application is now `left_pocket`.** `locket` is a short CLI alias only;
  all directives, templates, internal identifiers, and documentation use
  `left_pocket` / `LEFT_POCKET` / `left_pocket_dir` naming.
- Directives and markers are now `#LEFT_POCKET_TEMPLATE_DESTINATION`,
  `#LEFT_POCKET_INSTALL_DESTINATION`, `#LEFT_POCKET_QUIET_MERGE`,
  `#LEFT_POCKET_MERGE_AT_RUNTIME`, and
  `#LEFT_POCKET_RUNTIME_CONTENT_START` / `#LEFT_POCKET_RUNTIME_CONTENT_END`.
  Legacy `#CORNER_*` and `#SPOCKET_*` forms are still recognised when reading.
- Template variables are now `{{LEFT_POCKET_ROOT}}`, `{{LEFT_POCKET_NAME}}`,
  `{{LEFT_POCKET_CONFIG_ROOT}}`, and `{{LEFT_POCKET_REGISTRY_ROOT}}`; the
  legacy `{{CORNER_*}}` and `{{SPOCKET_*}}` forms still expand.
- Storage roots migrated: registry `~/.corner` → `~/.left_pocket`, config
  `~/.config/corner` → `~/.config/left_pocket`, backup repo
  `~/.corner_backup_repo` → `~/.left_pocket_backup_repo`, env key
  `CORNER_ROOT` → `LEFT_POCKET_ROOT` (legacy keys still honoured per-file).
- Binary renamed to `left_pocket`; compatibility binaries `locket`, `corner`,
  `safe_pocket`, and `spocket` remain installed as aliases.
- Workspace folder names are now `[left_pocket] <hash>`.
- VS Code extension renamed to `left-pocket` (commands `left_pocket.*`).

### Added
- **`left_pocket -u` now rewrites legacy directives in place.** The upgrade
  scan covers both the left_pocket directory and its registered project
  directories, rewriting `#CORNER_*` and `#SPOCKET_*` tokens (including the
  task-integration markers) to their `#LEFT_POCKET_*` equivalents before
  templates are re-placed.
- `left_pocket upgrade-installation` additionally rewrites `#CORNER_*` tokens
  and `{{CORNER_CONFIG_ROOT}}` literal-root artifacts.

## [2.3.0] - 2026-08-01

### Added
- **left_pocket-aware terminal prompt index.** Registry cache updates now atomically
  write a compact `prompt_paths` file containing registered project and left_pocket
  paths. The Zsh prompt uses this index without launching `left_pocket` or parsing
  JSON, so project paths can render cobalt and left_pocket paths yellow immediately
  after a directory change.

## [2.2.0] - 2026-07-25

### Added
- **`left_pocket tests --all` installed-binary operational harness.** Runs the
  currently executing left_pocket binary through create/open, locate, environment
  placement, quiet merge, reverse-sync prevention, runtime merge, template
  upgrade, augment, alias, heal, OpenCode-agent, unresolved-variable, and
  migration-idempotency scenarios. Every destructive scenario uses a fully
  isolated temporary HOME/registry/project and a fake VS Code executable. The
  command prints each child command plus a final PASS/FAIL/SKIP checklist.
- **Safe existing-project mode:** `left_pocket tests --all -i PATH` performs only
  read-only locate/artifact audits on the original, retains a complete content
  backup (file bytes and symlink targets) under `real-world-test-backups/`, and
  runs the operational suite against a retained isolated project copy. Projects
  without an existing registered left_pocket are supported too.
- `left_pocket upgrade-installation` now reports literal
  `{{SPOCKET_CONFIG_ROOT}}` / `{{LOCKET_CONFIG_ROOT}}` artifact directories.
  `--clean-literal-root-artifacts` can clean only directories containing exactly
  one regular `feature_tags.yaml`; it backs every file up, lists every target,
  and requires explicit confirmation. Unexpected contents are never removed.
- `install.sh -h` / `--help`, `--bump-version`,
  `--bump-extension-version`, `--set-version`, and
  `--set-extension-version`. Bumps accept major/minor/patch or exact SemVer.
  Legacy bump syntax remains compatible.
- **`left_pocket locate --read-only`.** Resolves a project's left_pocket by reading
  manifests directly, without loading/migrating alias config and without
  creating or rebuilding a registry cache. This is what makes the `left_pocket tests
  -i` audit genuinely non-mutating.

### Changed
- **`SPOCKET_ROOT` is no longer written.** The Spocket/Safe_pocket rename is
  complete: the project and left_pocket `.env` templates and `sync_root_env_file` now
  emit only `PROJECT_ROOT` and `LOCKET_ROOT`, and a stale `SPOCKET_ROOT` line is
  removed on migration rather than carried forward as a second, contradictory
  root. Backwards compatibility is preserved on the **read** side —
  `branding::LEGACY_ROOT_ENV_KEYS` and `left_pocket task` prefix detection still
  accept a pre-existing `SPOCKET_ROOT`, so a legacy-only `.env` keeps resolving.
- Installer version editing moved from an inline Python heredoc to the testable
  `scripts/bump_versions.py` helper. `install.sh` can now be invoked from any
  working directory and contains no heredocs.
- Documented `.opencode` ownership: left_pocket manages only `.opencode/agent`;
  project-local npm packages/configuration are external and are never removed.

### Fixed
- **`locate --read-only` resolved an ancestor project.** It returned the first
  manifest whose project path was merely a *prefix* of the requested path, so a
  left_pocket registered for `~/dev/bin` beat the left_pocket for `~/dev/bin/left_pocket`.
  Because `left_pocket tests -i` uses this to choose what to back up and clone, an
  audit could operate on the wrong left_pocket entirely. Candidates are now ranked by
  specificity (exact match, then longest matching path), and the reported `hash`
  is the left_pocket's directory name with the manifest's own hash exposed separately
  as `manifest_hash`.
- **A seeded existing left_pocket was never adopted, producing silent false PASSes.**
  The isolated clone was keyed to the original project's hash, so
  `find_workspace_by_manifest_paths` could never match it; `left_pocket -i` created a
  second blank left_pocket and every "existing state" check ran against empty state
  while still reporting PASS. The clone is now re-keyed
  (`hash_paths`, `manifest.hash`, workspace filename, lineage fields cleared) and
  a dedicated `Seeded existing left_pocket is adopted, not replaced` case fails loudly
  if adoption does not happen.
- **`upgrade-installation` rewrote files inside its own backups.** `SKIP_DIRS`
  excluded `snapshots`/`unhoused` but not `upgrade-backups` or
  `real-world-test-backups`, so retained backups were text-rewritten in place
  (a backup a later command edits is not a backup) and artifact counts inflated
  on every run. Both trees are now excluded, and the reported artifact count is
  stable across repeated runs.
- Consolidated four divergent reserved-registry-directory lists into
  `registry::RESERVED_REGISTRY_DIRS` / `is_reserved_registry_name`. The
  read-only locate copy had been missing `registry` and `.git`, so an audit
  could try to parse a manifest out of the registry's own git repository.
- `install.sh` ran `read -r` under `set -e`, so a piped/CI invocation aborted
  after copying the binaries but before seeding assets — a half-install. Both
  prompts are now guarded with `[ -t 0 ]` and fall back to the safe default.
- The isolated project clone no longer copies `node_modules`/`target`/`.git`,
  and never preserves project symlinks (an absolute or escaping link could route
  fixture writes back into the real project). The retained backup remains
  deliberately complete.

### Safety
- The operational harness never invokes `left_pocket clean --all`, hard cleanup,
  remote backup configuration, or a real editor process.
- No existing literal-root artifact directory is automatically removed. Cleanup
  remains an explicit, confirmed operation, and now moves the whole directory
  into a timestamped quarantine via a single atomic rename instead of
  copy-then-delete.

## [2.1.0] - 2026-07-25

Completes the Safe_pocket → left_pocket rename inside the template system, and makes
the rename repeatable for future renames.

### Added
- **`#LOCKET_*` template directives.** left_pocket's template grammar is now spelled
  with the `LOCKET` prefix:
  `#LOCKET_TEMPLATE_DESTINATION`, `#LOCKET_INSTALL_DESTINATION`,
  `#LOCKET_QUIET_MERGE`, `#LOCKET_MERGE_AT_RUNTIME`, and the runtime markers
  `#LOCKET_RUNTIME_CONTENT_START` / `#LOCKET_RUNTIME_CONTENT_END`.
  The legacy `#SPOCKET_*` spellings are still **recognised when reading**
  templates and already-placed files, so nothing breaks before you migrate.
  New content is always written with the `#LOCKET_` prefix.
- **`left_pocket upgrade-installation`.** Rewrites legacy `#SPOCKET_*` directives and
  runtime markers to their `#LOCKET_*` equivalents, in place, across every known
  config root (`~/.config/left_pocket`, `~/.config/safe_pocket`, `~/.config/spocket`)
  and registry root (`~/.left_pocket`, `~/.safe_pocket`, `~/.spocket`). Supports
  `--dry-run`, `--yes`, and `--root PATH` (repeatable). This is a one-way text
  migration, never a sync: it never copies content from a project back into the
  config templates directory. User-facing feature-tag names (e.g.
  `SPOCKET_MUST_INSTALL`) are deliberately left intact.
- **`left_pocket install-default-assets --replace`.** Overwrites existing templates
  and config assets instead of skipping files that already exist, clearing
  `templates/` first so legacy-named files are removed.
- `install.sh` now detects an existing `~/.config/left_pocket/templates` directory and
  asks whether to **replace** it with the new built-in templates, **skip** it, or
  run **`upgrade-installation --dry-run`** to preview a token migration instead.
  After installing it offers to run `left_pocket upgrade-installation` if legacy
  references are detected.

### Fixed
- **`left_pocket -u` no longer destroys quiet-merge destinations.** `#LOCKET_QUIET_MERGE`
  templates were merged on left_pocket *creation* but **overwritten** on upgrade, so
  `left_pocket -u <project>` would flatten a project's `.gitignore` down to the
  template's single `.env` line and replace a populated `.env` with just the
  template keys. Quiet-merge templates now merge in both modes, matching the
  documented contract: existing content is preserved and only genuinely new
  lines/keys are appended. Regression tests cover both `.env` and `.gitignore`.
- **Templates no longer "backwards-sync" into existence.** Renaming a template in
  `~/.config/left_pocket/templates` used to see the old filename reappear on the next
  `left_pocket -i .`, because the old name was still baked into the binary's embedded
  template set and re-staged on every launch. The shipped templates are now named
  `left_pocket.env.md` and `left_pocket.gitignore.md` (previously `safe_pocket.env.md` and
  `safe_pocket.gitignore.md`), so the stale names can no longer be resurrected.
  Use `left_pocket install-default-assets --replace` to clear leftovers.
- **`.env` files now define every root.** Both the project `.env` and the left_pocket
  `.env` are seeded with `PROJECT_ROOT`, `LOCKET_ROOT`, and the legacy
  `SPOCKET_ROOT` alias, fixing the case where opening a project did not export
  `LOCKET_ROOT` or `PROJECT_ROOT`.
- **Installation no longer aborts on orphaned left_pockets.** `install-default-assets`
  (and therefore `install.sh`) used to fail outright when any left_pocket's manifest
  referenced a project directory that no longer existed — a deleted repo, an
  unmounted volume, or a stale temp-directory left_pocket left behind by a test run.
  Missing project directories are now skipped instead.
- Removed a duplicated `LOCKET_ROOT=` line from the left_pocket `.env` template.

## [2.0.2] - 2026-07-18

### Fixed
- Stale registry caches no longer shadow fresh on-disk manifests. When
  `~/.left_pocket/registry_cache.json` or `~/.safe_pocket/registry_cache.json`
  claims a `manifest_hash` or `core_paths` that no longer matches the left_pocket's
  on-disk `manifest.json`, `left_pocket locate` and `left_pocket -i` now verify against
  disk before trusting the cache, and fall back to a full disk scan when no
  cached entry survives verification. This resolves the "lost connection"
  regression that appeared after renaming `safe_pocket` to `left_pocket` and then
  augmenting the left_pocket's core_paths (e.g. adding `~/.config/left_pocket/templates`).
- Split-brain registry entries (same left_pocket hash in multiple registry roots)
  now collapse to a single canonical entry during `load_cache_or_rebuild`.
  The survivor is chosen by, in order: the entry whose on-disk `birth_hash`
  matches the directory name (i.e. the left_pocket that "owns" the hash), the entry
  whose cached `manifest_hash` matches the on-disk manifest (fresh cache), the
  entry in the preferred registry root, then the first entry.
- `left_pocket augment` (and any other path that calls `Manifest::save`) now prunes
  duplicate entries from other registry roots' caches so a future lookup
  cannot resurrect a stale sibling.

### Added
- `left_pocket sync-registry` command: rebuilds the registry cache in every known
  registry root (`~/.left_pocket`, `~/.safe_pocket`, `~/.spocket`) directly from
  on-disk manifests, collapsing split-brain duplicates. Use this after
  manually editing a manifest, moving left_pocket directories outside left_pocket, or
  when `left_pocket locate` / `left_pocket -i` resolve to the wrong left_pocket.
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
- left_pocket now recognizes both `LOCKET_ROOT` and legacy `SPOCKET_ROOT` in project `.env` files, preferring `LOCKET_ROOT` when both are present.
- Template expansion now supports `{{LOCKET_ROOT}}`, `{{LOCKET_NAME}}`, `{{LOCKET_CONFIG_ROOT}}`, and `{{LOCKET_REGISTRY_ROOT}}` alongside the legacy `SPOCKET_*` placeholders.

### Changed
- Generated project and pocket `.env` files now write both `LOCKET_ROOT` and `SPOCKET_ROOT` for compatibility with older tooling.
- Repository metadata, workspace README links, VS Code extension metadata, and documentation examples now point at the renamed `designerGenes/left_pocket` repository and the `left_pocket` command.

## [2.0.0] - 2026-07-18

### Changed
- Renamed the primary CLI and product identity from `safe_pocket` to `left_pocket`.
- New installs now prefer `~/.left_pocket/` and `~/.config/left_pocket/` for pocket and config storage.
- Shell completions, generated help text, workspace labels, runtime task guidance, and the VS Code extension now present `left_pocket` as the primary command name.

### Compatibility
- Existing `safe_pocket` and `spocket` commands continue to work as compatibility aliases.
- Runtime storage lookup now prefers left_pocket-named directories and falls back to legacy `~/.safe_pocket/`, `~/.spocket/`, `~/.config/safe_pocket/`, and `~/.config/spocket/` locations when the requested files or directories still live there.
- Added coverage proving the legacy `safe_pocket` binary still works and that legacy storage roots are reused until a left_pocket root exists.

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
- `#LEFT_POCKET_TEMPLATE_DESTINATION` may now appear **multiple times** in a single
  template file. Each directive applies to all content beneath it until the next
  directive, letting one template file populate several destination files.
  Blocks targeting the same destination are concatenated.

## [0.10.0] - 2026-05-31

### Added
- `--simulate-runtime` flag for `safe_pocket -i`: injects runtime content into
  destination files (between `#LEFT_POCKET_RUNTIME_CONTENT_START` /
  `#LEFT_POCKET_RUNTIME_CONTENT_END` markers) exactly as it would appear at VS Code
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
