# Corner

**corner** (compatibility aliases: `safe_pocket`, `spocket`) — ad hoc VS Code workspace manager with AI copilot support.

Keep "meta" files (copilot instructions, prompts, observations, feature notes) in a dedicated pocket directory (`~/.corner/<hash>/`) so they never pollute your project repo, yet VS Code opens them together with your project as a single multi-root workspace.

Corner prefers the new roots `~/.corner/` and `~/.config/corner/`, but it falls back to legacy `~/.safe_pocket/`, `~/.spocket/`, `~/.config/safe_pocket/`, and `~/.config/spocket/` locations when the matching files or directories only exist there.

---

## Quick Start

```bash
# Create or open a workspace for the current directory
corner -i .

# Create a workspace spanning two projects
corner -i ~/dev/frontend -i ~/dev/backend

# Upgrade pocket templates to match latest config
corner -u ~/dev/myproject

# Check which pocket a project belongs to
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
│  ├─ -u, --upgrade PATH      Upgrade pocket templates
│  ├─ --clone-from PATH       Clone pocket from another project
│  ├─ --temporary             Use ~/.corner/temporary
│  ├─ --force-new             Create new pocket even if one exists
│  ├─ --no-readme             Skip README generation
│  ├─ --simulate-runtime      Inject runtime content without launching VS Code
│  ├─ --silent                Setup workspace without opening VS Code
│  ├─ --verbose               Show informational messages
│  └─ -v                      Print version
│
├─ register NAME="PATH"        Register directory alias
├─ unregister NAME             Remove directory alias
├─ list-aliases                List all aliases
├─ list-workspaces             List all known pockets
│
├─ sync [TARGET]               Sync assets or manifest
│  └─ TARGET: agents | all
│
├─ augment                      Add/remove directories in-place
│  ├─ --add PATH               Add directory
│  ├─ --remove PATH            Remove directory
│  └─ --no-open                Don't open VS Code
│
├─ mark MARK POCKET            Mark pocket with metadata
│  └─ MARK: temporary
│
├─ clean [SCOPE]               Clean pockets in bulk
│  ├─ SCOPE: temporary
│  ├─ --older-than AGE         Clean pockets older than AGE
│  ├─ --all                    Clean all pockets
│  ├─ --hard                   Delete directories (not just registry)
│  └─ -y, --yes                Skip confirmation
│
├─ heal                         Reconnect project to existing pocket
│  ├─ --project PATH           Project directory
│  ├─ --alias ALIAS            Alias name (instead of path)
│  └─ --pocket POCKET          Pocket directory
│
├─ locate                       Find pocket for a path
│  └─ --path PATH              Project path (default: .)
│
├─ sync-registry-git            Refresh git snapshot of all pockets
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
│  ├─ --pocket PATH            Pocket directory
│  ├─ --new                    Always create new file
│  └─ --subpath PATH           Subfolder for daily files
│
├─ worktree ACTION              Manage git worktrees sharing this pocket
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
safe_pocket -i ~/dev/frontend -i ~/dev/backend
```

#### `-s`, `--sidecar PATH` (repeatable)
Add a temporary sidecar directory (not saved to workspace file). Injected for this session only and removed next time the workspace opens normally. Useful for pulling in dependencies or reference repositories.

```bash
safe_pocket -i . -s ~/external/lib
```

#### `--with TOOL` (repeatable)
Add a tool for this session only. Tools are exposed as managed sidecar folders when the workspace opens but are not installed for future sessions.

Supported tools: `gitleaks`, `graphify`, `memgraph`

```bash
safe_pocket -i . --with gitleaks --with graphify
```

#### `--add TOOL` (repeatable)
Install a tool into the project and safe pocket for future sessions. If already installed, this is a no-op.

Supported tools: `gitleaks`, `graphify`, `memgraph`

```bash
safe_pocket -i . --add graphify
```

#### `--clone-from PATH`
Clone the pocket from the workspace containing this path. Copies all meta files (copilot instructions, prompts, observations, etc.) from the source pocket into the new one, then tracks lineage in both manifests. Useful when starting a new project that should inherit AI configuration from a related one.

