```
╭────────╮
│        │
│     ╭──┤   left_pocket
│     │▓▓│
╰─────┴──╯
```

# left_pocket

**left_pocket** (compatibility aliases: `safe_pocket`, `spocket`) — ad hoc VS Code workspace manager with AI copilot support.

Keep "meta" files (copilot instructions, prompts, observations, feature notes) in a dedicated left_pocket directory (`~/.left_pocket/<hash>/`) so they never pollute your project repo, yet VS Code opens them together with your project as a single multi-root workspace.

left_pocket prefers the new roots `~/.left_pocket/` and `~/.config/left_pocket/`, but it falls back to legacy `~/.safe_pocket/`, `~/.spocket/`, `~/.config/safe_pocket/`, and `~/.config/spocket/` locations when the matching files or directories only exist there.

---

To understand left_pocket, read the [documentation](documentation/Concepts.md) or run `left_pocket --help` for a quick overview of commands and flags.

Reading the documentation is highly recommended.

---

## Quick Start

```bash
# Create or open a workspace for the current directory
left_pocket -i .

# Create a workspace spanning two projects
left_pocket -i ~/dev/frontend -i ~/dev/backend

# Upgrade left_pocket templates to match latest config
left_pocket -u ~/dev/myproject

# Check which left_pocket a project belongs to
left_pocket locate --path ~/dev/myproject

# Generate shell completions (zsh)
left_pocket completions zsh > ~/.zsh/completions/_left_pocket
```

---

## Command Hierarchy

```
left_pocket
│
├─ GLOBAL FLAGS (always available)
│  ├─ -i, --include PATH      Add directory to workspace (repeatable)
│  ├─ -s, --sidecar PATH      Add temporary sidecar (session only)
│  ├─ --with TOOL             Add tool for this session only
│  ├─ --add TOOL              Install tool for future sessions
│  ├─ -u, --upgrade PATH      Upgrade left_pocket templates
│  ├─ --clone-from PATH       Clone left_pocket from another project
│  ├─ --temporary             Use ~/.left_pocket/temporary
│  ├─ --force-new             Create new left_pocket even if one exists
│  ├─ --no-readme             Skip README generation
│  ├─ --simulate-runtime      Inject runtime content without launching VS Code
│  ├─ --silent                Setup workspace without opening VS Code
│  ├─ --verbose               Show informational messages
│  └─ -v                      Print version
│
├─ register NAME="PATH"        Register directory alias
├─ unregister NAME             Remove directory alias
├─ list-aliases                List all aliases
├─ list-workspaces             List all known left_pockets
│
├─ sync [TARGET]               Sync assets or manifest
│  └─ TARGET: agents | all
│
├─ augment                      Add/remove directories in-place
│  ├─ --add PATH               Add directory
│  ├─ --remove PATH            Remove directory
│  └─ --no-open                Don't open VS Code
│
├─ mark MARK LEFT_POCKET            Mark left_pocket with metadata
│  └─ MARK: temporary
│
├─ clean [SCOPE]               Clean left_pockets in bulk
│  ├─ SCOPE: temporary
│  ├─ --older-than AGE         Clean left_pockets older than AGE
│  ├─ --all                    Clean all left_pockets
│  ├─ --hard                   Delete directories (not just registry)
│  └─ -y, --yes                Skip confirmation
│
├─ heal                         Reconnect project to existing left_pocket
│  ├─ --project PATH           Project directory
│  ├─ --alias ALIAS            Alias name (instead of path)
│  └─ --left_pocket LEFT_POCKET          left_pocket directory
│
├─ locate                       Find left_pocket for a path
│  ├─ --path PATH              Project path (default: .)
│  └─ --read-only              Resolve without writing config/registry caches
│
├─ sync-registry-git            Refresh git snapshot of all left_pockets
│
├─ upgrade-installation         Rewrite legacy #SPOCKET_* tokens to #LOCKET_*
│  ├─ --dry-run                Preview rewrites without changing files
│  ├─ -y, --yes                Apply without prompting
│  ├─ --root PATH              Extra root to scan (repeatable)
│  └─ --clean-literal-root-artifacts
│                              Back up/remove safe unresolved-variable dirs
│
├─ tests                        Run installed-binary operational tests
│  ├─ --all                    Add extended augment/alias/heal/audit checks
│  ├─ -i, --include PATH       Audit an existing project safely first
│  ├─ --verbose                Show every child command's output
│  └─ --keep                   Retain the isolated fixture
│
├─ backup                       Configure automatic backup
│  ├─ --repo GIT_URL           Git remote for backups
│  └─ --schedule CRON          Cron schedule (default: hourly)
│
├─ completions SHELL            Generate shell completions
│  └─ SHELL: bash | zsh | fish | powershell | elvish
│
├─ completion-spec              Machine-readable completion model
│
├─ daily-feature                Manage daily feature notes
│  ├─ --left_pocket PATH            left_pocket directory
│  ├─ --new                    Always create new file
│  └─ --subpath PATH           Subfolder for daily files
│
├─ worktree ACTION              Manage git worktrees sharing this left_pocket
│  ├─ add [PATH]               Register worktree
│  ├─ remove PATH              Unregister worktree
│  └─ list                     List all worktrees
│
└─ task ARGS                    Built-in task tracker
   ├─ list [OPTIONS]           List tasks
   │  ├─ --priority N          Filter by priority
   │  ├─ --project PATH        Project path
   │  └─ --raw                 Machine-readable JSON
   ├─ create                   Create new task
   │  ├─ --named "TITLE"       Task title (required)
   │  ├─ --description "TEXT"  Task description
   │  ├─ --priority N          Priority 0-4 (default: 2)
   │  └─ --project PATH        Project path
   ├─ <ID> assign              Assign task to agent
   │  └─ --agent "NAME"        Agent name
   ├─ <ID> start               Start task
   │  └─ --notes "TEXT"        Start notes (optional)
   ├─ <ID> log                 Add progress log
   │  └─ --notes "TEXT"        Log entry (required)
   ├─ <ID> close               Close task
   │  └─ --notes "TEXT"        Closing notes (optional)
   ├─ <ID> discard             Discard task
   ├─ <ID> describe            Show task details
   │  └─ --raw                 Machine-readable JSON
   └─ reprefix                 Rename task prefix
      ├─ --from OLD            Old prefix
      └─ --to NEW              New prefix
```

