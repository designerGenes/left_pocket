```
╭────────╮
│        │
│     ╭──┤   corner
│     │▓▓│
╰─────┴──╯
```

# Corner

**corner** (compatibility aliases: `safe_pocket`, `spocket`) — ad hoc VS Code workspace manager with AI copilot support.

Keep "meta" files (copilot instructions, prompts, observations, feature notes) in a dedicated corner directory (`~/.corner/<hash>/`) so they never pollute your project repo, yet VS Code opens them together with your project as a single multi-root workspace.

Corner prefers the new roots `~/.corner/` and `~/.config/corner/`, but it falls back to legacy `~/.safe_pocket/`, `~/.spocket/`, `~/.config/safe_pocket/`, and `~/.config/spocket/` locations when the matching files or directories only exist there.

---

To understand Corner, read the [documentation](documentation/Concepts.md) or run `corner --help` for a quick overview of commands and flags.

Reading the documentation is highly recommended.

---

## Quick Start

```bash
# Create or open a workspace for the current directory
corner -i .

# Create a workspace spanning two projects
corner -i ~/dev/frontend -i ~/dev/backend

# Upgrade corner templates to match latest config
corner -u ~/dev/myproject

# Check which corner a project belongs to
corner locate --path ~/dev/myproject

# Generate shell completions (zsh)
corner completions zsh > ~/.zsh/completions/_corner
```

---

## Command Hierarchy

```
corner
│
├─ GLOBAL FLAGS (always available)
│  ├─ -i, --include PATH      Add directory to workspace (repeatable)
│  ├─ -s, --sidecar PATH      Add temporary sidecar (session only)
│  ├─ --with TOOL             Add tool for this session only
│  ├─ --add TOOL              Install tool for future sessions
│  ├─ -u, --upgrade PATH      Upgrade corner templates
│  ├─ --clone-from PATH       Clone corner from another project
│  ├─ --temporary             Use ~/.corner/temporary
│  ├─ --force-new             Create new corner even if one exists
│  ├─ --no-readme             Skip README generation
│  ├─ --simulate-runtime      Inject runtime content without launching VS Code
│  ├─ --silent                Setup workspace without opening VS Code
│  ├─ --verbose               Show informational messages
│  └─ -v                      Print version
│
├─ register NAME="PATH"        Register directory alias
├─ unregister NAME             Remove directory alias
├─ list-aliases                List all aliases
├─ list-workspaces             List all known corners
│
├─ sync [TARGET]               Sync assets or manifest
│  └─ TARGET: agents | all
│
├─ augment                      Add/remove directories in-place
│  ├─ --add PATH               Add directory
│  ├─ --remove PATH            Remove directory
│  └─ --no-open                Don't open VS Code
│
├─ mark MARK CORNER            Mark corner with metadata
│  └─ MARK: temporary
│
├─ clean [SCOPE]               Clean corners in bulk
│  ├─ SCOPE: temporary
│  ├─ --older-than AGE         Clean corners older than AGE
│  ├─ --all                    Clean all corners
│  ├─ --hard                   Delete directories (not just registry)
│  └─ -y, --yes                Skip confirmation
│
├─ heal                         Reconnect project to existing corner
│  ├─ --project PATH           Project directory
│  ├─ --alias ALIAS            Alias name (instead of path)
│  └─ --corner CORNER          Corner directory
│
├─ locate                       Find corner for a path
│  └─ --path PATH              Project path (default: .)
│
├─ sync-registry-git            Refresh git snapshot of all corners
│
├─ upgrade-installation         Rewrite legacy #SPOCKET_* tokens to #CORNER_*
│  ├─ --dry-run                Preview rewrites without changing files
│  ├─ -y, --yes                Apply without prompting
│  └─ --root PATH              Extra root to scan (repeatable)
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
│  ├─ --corner PATH            Corner directory
│  ├─ --new                    Always create new file
│  └─ --subpath PATH           Subfolder for daily files
│
├─ worktree ACTION              Manage git worktrees sharing this corner
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
corner -i ~/dev/frontend -i ~/dev/backend
```

#### `-s`, `--sidecar PATH` (repeatable)
Add a temporary sidecar directory (not saved to workspace file). Injected for this session only and removed next time the workspace opens normally. Useful for pulling in dependencies or reference repositories.

```bash
corner -i . -s ~/external/lib
```

#### `--with TOOL` (repeatable)
Add a tool for this session only. Tools are exposed as managed sidecar folders when the workspace opens but are not installed for future sessions.

Supported tools: `gitleaks`, `graphify`, `memgraph`