```bash
safe_pocket -i ~/dev/new-project --clone-from ~/dev/existing-project
```

#### `-u`, `--upgrade PATH`
Upgrade an existing pocket to match current templates (does not open VS Code). Reads every template from `~/.config/safe_pocket/templates/`, expands variables, and writes the result to the pocket. If a file already exists with different content, you are shown a diff and asked to confirm.

PATH may be either the pocket directory itself or any project directory whose pocket you want to upgrade.

```bash
safe_pocket -u ~/dev/myproject
safe_pocket -u ~/.safe_pocket/abc123
```

#### `--new`
Force creation of a new workspace even if one already exists. By default, if a project already belongs to an existing safe pocket, that pocket is opened instead of creating a duplicate.

```bash
safe_pocket -i . --new
```

#### `--temporary`
Create or reuse the pocket under `~/.safe_pocket/temporary/`. Temporary pockets are tracked separately so test suites and other short-lived workflows can be cleaned up without touching normal pockets.

```bash
safe_pocket -i . --temporary
```

#### `--no-readme`
Skip creating README files in empty directories. By default safe_pocket writes helpful README.md files into new empty directories (observations/, .github/prompts/, etc.).

```bash
safe_pocket -i . --no-readme
```

### Execution Control

#### `--simulate-runtime`
Inject runtime content into destination files without launching VS Code. Every file that would normally gain inject-at-runtime content (wrapped in `#SPOCKET_RUNTIME_CONTENT_START` / `#SPOCKET_RUNTIME_CONTENT_END` markers) gains that content even though VS Code is never started. Implies `--silent`.

```bash
safe_pocket -i . --simulate-runtime --temporary
```

#### `--silent`
Perform every step except opening VS Code at the end. Useful for exercising safe_pocket setup without launching the editor.

```bash
safe_pocket -i . --silent
```

#### `--verbose`
Enable verbose output. Shows informational messages that are hidden by default, such as runtime merge notifications, template installation notices, and other non-error details.

```bash
safe_pocket -i . --verbose
```

#### `-v`
Print version and exit.

---

## Subcommands

### Alias Management

#### `register NAME="PATH"`
Register a short alias for a directory path. Aliases let you refer to long directory paths by a short name in any safe_pocket command that accepts a PATH argument.

```bash
safe_pocket register api="~/dev/my-api-project"
safe_pocket register frontend="~/dev/my-frontend"
safe_pocket -i api -i frontend  # use aliases
```

#### `unregister NAME`
Remove a previously registered directory alias.

```bash
safe_pocket unregister api
```

#### `list-aliases`
List all registered directory aliases.

```bash
safe_pocket list-aliases
```

#### `list-workspaces`
List all known safe pockets with their project paths and status.

```bash
safe_pocket list-workspaces
```

---

### Workspace Management

#### `sync [TARGET]`
Sync system-wide assets or the manifest.

**With a TARGET**, synchronizes system-wide safe_pocket assets that every pocket draws from:

- `agents` — Write unified agent definitions from `~/.config/safe_pocket/templates/agents/` into the places OpenCode looks for agents
- `all` — Run every system-wide sync (currently agents only)

**Without a TARGET** (legacy form used by VS Code extension), updates the pocket manifest to reflect the current .code-workspace folders and prints JSON. Requires `--pocket`. You rarely need to run this manually.

```bash
safe_pocket sync agents      # Write agent definitions
safe_pocket sync all         # Sync everything
safe_pocket sync --pocket ~/.safe_pocket/abc123  # Manifest sync (internal)
```

#### `augment`
Add or remove project directories from the current workspace in-place. Rewrites the .code-workspace file and manifest without moving the pocket directory. Run from inside a pocket or project directory that belongs to an existing workspace.

```bash
safe_pocket augment --add ~/dev/new-service
safe_pocket augment --remove ~/dev/old-service
safe_pocket augment --add ~/dev/new-service --no-open
```