---

## Global Flags

### Workspace Configuration

#### `-i`, `--include PATH` (repeatable)
Add a directory to the workspace. Can be used multiple times to create multi-root workspaces.

```bash
left_pocket -i ~/dev/frontend -i ~/dev/backend
```

#### `-s`, `--sidecar PATH` (repeatable)
Add a temporary sidecar directory (not saved to workspace file). Injected for this session only and removed next time the workspace opens normally. Useful for pulling in dependencies or reference repositories.

```bash
left_pocket -i . -s ~/external/lib
```

#### `--with TOOL` (repeatable)
Add a tool for this session only. Tools are exposed as managed sidecar folders when the workspace opens but are not installed for future sessions.

Supported tools: `gitleaks`, `graphify`, `memgraph`

```bash
left_pocket -i . --with gitleaks --with graphify
```

#### `--add TOOL` (repeatable)
Install a tool into the project and left_pocket for future sessions. If already installed, this is a no-op.

Supported tools: `gitleaks`, `graphify`, `memgraph`

```bash
left_pocket -i . --add graphify
```

#### `--clone-from PATH`
Clone the left_pocket from the workspace containing this path. Copies all meta files (copilot instructions, prompts, observations, etc.) from the source left_pocket into the new one, then tracks lineage in both manifests. Useful when starting a new project that should inherit AI configuration from a related one.

```bash
left_pocket -i ~/dev/new-project --clone-from ~/dev/existing-project
```

#### `-u`, `--upgrade PATH`
Upgrade an existing left_pocket to match current templates (does not open VS Code). Reads every template from the preferred config root, starting with `~/.config/left_pocket/templates/` and falling back to legacy config roots when needed, then expands variables and writes the result to the left_pocket. If a file already exists with different content, you are shown a diff and asked to confirm.