```bash
corner -i . --with gitleaks --with graphify
```

#### `--add TOOL` (repeatable)
Install a tool into the project and corner for future sessions. If already installed, this is a no-op.

Supported tools: `gitleaks`, `graphify`, `memgraph`

```bash
corner -i . --add graphify
```

#### `--clone-from PATH`
Clone the corner from the workspace containing this path. Copies all meta files (copilot instructions, prompts, observations, etc.) from the source corner into the new one, then tracks lineage in both manifests. Useful when starting a new project that should inherit AI configuration from a related one.

```bash
corner -i ~/dev/new-project --clone-from ~/dev/existing-project
```

#### `-u`, `--upgrade PATH`
Upgrade an existing corner to match current templates (does not open VS Code). Reads every template from the preferred config root, starting with `~/.config/corner/templates/` and falling back to legacy config roots when needed, then expands variables and writes the result to the corner. If a file already exists with different content, you are shown a diff and asked to confirm.

PATH may be either the corner directory itself or any project directory whose corner you want to upgrade.

```bash
corner -u ~/dev/myproject
corner -u ~/.corner/abc123
```

#### `--new`
Force creation of a new workspace even if one already exists. By default, if a project already belongs to an existing corner, that corner is opened instead of creating a duplicate.

```bash
corner -i . --new
```

#### `--temporary`
Create or reuse the corner under `~/.corner/temporary/`. Temporary corners are tracked separately so test suites and other short-lived workflows can be cleaned up without touching normal corners.

```bash
corner -i . --temporary
```

#### `--no-readme`
Skip creating README files in empty directories. By default Corner writes helpful README.md files into new empty directories (observations/, .github/prompts/, etc.).

```bash
corner -i . --no-readme
```

### Execution Control

#### `--simulate-runtime`
Inject runtime content into destination files without launching VS Code. Every file that would normally gain inject-at-runtime content (wrapped in `#CORNER_RUNTIME_CONTENT_START` / `#CORNER_RUNTIME_CONTENT_END` markers) gains that content even though VS Code is never started. Implies `--silent`.

```bash
corner -i . --simulate-runtime --temporary
```

#### `--silent`
Perform every step except opening VS Code at the end. Useful for exercising Corner setup without launching the editor.

```bash
corner -i . --silent
```

#### `--verbose`
Enable verbose output. Shows informational messages that are hidden by default, such as runtime merge notifications, template installation notices, and other non-error details.

```bash
corner -i . --verbose
```

#### `-v`
Print version and exit.

---

## Subcommands

### Alias Management

#### `register NAME="PATH"`
Register a short alias for a directory path. Aliases let you refer to long directory paths by a short name in any `corner` command that accepts a PATH argument.

```bash
corner register api="~/dev/my-api-project"
corner register frontend="~/dev/my-frontend"
corner -i api -i frontend  # use aliases
```

#### `unregister NAME`
Remove a previously registered directory alias.

```bash
corner unregister api
```

#### `list-aliases`
List all registered directory aliases.

```bash
corner list-aliases
```

#### `list-workspaces`
List all known corners with their project paths and status.

```bash
corner list-workspaces
```

---

### Workspace Management

#### `sync [TARGET]`
Sync system-wide assets or the manifest.

**With a TARGET**, synchronizes system-wide Corner assets that every corner draws from:

- `agents` — Write unified agent definitions from `~/.config/corner/templates/agents/` into the places OpenCode looks for agents
- `all` — Run every system-wide sync (currently agents only)

**Without a TARGET** (legacy form used by VS Code extension), updates the corner manifest to reflect the current .code-workspace folders and prints JSON. Requires `--corner`. You rarely need to run this manually.

```bash
corner sync agents      # Write agent definitions
corner sync all         # Sync everything
corner sync --corner ~/.corner/abc123  # Manifest sync (internal)
```

#### `augment`
Add or remove project directories from the current workspace in-place. Rewrites the .code-workspace file and manifest without moving the corner directory. Run from inside a corner or project directory that belongs to an existing workspace.

```bash
corner augment --add ~/dev/new-service
corner augment --remove ~/dev/old-service
corner augment --add ~/dev/new-service --no-open
```

**Options:**
- `--add PATH` — Project directory to add to the workspace (repeatable)
- `--remove PATH` — Project directory to remove from the workspace (repeatable)
- `--no-open` — Update workspace without opening VS Code afterwards

#### `mark MARK CORNER`
Mark an existing corner with additional metadata.

```bash
corner mark temporary ~/.corner/abc123
```