**Options:**
- `--add PATH` — Project directory to add to the workspace (repeatable)
- `--remove PATH` — Project directory to remove from the workspace (repeatable)
- `--no-open` — Update workspace without opening VS Code afterwards

#### `mark MARK POCKET`
Mark an existing safe pocket with additional metadata.

```bash
safe_pocket mark temporary ~/.safe_pocket/abc123
```

**Mark types:**
- `temporary` — Mark pocket as temporary

#### `locate`
Locate the safe pocket associated with a project or pocket path. Outputs JSON for editor integrations.

```bash
safe_pocket locate --path ~/dev/myproject
```

**Options:**
- `--path PATH` — Project path to locate (default: current directory)

---

### Pocket Maintenance

#### `heal`
Reconnect a project directory to an existing safe pocket. Moves the selected pocket's contents into the deterministic pocket path for the PROJECT. If that target pocket already exists, it is moved aside under `~/.safe_pocket/unhoused/` before replacement.

```bash
safe_pocket heal --project ~/dev/app --pocket abc123
safe_pocket heal --project . --pocket ~/.safe_pocket/oldhash
safe_pocket heal --alias myproject --pocket ~/.safe_pocket/xyz789
```

**Options:**
- `--project PATH` — Project directory (conflicts with `--alias`)
- `--alias ALIAS` — Alias name (conflicts with `--project`)
- `--pocket POCKET` — Pocket directory to move

#### `clean [SCOPE]`
Remove registry entries or pocket directories in bulk.

```bash
safe_pocket clean temporary              # Remove temporary pockets
safe_pocket clean --all --hard           # Delete all pockets
safe_pocket clean --older-than "7 days"  # Remove old pockets
safe_pocket clean temporary -y           # Skip confirmation
```

**Scopes:**
- `temporary` — Remove temporary pockets only

**Options:**
- `--older-than AGE` — Remove pockets older than AGE (e.g., "7 days", "2 weeks")
- `--all` — Remove all pockets
- `--hard` — Delete directories (not just registry entries)
- `-y`, `--yes` — Skip confirmation

#### `sync-registry-git`
Refresh the top-level `~/.safe_pocket` git snapshot. Copies every safe pocket into `~/.safe_pocket/snapshots` without nested `.git` directories so the registry root repository can version all pocket contents together.

```bash
safe_pocket sync-registry-git
```

#### `backup`
Configure a cron job that backs up `~/.safe_pocket` to a git remote. The backup mirror lives at `~/.safe_pocket_backup_repo` and is pushed by cron.

```bash
safe_pocket backup --repo git@github.com:you/safe-pocket-backup.git
safe_pocket backup --repo git@github.com:you/safe-pocket-backup.git --schedule "0 */6 * * *"  # every 6 hours
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
safe_pocket completions bash > ~/.local/share/bash-completion/completions/safe_pocket

# ZSH (add ~/.zsh/completions to fpath first)
safe_pocket completions zsh > ~/.zsh/completions/_safe_pocket

# FISH
safe_pocket completions fish > ~/.config/fish/completions/safe_pocket.fish

# POWERSHELL
safe_pocket completions powershell >> $PROFILE

# ELVISH
safe_pocket completions elvish >> ~/.config/elvish/rc.elv
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
safe_pocket completion-spec
```

#### `daily-feature`
Resolve (and create) today's daily feature file for the VS Code hotkey. Outputs JSON describing the file to open. When `--new` is passed, always creates the next numbered file (YYYY_MM_DD_N.md); otherwise reuses any existing feature file dated today, creating a base file if none exists.

Newly created files are seeded with feature tags whose definition in `feature_tags.yaml` sets `place_automatically: true`.

```bash
safe_pocket daily-feature --pocket ~/.safe_pocket/abc123
safe_pocket daily-feature --pocket ~/.safe_pocket/abc123 --new
safe_pocket daily-feature --pocket ~/.safe_pocket/abc123 --subpath dailies
```

**Options:**
- `--pocket PATH` — Pocket directory containing the FEATURES folder (required)
- `--new` — Always create a new numbered daily feature file
- `--subpath PATH` — Subfolder under FEATURES where new daily files are created