PATH may be either the left_pocket directory itself or any project directory whose left_pocket you want to upgrade.

```bash
left_pocket -u ~/dev/myproject
left_pocket -u ~/.left_pocket/abc123
```

#### `--new`
Force creation of a new workspace even if one already exists. By default, if a project already belongs to an existing left_pocket, that left_pocket is opened instead of creating a duplicate.

```bash
left_pocket -i . --new
```

#### `--temporary`
Create or reuse the left_pocket under `~/.left_pocket/temporary/`. Temporary left_pockets are tracked separately so test suites and other short-lived workflows can be cleaned up without touching normal left_pockets.

```bash
left_pocket -i . --temporary
```

#### `--no-readme`
Skip creating README files in empty directories. By default left_pocket writes helpful README.md files into new empty directories (observations/, .github/prompts/, etc.).

```bash
left_pocket -i . --no-readme
```

### Execution Control

#### `--simulate-runtime`
Inject runtime content into destination files without launching VS Code. Every file that would normally gain inject-at-runtime content (wrapped in `#LOCKET_RUNTIME_CONTENT_START` / `#LOCKET_RUNTIME_CONTENT_END` markers) gains that content even though VS Code is never started. Implies `--silent`.

```bash
left_pocket -i . --simulate-runtime --temporary
```

#### `--silent`
Perform every step except opening VS Code at the end. Useful for exercising left_pocket setup without launching the editor.

```bash
left_pocket -i . --silent
```

#### `--verbose`
Enable verbose output. Shows informational messages that are hidden by default, such as runtime merge notifications, template installation notices, and other non-error details.

```bash
left_pocket -i . --verbose
```

#### `-v`
Print version and exit.

---

## Subcommands

### Alias Management

#### `register NAME="PATH"`
Register a short alias for a directory path. Aliases let you refer to long directory paths by a short name in any `left_pocket` command that accepts a PATH argument.

```bash
left_pocket register api="~/dev/my-api-project"
left_pocket register frontend="~/dev/my-frontend"
left_pocket -i api -i frontend  # use aliases
```

#### `unregister NAME`
Remove a previously registered directory alias.

```bash
left_pocket unregister api
```

#### `list-aliases`
List all registered directory aliases.

```bash
left_pocket list-aliases
```

#### `list-workspaces`
List all known left_pockets with their project paths and status.

```bash
left_pocket list-workspaces
```

---

### Workspace Management

#### `sync [TARGET]`
Sync system-wide assets or the manifest.

**With a TARGET**, synchronizes system-wide left_pocket assets that every left_pocket draws from:

- `agents` — Write unified agent definitions from `~/.config/left_pocket/templates/agents/` into the places OpenCode looks for agents
- `all` — Run every system-wide sync (currently agents only)

**Without a TARGET** (legacy form used by VS Code extension), updates the left_pocket manifest to reflect the current .code-workspace folders and prints JSON. Requires `--left_pocket`. You rarely need to run this manually.

```bash
left_pocket sync agents      # Write agent definitions
left_pocket sync all         # Sync everything
left_pocket sync --left_pocket ~/.left_pocket/abc123  # Manifest sync (internal)
```

#### `augment`
Add or remove project directories from the current workspace in-place. Rewrites the .code-workspace file and manifest without moving the left_pocket directory. Run from inside a left_pocket or project directory that belongs to an existing workspace.

```bash
left_pocket augment --add ~/dev/new-service
left_pocket augment --remove ~/dev/old-service
left_pocket augment --add ~/dev/new-service --no-open
```

**Options:**
- `--add PATH` — Project directory to add to the workspace (repeatable)
- `--remove PATH` — Project directory to remove from the workspace (repeatable)
- `--no-open` — Update workspace without opening VS Code afterwards

#### `mark MARK LEFT_POCKET`
Mark an existing left_pocket with additional metadata.

```bash
left_pocket mark temporary ~/.left_pocket/abc123
```