**Mark types:**
- `temporary` — Mark corner as temporary

#### `locate`
Locate the corner associated with a project or corner path. Outputs JSON for editor integrations.

```bash
corner locate --path ~/dev/myproject
```

**Options:**
- `--path PATH` — Project path to locate (default: current directory)

---

### Corner Maintenance

#### `heal`
Reconnect a project directory to an existing corner. Moves the selected corner's contents into the deterministic corner path for the PROJECT. If that target corner already exists, it is moved aside under `~/.corner/unhoused/`, with fallback to legacy roots before replacement.

```bash
corner heal --project ~/dev/app --corner abc123
corner heal --project . --corner ~/.corner/oldhash
corner heal --alias myproject --corner ~/.corner/xyz789
```

**Options:**
- `--project PATH` — Project directory (conflicts with `--alias`)
- `--alias ALIAS` — Alias name (conflicts with `--project`)
- `--corner CORNER` — Corner directory to move

#### `clean [SCOPE]`
Remove registry entries or corner directories in bulk.

```bash
corner clean temporary              # Remove temporary corners
corner clean --all --hard           # Delete all corners
corner clean --older-than "7 days"  # Remove old corners
corner clean temporary -y           # Skip confirmation
```

**Scopes:**
- `temporary` — Remove temporary corners only

**Options:**
- `--older-than AGE` — Remove corners older than AGE (e.g., "7 days", "2 weeks")
- `--all` — Remove all corners
- `--hard` — Delete directories (not just registry entries)
- `-y`, `--yes` — Skip confirmation

#### `sync-registry-git`
Refresh the top-level `~/.corner` git snapshot. Copies every corner into `~/.corner/snapshots` without nested `.git` directories so the registry root repository can version all corner contents together.

```bash
corner sync-registry-git
```

#### `backup`
Configure a cron job that backs up `~/.corner` to a git remote. The backup mirror lives at `~/.corner_backup_repo` and is pushed by cron.

```bash
corner backup --repo git@github.com:you/corner-backup.git
corner backup --repo git@github.com:you/corner-backup.git --schedule "0 */6 * * *"  # every 6 hours
```

**Options:**
- `--repo GIT_URL` — Git remote for backups (required)
- `--schedule CRON` — Cron schedule (default: `0 * * * *` — every hour)

---

### Development & Configuration

#### `completions SHELL`
Print a shell completion script to stdout. Generates tab-completion definitions for your shell. Pipe the output to the appropriate location for your shell, then source it.

```bash
# BASH
corner completions bash > ~/.local/share/bash-completion/completions/corner

# ZSH (add ~/.zsh/completions to fpath first)
corner completions zsh > ~/.zsh/completions/_corner

# FISH
corner completions fish > ~/.config/fish/completions/corner.fish

# POWERSHELL
corner completions powershell >> $PROFILE

# ELVISH
corner completions elvish >> ~/.config/elvish/rc.elv
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
corner completion-spec
```

#### `daily-feature`
Resolve (and create) today's daily feature file for the VS Code hotkey. Outputs JSON describing the file to open. When `--new` is passed, always creates the next numbered file (YYYY_MM_DD_N.md); otherwise reuses any existing feature file dated today, creating a base file if none exists.

Newly created files are seeded with feature tags whose definition in `feature_tags.yaml` sets `place_automatically: true`.

```bash
corner daily-feature --corner ~/.corner/abc123
corner daily-feature --corner ~/.corner/abc123 --new
corner daily-feature --corner ~/.corner/abc123 --subpath dailies
```

**Options:**
- `--corner PATH` — Corner directory containing the FEATURES folder (required)
- `--new` — Always create a new numbered daily feature file
- `--subpath PATH` — Subfolder under FEATURES where new daily files are created

---

### Worktree Management

#### `worktree`
Manage git worktrees that share this corner. Worktrees are additional project directories (typically git worktrees of the same repo) that share the same corner — meaning the same copilot instructions, FEATURES notes, and observations apply to all of them.

```bash
# Register a worktree to share the corner for the current directory
corner worktree add ~/dev/my-project-feature-x

# Remove a previously registered worktree
corner worktree remove ~/dev/my-project-feature-x

# List all worktrees sharing this corner
corner worktree list
```

**Subcommands:**

##### `worktree add [PATH]`
Register a worktree directory to share this corner. Run from inside the main project directory (or any directory that already belongs to a corner). Corner will also suggest any git worktrees it detects in the same repo if PATH is not provided explicitly.