---

### Worktree Management

#### `worktree`
Manage git worktrees that share this safe pocket. Worktrees are additional project directories (typically git worktrees of the same repo) that share the same safe pocket — meaning the same copilot instructions, FEATURES notes, and observations apply to all of them.

```bash
# Register a worktree to share the pocket for the current directory
safe_pocket worktree add ~/dev/my-project-feature-x

# Remove a previously registered worktree
safe_pocket worktree remove ~/dev/my-project-feature-x

# List all worktrees sharing this pocket
safe_pocket worktree list
```

**Subcommands:**

##### `worktree add [PATH]`
Register a worktree directory to share this pocket. Run from inside the main project directory (or any directory that already belongs to a pocket). Safe Pocket will also suggest any git worktrees it detects in the same repo if PATH is not provided explicitly.

```bash
safe_pocket worktree add ~/dev/my-project-feature-x
safe_pocket worktree add  # auto-suggest git worktrees
```

##### `worktree remove PATH`
Unregister a worktree directory from this pocket.

```bash
safe_pocket worktree remove ~/dev/my-project-feature-x
```

##### `worktree list`
List all worktrees registered to this pocket.

```bash
safe_pocket worktree list
```

---

### Task Tracking

#### `task`
Built-in issue tracker (replaces Beads). Tasks live in a global SQLite database at `~/.corner/global_data/tasks.db`, with fallback to legacy registry roots when needed. Tasks are grouped per pocket by a prefix derived from the pocket directory name. When run from inside a registered project, the correct prefix is detected automatically.

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
Rename task prefix (when a pocket is reorganized or renamed).

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

Customize files written into every new pocket by editing templates in `~/.config/corner/templates/`, with fallback to legacy config roots when those directories already exist. Each template file must begin with:

```
#SPOCKET_TEMPLATE_DESTINATION: <relative-path>
```

Supported variables:
- `{{SPOCKET_ROOT}}` — Absolute path to the pocket directory
- `{{PROJECT_ROOT}}` — Absolute path to the first included project
- `{{SPOCKET_NAME}}` — Hash-based pocket identifier

Directory structure is controlled by `~/.config/corner/directory_structure.yaml`.

### Directory Structure

The default pocket directory structure is:

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
└── manifest.json        # Pocket metadata
```

Customize by editing `~/.config/safe_pocket/directory_structure.md`.

---

## Pocket Directory Location

Pockets are stored in `~/.safe_pocket/<hash>/` where `<hash>` is derived from the project directory path, ensuring deterministic pocket association across sessions.

View all pockets:

```bash
safe_pocket list-workspaces
```

---

## Environment Variables

- `SPOCKET_ROOT` — (internal) Path to the pocket directory
- `PROJECT_ROOT` — (internal) Path to the first included project
- `SPOCKET_NAME` — (internal) Pocket hash identifier

---

## Tips & Tricks

### Create a temporary workspace for experimentation

```bash
safe_pocket -i . --temporary --silent
```

### Merge two existing projects into one workspace

```bash
safe_pocket -i ~/dev/api -i ~/dev/frontend
```

### Switch a project to a different pocket

```bash
safe_pocket heal --project ~/dev/app --pocket ~/.safe_pocket/new-hash
```

### Upgrade all pocket templates

```bash
safe_pocket sync agents
```

### Clean up old temporary pockets

```bash
safe_pocket clean temporary --hard -y
```

### Generate shell completions (ZSH)

```bash
safe_pocket completions zsh > ~/.zsh/completions/_safe_pocket
# Then add to ~/.zshrc:
# fpath=(~/.zsh/completions $fpath)
# autoload -Uz compinit && compinit
```

---

## See Also

- `~/.safe_pocket/registry.json` — Registry of all pockets
- `~/.safe_pocket/snapshots/` — Git snapshot of all pockets
- `~/.safe_pocket/global_data/tasks.db` — Global tasks database
- `~/.config/safe_pocket/` — User configuration and templates