**Mark types:**
- `temporary` — Mark left_pocket as temporary

#### `locate`
Locate the left_pocket associated with a project or left_pocket path. Outputs JSON for editor integrations.

```bash
left_pocket locate --path ~/dev/myproject
left_pocket locate --read-only --path ~/dev/myproject
```

**Options:**
- `--path PATH` — Project path to locate (default: current directory)
- `--read-only` — Resolve by reading manifests directly, without loading or
  migrating alias config and without creating or rebuilding a registry cache

Normal `locate` may create or refresh the registry cache as a side effect, which
is undesirable when auditing an installation you must not disturb. `--read-only`
guarantees no writes, adds `"read_only": true` to the payload, and reports the
left_pocket's directory name as `hash` alongside the manifest's own `manifest_hash`
(these differ for a renamed left_pocket). Both forms resolve the most specific
matching project, so a left_pocket registered for a parent directory never shadows the
left_pocket for a nested project. This is the mode `left_pocket tests -i` uses.

---

### left_pocket Maintenance

#### `heal`
Reconnect a project directory to an existing left_pocket. Moves the selected left_pocket's contents into the deterministic left_pocket path for the PROJECT. If that target left_pocket already exists, it is moved aside under `~/.left_pocket/unhoused/`, with fallback to legacy roots before replacement.

```bash
left_pocket heal --project ~/dev/app --left_pocket abc123
left_pocket heal --project . --left_pocket ~/.left_pocket/oldhash
left_pocket heal --alias myproject --left_pocket ~/.left_pocket/xyz789
```

**Options:**
- `--project PATH` — Project directory (conflicts with `--alias`)
- `--alias ALIAS` — Alias name (conflicts with `--project`)
- `--left_pocket LEFT_POCKET` — left_pocket directory to move

#### `clean [SCOPE]`
Remove registry entries or left_pocket directories in bulk.

```bash
left_pocket clean temporary              # Remove temporary left_pockets
left_pocket clean --all --hard           # Delete all left_pockets
left_pocket clean --older-than "7 days"  # Remove old left_pockets
left_pocket clean temporary -y           # Skip confirmation
```

**Scopes:**
- `temporary` — Remove temporary left_pockets only

**Options:**
- `--older-than AGE` — Remove left_pockets older than AGE (e.g., "7 days", "2 weeks")
- `--all` — Remove all left_pockets
- `--hard` — Delete directories (not just registry entries)
- `-y`, `--yes` — Skip confirmation

#### `sync-registry-git`
Refresh the top-level `~/.left_pocket` git snapshot. Copies every left_pocket into `~/.left_pocket/snapshots` without nested `.git` directories so the registry root repository can version all left_pocket contents together.

```bash
left_pocket sync-registry-git
```

#### `backup`
Configure a cron job that backs up `~/.left_pocket` to a git remote. The backup mirror lives at `~/.left_pocket_backup_repo` and is pushed by cron.

```bash
left_pocket backup --repo git@github.com:you/left_pocket-backup.git
left_pocket backup --repo git@github.com:you/left_pocket-backup.git --schedule "0 */6 * * *"  # every 6 hours
```

**Options:**
- `--repo GIT_URL` — Git remote for backups (required)
- `--schedule CRON` — Cron schedule (default: `0 * * * *` — every hour)

---

### Development & Configuration

#### `tests`
Run a post-installation, real-world operational suite against the **currently
running left_pocket executable**. The harness does not call internal workspace or
template functions and does not assume a source checkout exists.

```bash
left_pocket tests --all
left_pocket tests --all --verbose
left_pocket tests --all -i .
```

Without `-i`, left_pocket creates an isolated temporary `HOME`, config tree, registry,
projects, and fake `code` executable. It tests:

- creating and locating a normal left_pocket inside the temporary isolated HOME;
- `.env` roots and quiet template merging;
- repeated/open idempotency and reverse-sync prevention;
- runtime merge start/stop and preservation outside managed markers;
- `left_pocket -u` replacement versus quiet-merge preservation;
- augment add/remove/idempotency;
- alias lifecycle;
- healing a left_pocket to a different project;
- per-left_pocket OpenCode agent placement;
- unresolved config-root artifacts; and
- `upgrade-installation` idempotency.