```bash
corner worktree add ~/dev/my-project-feature-x
corner worktree add  # auto-suggest git worktrees
```

##### `worktree remove PATH`
Unregister a worktree directory from this corner.

```bash
corner worktree remove ~/dev/my-project-feature-x
```

##### `worktree list`
List all worktrees registered to this corner.

```bash
corner worktree list
```

---

### Task Tracking

#### `task`
Built-in issue tracker (replaces Beads). Tasks live in a global SQLite database at `~/.corner/global_data/tasks.db`, with fallback to legacy registry roots when needed. Tasks are grouped per corner by a prefix derived from the corner directory name. When run from inside a registered project, the correct prefix is detected automatically.

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
corner task list
corner task list --priority 1              # only P0 and P1
corner task list --project ~/dev/api
corner task list --raw                     # JSON output
```

**Options:**
- `--priority N` — Filter by priority level (0-4)
- `--project PATH` — Project path (auto-detected if in registered project)
- `--raw` — Machine-readable JSON output

##### `task create`
Create a new task.

```bash
corner task create --named "Implement feature X" \
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
corner task 27472722730d-AB12CD assign --agent "Builder"
```

**Options:**
- `--agent "NAME"` — Agent name (e.g., "Builder", "Critic", "Documenter")

##### `task <ID> start`
Start working on a task (moves to in_progress status).

```bash
corner task 27472722730d-AB12CD start
corner task 27472722730d-AB12CD start --notes "Starting implementation"
```

**Options:**
- `--notes "TEXT"` — Optional start notes

##### `task <ID> log`
Add a progress log entry to a task.

```bash
corner task 27472722730d-AB12CD log --notes "Completed API endpoints"
```

**Options:**
- `--notes "TEXT"` — Log entry (required)

##### `task <ID> close`
Close a task (mark as completed).

```bash
corner task 27472722730d-AB12CD close
corner task 27472722730d-AB12CD close --notes "Released in v1.2.0"
```

**Options:**
- `--notes "TEXT"` — Closing notes (optional)

##### `task <ID> discard`
Discard a task (mark as cancelled or invalid).

```bash
corner task 27472722730d-AB12CD discard
```

##### `task <ID> describe`
Show full details for a task.

```bash
corner task 27472722730d-AB12CD describe
corner task 27472722730d-AB12CD describe --raw  # JSON output
```

**Options:**
- `--raw` — Machine-readable JSON output

##### `task reprefix`
Rename task prefix (when a corner is reorganized or renamed).

```bash
corner task reprefix --from 27472722730d --to abc123def456
```

**Options:**
- `--from OLD` — Old prefix (required)
- `--to NEW` — New prefix (required)

---

## Configuration

### Aliases

Aliases are stored in the registry root's `aliases` file, preferring `~/.corner/aliases` and falling back to legacy registry roots when needed. Register them via CLI:

```bash
corner register api="~/dev/my-api"
corner register frontend="~/dev/my-frontend"
```

Or use the aliases immediately:

```bash
corner -i api -i frontend
```

### Templates

Customize files written into every new corner by editing templates in `~/.config/corner/templates/`, with fallback to legacy config roots when those directories already exist. Each template file must begin with:

```
#CORNER_TEMPLATE_DESTINATION: <relative-path>
```

Supported directives:

| Directive | Meaning |
| --- | --- |
| `#CORNER_TEMPLATE_DESTINATION: <path>` | Where the template is Placed inside a corner. A file may contain several, each starting a new block. |
| `#CORNER_INSTALL_DESTINATION: <path>` | Place this file at an arbitrary path at **install** time only (e.g. `directory_structure.yaml` lands directly in `~/.config/corner`, not under `templates/`). Stripped from the placed file. |
| `#CORNER_QUIET_MERGE` | Merge into an existing destination file instead of overwriting, de-duplicating `KEY=VALUE` lines. Used for `.env` and `.gitignore`. |
| `#CORNER_MERGE_AT_RUNTIME` | Inject content when a session opens, wrapped in `#CORNER_RUNTIME_CONTENT_START` / `#CORNER_RUNTIME_CONTENT_END`, and strip it when the session closes. |