It prints every command and a final PASS/FAIL/SKIP checklist, then removes only
the harness-owned temporary root. It never invokes `left_pocket clean --all`,
`left_pocket clean --hard`, remote backup configuration, or a real VS Code process.

With `-i PATH`, left_pocket uses read-only `locate` on the original. If it has a
left_pocket, a complete byte-identical backup is retained under
`<registry>/real-world-test-backups/`; unregistered projects are supported too.
The supplied project is copied (excluding generated `.git`, `node_modules`,
`target`, and `graphify-out` trees) into the isolated HOME, and the operational
suite runs against that retained copy. Only `.opencode` and artifact reporting
reads the original left_pocket. The original project/left_pocket is never opened,
upgraded, augmented, healed, restored, or deleted.

**Options:**
- `--all` - Add augment, alias, heal, OpenCode sync, artifact, and migration checks
- `-i`, `--include PATH` - Safely audit an existing project before isolated tests
- `--verbose` - Print stdout/stderr for every child command
- `--keep` - Retain the default isolated fixture instead of removing it

#### `completions SHELL`
Print a shell completion script to stdout. Generates tab-completion definitions for your shell. Pipe the output to the appropriate location for your shell, then source it.

```bash
# BASH
left_pocket completions bash > ~/.local/share/bash-completion/completions/left_pocket

# ZSH (add ~/.zsh/completions to fpath first)
left_pocket completions zsh > ~/.zsh/completions/_left_pocket

# FISH
left_pocket completions fish > ~/.config/fish/completions/left_pocket.fish

# POWERSHELL
left_pocket completions powershell >> $PROFILE

# ELVISH
left_pocket completions elvish >> ~/.config/elvish/rc.elv
```

**Supported shells:**
- `bash`
- `zsh`
- `fish`
- `powershell`
- `elvish`

#### `completion-spec`
Print a concise machine-readable completion model. This JSON is intended for editor and shell integrations that want richer nested command data than a single shell script can comfortably expose.

```bash
left_pocket completion-spec
```

#### `daily-feature`
Resolve (and create) today's daily feature file for the VS Code hotkey. Outputs JSON describing the file to open. When `--new` is passed, always creates the next numbered file (YYYY_MM_DD_N.md); otherwise reuses any existing feature file dated today, creating a base file if none exists.

Newly created files are seeded with feature tags whose definition in `feature_tags.yaml` sets `place_automatically: true`.

```bash
left_pocket daily-feature --left_pocket ~/.left_pocket/abc123
left_pocket daily-feature --left_pocket ~/.left_pocket/abc123 --new
left_pocket daily-feature --left_pocket ~/.left_pocket/abc123 --subpath dailies
```

**Options:**
- `--left_pocket PATH` — left_pocket directory containing the FEATURES folder (required)
- `--new` — Always create a new numbered daily feature file
- `--subpath PATH` — Subfolder under FEATURES where new daily files are created

---

### Worktree Management

#### `worktree`
Manage git worktrees that share this left_pocket. Worktrees are additional project directories (typically git worktrees of the same repo) that share the same left_pocket — meaning the same copilot instructions, FEATURES notes, and observations apply to all of them.

```bash
# Register a worktree to share the left_pocket for the current directory
left_pocket worktree add ~/dev/my-project-feature-x

# Remove a previously registered worktree
left_pocket worktree remove ~/dev/my-project-feature-x

# List all worktrees sharing this left_pocket
left_pocket worktree list
```

**Subcommands:**

##### `worktree add [PATH]`
Register a worktree directory to share this left_pocket. Run from inside the main project directory (or any directory that already belongs to a left_pocket). left_pocket will also suggest any git worktrees it detects in the same repo if PATH is not provided explicitly.

```bash
left_pocket worktree add ~/dev/my-project-feature-x
left_pocket worktree add  # auto-suggest git worktrees
```