The legacy `#SPOCKET_*` spellings of all of the above are still recognised when
reading templates and already-placed files. New content is always written with
the `#CORNER_` prefix. Run [`corner upgrade-installation`](#corner-upgrade-installation)
to migrate old files in place.

Supported variables:
- `{{CORNER_ROOT}}` / `{{SPOCKET_ROOT}}` — Absolute path to the corner directory (`CORNER_*` preferred)
- `{{CORNER_NAME}}` / `{{SPOCKET_NAME}}` — Hash-based corner identifier (`CORNER_*` preferred)
- `{{CORNER_CONFIG_ROOT}}` / `{{SPOCKET_CONFIG_ROOT}}` — Preferred config root with fallback support
- `{{CORNER_REGISTRY_ROOT}}` / `{{SPOCKET_REGISTRY_ROOT}}` — Preferred registry root with fallback support
- `{{PROJECT_ROOT}}` — Absolute path to the first included project

Directory structure is controlled by `~/.config/corner/directory_structure.yaml`.

#### Placement

Corner never syncs *from* a project *to* the templates. Templates are **Placed**,
always one-way, in one of three ways:

1. **Into config**, at install time — `src/templates` is materialised into
   `~/.config/corner` (honouring `#CORNER_INSTALL_DESTINATION`).
2. **Into a project**, when a corner is created with `corner -i .` — templates are
   Placed per their `#CORNER_TEMPLATE_DESTINATION`. Edits you then make to the
   *placed* copies never travel back to the templates.
3. **At runtime into a project**, when a session opens — templates marked
   `#CORNER_MERGE_AT_RUNTIME` are merged into their destination between the
   runtime markers, and removed again when the session closes. Content you write
   above or below the markers is preserved.

`corner -u <path>` re-Places templates over an existing corner. This is
destructive to the placed copies: local edits inside the corner are replaced with
the current template content.

#### `corner upgrade-installation`

Rewrites legacy `#SPOCKET_*` directives and runtime markers to `#CORNER_*` in
place, across every known config root (`~/.config/corner`,
`~/.config/safe_pocket`, `~/.config/spocket`) and registry root (`~/.corner`,
`~/.safe_pocket`, `~/.spocket`):

```bash
corner upgrade-installation --dry-run   # preview, change nothing
corner upgrade-installation             # prompt, then apply
corner upgrade-installation --yes       # apply without prompting
corner upgrade-installation --root ~/some/other/tree
```

This is a one-way text migration, not a sync — it never copies content from a
project back into the config templates directory. User-facing feature-tag names
such as `SPOCKET_MUST_INSTALL` are deliberately left untouched, since they are
defined in `feature_tags.yaml` and referenced from your feature files.

To replace your installed templates outright with the ones built into the binary
(clearing stale/legacy filenames):

```bash
corner install-default-assets --replace
```

### Directory Structure

The default corner directory structure is:

```
~/.corner/<hash>/
├── .code-workspace       # VS Code workspace file
├── .github/
│  └── copilot-instructions.md
├── AGENTS.md            # Agent definitions
├── FEATURES/            # Daily feature notes
│  └── dailies/
├── observations/        # Session notes
├── README.md
└── manifest.json        # Corner metadata
```

Customize by editing `~/.config/corner/directory_structure.yaml`.

---

## Corner Directory Location

Corners are stored in `~/.corner/<hash>/` where `<hash>` is derived from the project directory path, ensuring deterministic corner association across sessions. If a Corner root does not exist yet, the runtime falls back to legacy `~/.safe_pocket/` and `~/.spocket/` roots.

View all corners:

```bash
corner list-workspaces
```

---

## Environment Variables

- `CORNER_ROOT` — Preferred path to the corner directory
- `SPOCKET_ROOT` — Legacy compatibility alias for `CORNER_ROOT`
- `PROJECT_ROOT` — (internal) Path to the first included project
- `CORNER_NAME` / `SPOCKET_NAME` — Internal corner hash identifier (template variables)

---

## Tips & Tricks

### Create a temporary workspace for experimentation

```bash
corner -i . --temporary --silent
```

### Merge two existing projects into one workspace

```bash
corner -i ~/dev/api -i ~/dev/frontend
```

### Switch a project to a different corner

```bash
corner heal --project ~/dev/app --corner ~/.corner/new-hash
```

### Upgrade all corner templates

```bash
corner sync agents
```

### Clean up old temporary corners

```bash
corner clean temporary --hard -y
```

### Generate shell completions (ZSH)

```bash
corner completions zsh > ~/.zsh/completions/_corner
# Then add to ~/.zshrc:
# fpath=(~/.zsh/completions $fpath)
# autoload -Uz compinit && compinit
```

---

## See Also

- `~/.corner/aliases` — Preferred alias registry file
- `~/.corner/snapshots/` — Preferred git snapshot of all corners
- `~/.corner/global_data/tasks.db` — Preferred global tasks database
- `~/.config/corner/` — Preferred user configuration and templates