##### `worktree remove PATH`
Unregister a worktree directory from this left_pocket.

```bash
left_pocket worktree remove ~/dev/my-project-feature-x
```

##### `worktree list`
List all worktrees registered to this left_pocket.

```bash
left_pocket worktree list
```

---

### Task Tracking

#### `task`
Built-in issue tracker (replaces Beads). Tasks live in a global SQLite database at `~/.left_pocket/global_data/tasks.db`, with fallback to legacy registry roots when needed. Tasks are grouped per left_pocket by a prefix derived from the left_pocket directory name. When run from inside a registered project, the correct prefix is detected automatically.

Task IDs are formatted as `<prefix>-<hash>` (e.g., `27472722730d-AB12CD`).

**Priority levels:**
- `0` — Critical (security, data loss, broken builds)
- `1` — High (major features, important bugs)
- `2` — Medium (default)
- `3` — Low (polish, optimization)
- `4` — Backlog (future ideas)

**Task statuses:**
- `open` — Not started
- `in_progress` — Currently being worked on
- `closed` — Completed
- `discarded` — Cancelled or invalid

##### `task list`
List all tasks for the current project (or specified project). Shows open and in-progress tasks by default.

```bash
left_pocket task list
left_pocket task list --priority 1              # only P0 and P1
left_pocket task list --project ~/dev/api
left_pocket task list --raw                     # JSON output
```

**Options:**
- `--priority N` — Filter by priority level (0-4)
- `--project PATH` — Project path (auto-detected if in registered project)
- `--raw` — Machine-readable JSON output

##### `task create`
Create a new task.

```bash
left_pocket task create --named "Implement feature X" \
  --description "Why this matters and what to do" --priority 1
```

**Options:**
- `--named "TITLE"` — Task title (required)
- `--description "TEXT"` — Task description
- `--priority N` — Priority 0-4 (default: 2)
- `--project PATH` — Project path (auto-detected if in registered project)

##### `task <ID> assign`
Assign a task to an agent.

```bash
left_pocket task 27472722730d-AB12CD assign --agent "Builder"
```

**Options:**
- `--agent "NAME"` — Agent name (e.g., "Builder", "Critic", "Documenter")

##### `task <ID> start`
Start working on a task (moves to in_progress status).

```bash
left_pocket task 27472722730d-AB12CD start
left_pocket task 27472722730d-AB12CD start --notes "Starting implementation"
```

**Options:**
- `--notes "TEXT"` — Optional start notes

##### `task <ID> log`
Add a progress log entry to a task.

```bash
left_pocket task 27472722730d-AB12CD log --notes "Completed API endpoints"
```

**Options:**
- `--notes "TEXT"` — Log entry (required)

##### `task <ID> close`
Close a task (mark as completed).

```bash
left_pocket task 27472722730d-AB12CD close
left_pocket task 27472722730d-AB12CD close --notes "Released in v1.2.0"
```

**Options:**
- `--notes "TEXT"` — Closing notes (optional)

##### `task <ID> discard`
Discard a task (mark as cancelled or invalid).

```bash
left_pocket task 27472722730d-AB12CD discard
```

##### `task <ID> describe`
Show full details for a task.

```bash
left_pocket task 27472722730d-AB12CD describe
left_pocket task 27472722730d-AB12CD describe --raw  # JSON output
```

**Options:**
- `--raw` — Machine-readable JSON output

##### `task reprefix`
Rename task prefix (when a left_pocket is reorganized or renamed).

```bash
left_pocket task reprefix --from 27472722730d --to abc123def456
```

**Options:**
- `--from OLD` — Old prefix (required)
- `--to NEW` — New prefix (required)

---

## Configuration

### Aliases

Aliases are stored in the registry root's `aliases` file, preferring `~/.left_pocket/aliases` and falling back to legacy registry roots when needed. Register them via CLI:

```bash
left_pocket register api="~/dev/my-api"
left_pocket register frontend="~/dev/my-frontend"
```

Or use the aliases immediately:

```bash
left_pocket -i api -i frontend
```

### Templates

Customize files written into every new left_pocket by editing templates in `~/.config/left_pocket/templates/`, with fallback to legacy config roots when those directories already exist. Each template file must begin with:

```
#LOCKET_TEMPLATE_DESTINATION: <relative-path>
```

Supported directives:

| Directive | Meaning |
| --- | --- |
| `#LOCKET_TEMPLATE_DESTINATION: <path>` | Where the template is Placed inside a left_pocket. A file may contain several, each starting a new block. |
| `#LOCKET_INSTALL_DESTINATION: <path>` | Place this file at an arbitrary path at **install** time only (e.g. `directory_structure.yaml` lands directly in `~/.config/left_pocket`, not under `templates/`). Stripped from the placed file. |
| `#LOCKET_QUIET_MERGE` | Merge into an existing destination file instead of overwriting, de-duplicating `KEY=VALUE` lines. Used for `.env` and `.gitignore`. |
| `#LOCKET_MERGE_AT_RUNTIME` | Inject content when a session opens, wrapped in `#LOCKET_RUNTIME_CONTENT_START` / `#LOCKET_RUNTIME_CONTENT_END`, and strip it when the session closes. |

The legacy `#SPOCKET_*` spellings of all of the above are still recognised when
reading templates and already-placed files. New content is always written with
the `#LOCKET_` prefix. Run [`left_pocket upgrade-installation`](#left_pocket-upgrade-installation)
to migrate old files in place.

Supported variables:
- `{{LOCKET_ROOT}}` / `{{SPOCKET_ROOT}}` — Absolute path to the left_pocket directory (`LOCKET_*` preferred)
- `{{LOCKET_NAME}}` / `{{SPOCKET_NAME}}` — Hash-based left_pocket identifier (`LOCKET_*` preferred)
- `{{LOCKET_CONFIG_ROOT}}` / `{{SPOCKET_CONFIG_ROOT}}` — Preferred config root with fallback support
- `{{LOCKET_REGISTRY_ROOT}}` / `{{SPOCKET_REGISTRY_ROOT}}` — Preferred registry root with fallback support
- `{{PROJECT_ROOT}}` — Absolute path to the first included project

Directory structure is controlled by `~/.config/left_pocket/directory_structure.yaml`.

#### Placement

left_pocket never syncs *from* a project *to* the templates. Templates are **Placed**,
always one-way, in one of three ways:

1. **Into config**, at install time — `src/templates` is materialised into
   `~/.config/left_pocket` (honouring `#LOCKET_INSTALL_DESTINATION`).
2. **Into a project**, when a left_pocket is created with `left_pocket -i .` — templates are
   Placed per their `#LOCKET_TEMPLATE_DESTINATION`. Edits you then make to the
   *placed* copies never travel back to the templates.
3. **At runtime into a project**, when a session opens — templates marked
   `#LOCKET_MERGE_AT_RUNTIME` are merged into their destination between the
   runtime markers, and removed again when the session closes. Content you write
   above or below the markers is preserved.

`left_pocket -u <path>` re-Places templates over an existing left_pocket. This is
destructive to the placed copies: local edits inside the left_pocket are replaced with
the current template content.

#### `left_pocket upgrade-installation`

Rewrites legacy `#SPOCKET_*` directives and runtime markers to `#LOCKET_*` in
place, across every known config root (`~/.config/left_pocket`,
`~/.config/safe_pocket`, `~/.config/spocket`) and registry root (`~/.left_pocket`,
`~/.safe_pocket`, `~/.spocket`):

```bash
left_pocket upgrade-installation --dry-run   # preview, change nothing
left_pocket upgrade-installation             # prompt, then apply
left_pocket upgrade-installation --yes       # apply without prompting
left_pocket upgrade-installation --root ~/some/other/tree
left_pocket upgrade-installation --clean-literal-root-artifacts  # lists, backs up, prompts
```

This is a one-way text migration, not a sync — it never copies content from a
project back into the config templates directory. User-facing feature-tag names
such as `SPOCKET_MUST_INSTALL` are deliberately left untouched, since they are
defined in `feature_tags.yaml` and referenced from your feature files.

The command also reports active-left_pocket directories whose literal name is
`{{SPOCKET_CONFIG_ROOT}}` or `{{LOCKET_CONFIG_ROOT}}`. These are artifacts from
an older install-time template bug. Historical `snapshots/` and `unhoused/`
archives are intentionally excluded. Artifacts are report-only by default. Cleanup is
deliberately conservative: `--clean-literal-root-artifacts` accepts only a
directory containing exactly one regular `feature_tags.yaml`, copies that file
to `~/.left_pocket/upgrade-backups/`, lists every target, and requires typing
`REMOVE` (or separately supplying `--yes`). Any unexpected contents cause that
directory to be skipped.

To replace your installed templates outright with the ones built into the binary
(clearing stale/legacy filenames):

```bash
left_pocket install-default-assets --replace
```

### Directory Structure

The default left_pocket directory structure is:

```
~/.left_pocket/<hash>/
├── .code-workspace       # VS Code workspace file
├── .github/
│  └── copilot-instructions.md
├── AGENTS.md            # Agent definitions
├── FEATURES/            # Daily feature notes
│  └── dailies/
├── observations/        # Session notes
├── README.md
└── manifest.json        # left_pocket metadata
```

Customize by editing `~/.config/left_pocket/directory_structure.yaml`.

---

## left_pocket Directory Location

left_pockets are stored in `~/.left_pocket/<hash>/` where `<hash>` is derived from the project directory path, ensuring deterministic left_pocket association across sessions. If a left_pocket root does not exist yet, the runtime falls back to legacy `~/.safe_pocket/` and `~/.spocket/` roots.

View all left_pockets:

```bash
left_pocket list-workspaces
```

### The `.opencode` Directory

left_pocket renders its managed OpenCode agent definitions into
`<left_pocket>/.opencode/agent/` whenever a left_pocket is created/opened or
`left_pocket sync agents` runs. Those Markdown files are generated and small; deleting
them is safe, but left_pocket will recreate them.

left_pocket does **not** install `.opencode/node_modules`, `package.json`, or
`package-lock.json`. If those exist, an OpenCode/plugin/npm setup created them.
They may be removed if the project does not rely on local OpenCode plugins or
dependencies, but left_pocket deliberately does not remove them. Deleting the whole
`.opencode` directory also removes any hand-authored OpenCode configuration;
only left_pocket's managed `agent/` files will come back automatically.

---

## Environment Variables

- `LOCKET_ROOT` — Preferred path to the left_pocket directory
- `SPOCKET_ROOT` — Legacy compatibility alias for `LOCKET_ROOT`
- `PROJECT_ROOT` — (internal) Path to the first included project
- `LOCKET_NAME` / `SPOCKET_NAME` — Internal left_pocket hash identifier (template variables)

---

## Tips & Tricks

### Create a temporary workspace for experimentation

```bash
left_pocket -i . --temporary --silent
```

### Merge two existing projects into one workspace

```bash
left_pocket -i ~/dev/api -i ~/dev/frontend
```

### Switch a project to a different left_pocket

```bash
left_pocket heal --project ~/dev/app --left_pocket ~/.left_pocket/new-hash
```

### Upgrade all left_pocket templates

```bash
left_pocket sync agents
```

### Clean up old temporary left_pockets

```bash
left_pocket clean temporary --hard -y
```

### Generate shell completions (ZSH)

```bash
left_pocket completions zsh > ~/.zsh/completions/_left_pocket
# Then add to ~/.zshrc:
# fpath=(~/.zsh/completions $fpath)
# autoload -Uz compinit && compinit
```

---

## See Also

- `~/.left_pocket/aliases` — Preferred alias registry file
- `~/.left_pocket/snapshots/` — Preferred git snapshot of all left_pockets
- `~/.left_pocket/global_data/tasks.db` — Preferred global tasks database
- `~/.config/left_pocket/` — Preferred user configuration and templates
