use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

// ── Top-level CLI ─────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "left_pocket", visible_aliases = ["locket", "corner", "safe_pocket", "spocket"])]
#[command(version)]
#[command(
    about = "left_pocket (locket) — ad hoc VS Code workspace manager with AI copilot support",
    long_about = "\
left_pocket keeps \"meta\" files (copilot instructions, prompts, observations, \
feature notes) in a dedicated left_pocket directory (~/.left_pocket/<hash>/) so they \
never pollute your project repo, yet VS Code opens them together with your project \
as a single multi-root workspace.

QUICK START

  # Create or open a workspace for the current directory
  left_pocket -i .

  # Create a workspace spanning two projects
  left_pocket -i ~/dev/frontend -i ~/dev/backend

  # Upgrade the left_pocket's template files to match your latest templates
  left_pocket -u ~/dev/myproject

  # Generate and install shell completions (zsh example)
  left_pocket completions zsh > ~/.zsh/completions/_left_pocket

LEGACY ALIASES

  `safe_pocket` and `spocket` continue to work as compatibility aliases.

LOCKET DIRECTORY

  New left_pockets are stored in ~/.left_pocket/<hash>/. If that tree does not exist yet,
  left_pocket falls back to ~/.safe_pocket/ (and then ~/.spocket/) so existing data,
  tasks, and registry state keep working.

TEMPLATES

  Customise the files written into every new left_pocket by editing templates in
  ~/.config/left_pocket/templates/. If that path is absent, left_pocket falls back to
  ~/.config/safe_pocket/templates/ and ~/.config/spocket/templates/.
  Each template file must begin with:

    #LEFT_POCKET_TEMPLATE_DESTINATION: <relative-path>

  (The legacy #LEFT_POCKET_TEMPLATE_DESTINATION form is still recognised when
  reading templates for backwards compatibility.)

  Supported variables: {{LEFT_POCKET_ROOT}}/{{SPOCKET_ROOT}}, {{PROJECT_ROOT}},
  {{LEFT_POCKET_NAME}}/{{SPOCKET_NAME}}, {{LEFT_POCKET_CONFIG_ROOT}}/{{SPOCKET_CONFIG_ROOT}},
  {{LEFT_POCKET_REGISTRY_ROOT}}/{{SPOCKET_REGISTRY_ROOT}}

  Directory structure is controlled by ~/.config/left_pocket/directory_structure.yaml."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Add a directory to the workspace (repeatable)
    ///
    /// Resolves aliases registered with `left_pocket register`. Can be specified
    /// multiple times to create a multi-root workspace:
    ///
    ///   left_pocket -i ~/dev/api -i ~/dev/frontend
    #[arg(short = 'i', long = "include", value_name = "PATH")]
    pub include: Vec<String>,

    /// Add a temporary sidecar directory (not saved to workspace file)
    ///
    /// Sidecar directories are injected into the workspace for this session only
    /// and are removed next time the workspace is opened normally. Useful for
    /// pulling in a dependency or reference repo without permanently changing the
    /// workspace definition.
    #[arg(short = 's', long = "sidecar", value_name = "PATH")]
    pub sidecar: Vec<String>,

    /// Add a tool for this left_pocket session only
    ///
    /// Tools are exposed as managed sidecar folders when the workspace opens, but
    /// are not installed into the project for future sessions.
    ///
    ///   left_pocket -i . --with gitleaks
    ///   left_pocket -i . --with graphify
    ///   left_pocket -i . --with memgraph
    #[arg(long = "with", value_name = "TOOL")]
    pub with_tools: Vec<String>,

    /// Install a tool into the project and left_pocket for future sessions
    ///
    /// If the tool is already installed this is a no-op. Supported tools:
    /// gitleaks, graphify, memgraph.
    ///
    ///   left_pocket -i . --add gitleaks
    ///   left_pocket -i . --add graphify
    ///   left_pocket -i . --add memgraph
    #[arg(long = "add", value_name = "TOOL")]
    pub add_tools: Vec<String>,

    /// Clone the left_pocket from the workspace that contains this path
    ///
    /// Copies all meta files (copilot instructions, prompts, observations, etc.)
    /// from the source left_pocket into the new one, then tracks lineage in both
    /// manifests. Useful when starting a new project that should inherit the
    /// AI configuration of a related one.
    ///
    ///   left_pocket -i ~/dev/new-project --clone-from ~/dev/existing-project
    #[arg(long = "clone-from", value_name = "PATH")]
    pub clone_from: Option<String>,

    /// Create or reuse the left_pocket under ~/.left_pocket/temporary/
    ///
    /// Temporary left_pockets are tracked separately so test suites and other
    /// short-lived workflows can be cleaned up without touching normal left_pockets.
    #[arg(long = "temporary")]
    pub temporary: bool,

    /// Skip creating README files in empty directories
    ///
    /// By default left_pocket writes helpful README.md files into new empty
    /// directories (observations/, .github/prompts/, etc.).  Pass this flag
    /// to suppress them, e.g. when cloning a left_pocket for a minimal setup.
    #[arg(long = "no-readme")]
    pub no_readme: bool,

    /// Upgrade an existing left_pocket to match current templates (does not open VS Code)
    ///
    /// Reads every template from the preferred config templates directory,
    /// expands
    /// {{LEFT_POCKET_ROOT}}/{{SPOCKET_ROOT}}, {{PROJECT_ROOT}}, and
    /// {{LEFT_POCKET_NAME}}/{{SPOCKET_NAME}} variables, and
    /// writes the result to the left_pocket.  If a file already exists with different
    /// content you are shown a diff and asked to confirm before overwriting.
    ///
    /// PATH may be either:
    ///   • The left_pocket directory itself  (~/.left_pocket/abc123)
    ///   • Any project directory whose left_pocket you want to upgrade
    ///
    ///   left_pocket -u ~/dev/myproject
    ///   left_pocket -u ~/.left_pocket/abc123
    #[arg(short = 'u', long = "upgrade", value_name = "PATH")]
    pub upgrade: Option<String>,

    /// Force creation of a new workspace even if one already exists
    ///
    /// By default, if `left_pocket -i .` detects that the current directory (or any
    /// included path) already belongs to an existing left_pocket, it opens that
    /// left_pocket instead of creating a duplicate.  Pass `--new` to override this
    /// behaviour and always create a fresh workspace.
    ///
    ///   left_pocket -i . --new
    #[arg(long = "new")]
    pub force_new: bool,

    /// Enable verbose output
    ///
    /// Shows informational messages that are hidden by default, such as runtime
    /// merge notifications, template installation notices, and other
    /// non-error/non-warning details.
    ///
    ///   left_pocket -i . --verbose
    #[arg(long = "verbose")]
    pub verbose: bool,

    /// Inject runtime content into destination files without launching VS Code
    ///
    /// Simulates how files would look at VS Code runtime. Every file that would
    /// normally gain inject-at-runtime content (wrapped in
    /// #LEFT_POCKET_RUNTIME_CONTENT_START / #LEFT_POCKET_RUNTIME_CONTENT_END markers)
    /// gains that content even though the runtime (VS Code) is never started.
    /// Implies --silent: VS Code is not opened.
    ///
    ///   left_pocket -i . --simulate-runtime --temporary --silent
    #[arg(long = "simulate-runtime")]
    pub simulate_runtime: bool,

    /// Perform every step except opening VS Code at the end (debug)
    ///
    /// Useful for exercising left_pocket setup without launching the editor. If
    /// run inside a directory already associated with a left_pocket this is
    /// effectively a no-op, since the normal action would simply have opened the
    /// related project.
    ///
    ///   left_pocket -i . --silent
    #[arg(long = "silent")]
    pub silent: bool,

    #[arg(short = 'v', hide = true)]
    pub short_version: bool,
}

// ── Subcommands ───────────────────────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Register a short alias for a directory path
    ///
    /// Aliases let you refer to long directory paths by a short name in any
    /// left_pocket command that accepts a PATH argument.
    ///
    ///   left_pocket register api="~/dev/my-api-project"
    ///   left_pocket -i api      # same as -i ~/dev/my-api-project
    #[command(name = "register")]
    Register {
        /// Alias definition in format: name="path"
        #[arg(value_name = "NAME=PATH")]
        alias: String,
    },

    /// Remove a previously registered directory alias
    ///
    ///   left_pocket unregister api
    #[command(name = "unregister")]
    Unregister {
        /// Name of the alias to remove
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// List all registered directory aliases
    #[command(name = "list-aliases")]
    ListAliases,

    /// List all known left_pockets with their project paths
    #[command(name = "list-workspaces")]
    ListWorkspaces,

    /// Sync system-wide assets, or the manifest after the workspace file changes
    ///
    /// With a TARGET, synchronizes system-wide left_pocket assets that every
    /// left_pocket draws from:
    ///
    ///   left_pocket sync agents        Write the unified agent definitions from
    ///                             ~/.config/left_pocket/templates/agents into the
    ///                             places OpenCode looks for agents.
    ///   left_pocket sync all           Run every system-wide sync (currently agents).
    ///
    /// Without a TARGET (the legacy form used by the VS Code extension), updates
    /// the left_pocket manifest to reflect the current .code-workspace folders and
    /// prints JSON. Requires --left_pocket. You rarely need to run this manually.
    #[command(name = "sync")]
    Sync {
        /// What to sync: "agents" or "all". Omit for the manifest sync.
        #[arg(value_name = "TARGET")]
        target: Option<String>,

        /// Path to the left_pocket directory (required for the manifest sync)
        #[arg(long = "left_pocket", alias = "corner", alias = "pocket", value_name = "PATH")]
        left_pocket: Option<String>,
    },

    /// Add or remove project directories from the current workspace in-place
    ///
    /// Rewrites the .code-workspace file and manifest without moving the left_pocket
    /// directory. Run this from inside a left_pocket or project directory that
    /// belongs to an existing workspace.
    ///
    ///   left_pocket augment --add ~/dev/new-service
    ///   left_pocket augment --remove ~/dev/old-service
    ///   left_pocket augment --add ~/dev/new-service --no-open
    #[command(name = "augment")]
    Augment {
        /// Project directory to add to the workspace
        #[arg(long = "add", value_name = "PATH")]
        add: Vec<String>,

        /// Project directory to remove from the workspace
        #[arg(long = "remove", value_name = "PATH")]
        remove: Vec<String>,

        /// Update the workspace without opening VS Code afterwards
        #[arg(long = "no-open")]
        no_open: bool,
    },

    /// Mark an existing left_pocket with additional metadata
    #[command(name = "mark")]
    Mark {
        #[arg(value_enum, value_name = "MARK")]
        mark: MarkChoice,

        #[arg(value_name = "LEFT_POCKET")]
        left_pocket: String,
    },

    /// Remove registry entries or left_pocket directories in bulk
    #[command(name = "clean")]
    Clean {
        #[arg(value_enum, value_name = "SCOPE")]
        scope: Option<CleanScope>,

        #[arg(long = "older-than", value_name = "AGE", conflicts_with_all = ["scope", "all"])]
        older_than: Option<String>,

        #[arg(long = "all", conflicts_with_all = ["scope", "older_than"])]
        all: bool,

        #[arg(long = "hard")]
        hard: bool,

        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Reconnect a project directory to an existing left_pocket
    ///
    /// Moves the selected left_pocket's contents into the deterministic left_pocket path
    /// for PROJECT. If that target left_pocket already exists, it is moved aside under
    /// ~/.left_pocket/unhoused/ before replacement, with fallback to legacy roots.
    ///
    ///   left_pocket heal --project ~/dev/app --left_pocket abc123
    ///   left_pocket heal --project . --left_pocket ~/.left_pocket/oldhash
    #[command(name = "heal")]
    Heal {
        #[arg(long = "project", value_name = "PATH", conflicts_with = "alias")]
        project: Option<String>,

        #[arg(long = "alias", value_name = "ALIAS", conflicts_with = "project")]
        alias: Option<String>,

        #[arg(long = "left_pocket", alias = "corner", alias = "pocket", value_name = "LEFT_POCKET")]
        left_pocket: Option<String>,
    },

    /// Locate the left_pocket associated with a project or left_pocket path
    ///
    /// Outputs JSON for editor integrations.
    #[command(name = "locate")]
    Locate {
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path: String,

        /// Scan manifests directly without loading/migrating config or writing
        /// registry caches. Intended for audits and post-install tests.
        #[arg(long = "read-only")]
        read_only: bool,
    },

    /// Refresh the top-level left_pocket registry git snapshot
    ///
    /// Copies every left_pocket into ~/.left_pocket/snapshots without nested
    /// `.git` directories so the registry root repository can version all left_pocket
    /// contents together.
    #[command(name = "sync-registry-git")]
    SyncRegistryGit,

    /// Rebuild the registry cache in every known registry root
    ///
    /// Reads each left_pocket's on-disk manifest directly and rewrites
    /// `~/.left_pocket/registry_cache.json` (and any legacy roots' caches) so
    /// stale entries are pruned and split-brain duplicates collapse to a
    /// single canonical entry per left_pocket hash. Use this after manually
    /// editing a manifest, moving left_pocket directories outside left_pocket, or when
    /// `left_pocket locate` / `left_pocket -i` resolve to the wrong left_pocket.
    ///
    ///   left_pocket sync-registry
    #[command(name = "sync-registry")]
    SyncRegistry,

    /// Configure a cron job that backs up ~/.left_pocket to a git remote
    ///
    /// The backup mirror lives at ~/.left_pocket_backup_repo and is pushed by cron.
    ///
    ///   left_pocket backup --repo git@github.com:you/left_pocket-backup.git
    #[command(name = "backup")]
    Backup {
        #[arg(long = "repo", value_name = "GIT_URL")]
        repo: String,

        #[arg(long = "schedule", value_name = "CRON", default_value = "0 * * * *")]
        schedule: String,
    },

    /// Inject runtime content into destination files (called by the VS Code extension on open)
    ///
    /// For each template marked with `#LEFT_POCKET_MERGE_AT_RUNTIME`, injects the expanded
    /// template content into the destination file wrapped in runtime markers.
    #[command(name = "runtime-merge-start", hide = true)]
    RuntimeMergeStart {
        /// Path to the left_pocket directory containing the manifest
        #[arg(long = "left_pocket", alias = "corner", alias = "pocket", value_name = "PATH")]
        left_pocket: String,
    },

    /// Strip runtime content from destination files (called by the VS Code extension on close)
    ///
    /// Removes any content between `#LEFT_POCKET_RUNTIME_CONTENT_START` and
    /// `#LEFT_POCKET_RUNTIME_CONTENT_END` markers from destination files.
    #[command(name = "runtime-merge-stop", hide = true)]
    RuntimeMergeStop {
        /// Path to the left_pocket directory containing the manifest
        #[arg(long = "left_pocket", alias = "corner", alias = "pocket", value_name = "PATH")]
        left_pocket: String,
    },

    /// Seed left_pocket's canonical config and registry roots with default assets
    #[command(name = "install-default-assets", hide = true)]
    InstallDefaultAssets {
        /// Overwrite existing templates and config assets instead of skipping
        /// files that already exist. Clears the `templates/` directory first,
        /// which also removes legacy-named files (e.g. `safe_pocket.env.md`).
        #[arg(long = "replace")]
        replace: bool,
    },

    /// Rewrite legacy Spocket/Safe_pocket references to left_pocket across installed roots
    ///
    /// Scans the known config roots (~/.config/left_pocket, ~/.config/safe_pocket,
    /// ~/.config/spocket) and registry roots (~/.left_pocket, ~/.safe_pocket,
    /// ~/.spocket) for files that still contain legacy `#SPOCKET_*` directive
    /// or runtime markers, and rewrites them in place to their `#LEFT_POCKET_*`
    /// equivalents.
    ///
    /// This is a one-way text migration, NOT a template sync. It never copies
    /// content from a project back into the config templates directory. User-
    /// facing feature-tag names (e.g. `SPOCKET_MUST_INSTALL`) are left intact;
    /// only structural directive/marker tokens are rewritten.
    ///
    ///   left_pocket upgrade-installation
    ///   left_pocket upgrade-installation --dry-run
    ///   left_pocket upgrade-installation --yes
    #[command(name = "upgrade-installation")]
    UpgradeInstallation {
        /// Preview the rewrites without modifying any files
        #[arg(long = "dry-run")]
        dry_run: bool,

        /// Apply rewrites without prompting for confirmation
        #[arg(short = 'y', long = "yes")]
        yes: bool,

        /// Additional roots to scan (repeatable)
        #[arg(long = "root", value_name = "PATH")]
        roots: Vec<String>,

        /// Back up and remove literal {{SPOCKET_CONFIG_ROOT}} /
        /// {{LEFT_POCKET_CONFIG_ROOT}} artifact directories, but only when they
        /// contain exactly one regular `feature_tags.yaml` file. Lists every
        /// target and asks for explicit confirmation unless --yes is supplied.
        #[arg(long = "clean-literal-root-artifacts")]
        clean_literal_root_artifacts: bool,
    },

    /// Run post-installation operational tests against this installed binary
    ///
    /// Without `-i`, creates an isolated temporary HOME/project, exercises the
    /// major left_pocket workflows, prints a detailed checklist, then removes only
    /// that harness-owned fixture. With `-i`, first backs up the existing left_pocket
    /// and performs non-destructive idempotency checks against it; the separate
    /// operational fixture is retained for inspection.
    ///
    ///   left_pocket tests --all
    ///   left_pocket tests --all --verbose
    ///   left_pocket tests --all -i .
    #[command(name = "tests")]
    Tests {
        /// Run the complete suite, including augment, alias, heal and audits
        #[arg(long = "all")]
        all: bool,

        /// Existing project to audit non-destructively before isolated tests
        #[arg(short = 'i', long = "include", value_name = "PATH")]
        include: Option<String>,

        /// Print stdout/stderr for every command executed by the harness
        #[arg(long = "verbose")]
        verbose: bool,

        /// Keep the isolated temporary fixture after testing
        #[arg(long = "keep")]
        keep: bool,
    },

    /// Print a shell completion script to stdout
    ///
    /// Generates tab-completion definitions for your shell.  Pipe the output to
    /// the appropriate location for your shell, then source it.
    ///
    /// BASH
    ///   left_pocket completions bash > ~/.local/share/bash-completion/completions/left_pocket
    ///
    /// ZSH  (add ~/.zsh/completions to fpath first)
    ///   left_pocket completions zsh > ~/.zsh/completions/_left_pocket
    ///
    /// FISH
    ///   left_pocket completions fish > ~/.config/fish/completions/left_pocket.fish
    ///
    /// POWERSHELL
    ///   left_pocket completions powershell >> $PROFILE
    ///
    /// ELVISH
    ///   left_pocket completions elvish >> ~/.config/elvish/rc.elv
    #[command(name = "completions")]
    Completions {
        /// Shell to generate completions for
        #[arg(value_name = "SHELL")]
        shell: ShellChoice,
    },

    /// Print a concise machine-readable completion model
    ///
    /// This JSON is intended for editor and shell integrations that want richer
    /// nested command data than a single shell script can comfortably expose.
    #[command(name = "completion-spec")]
    CompletionSpec,

    /// Resolve (and create) today's daily feature file for the VS Code hotkey
    ///
    /// Outputs JSON describing the file to open. When `--new` is passed, always
    /// creates the next numbered file (YYYY_MM_DD_N.md); otherwise reuses any
    /// existing feature file dated today, creating a base file if none exists.
    ///
    /// Newly created files are seeded with feature tags whose definition in
    /// feature_tags.yaml sets `place_automatically: true`.
    #[command(name = "daily-feature")]
    DailyFeature {
        /// Path to the left_pocket directory containing the FEATURES folder
        #[arg(long = "left_pocket", alias = "corner", alias = "pocket", value_name = "PATH")]
        left_pocket: String,

        /// Always create a new numbered daily feature file
        #[arg(long = "new")]
        new: bool,

        /// Subfolder under FEATURES where new daily files are created
        #[arg(long = "subpath", value_name = "PATH")]
        subpath: Option<String>,
    },

    /// Manage git worktrees that share this left_pocket
    ///
    /// Worktrees are additional project directories (typically git worktrees of
    /// the same repo) that share the same left_pocket — meaning the same copilot
    /// instructions, FEATURES notes, and observations apply to all of them.
    ///
    ///   # Register a worktree to share the left_pocket for the current directory
    ///   left_pocket worktree add ~/dev/my-project-feature-x
    ///
    ///   # Remove a previously registered worktree
    ///   left_pocket worktree remove ~/dev/my-project-feature-x
    ///
    ///   # List all worktrees sharing this left_pocket
    ///   left_pocket worktree list
    #[command(name = "worktree")]
    Worktree {
        #[command(subcommand)]
        action: WorktreeAction,
    },

    /// Track project tasks in a fast, built-in issue tracker (replaces Beads)
    ///
    /// Tasks live in a global SQLite database at
    /// ~/.left_pocket/global_data/tasks.db and are grouped per left_pocket by a
    /// prefix derived from the left_pocket directory name. When run from inside a
    /// registered project, the correct prefix is detected automatically.
    ///
    ///   left_pocket task list [--priority N] [--project PATH] [--raw]
    ///   left_pocket task create --named "Implement X" --description "…" --priority 1
    ///   left_pocket task <ID> assign --agent "Builder"
    ///   left_pocket task <ID> start  [--notes "…"]
    ///   left_pocket task <ID> log     --notes "…"
    ///   left_pocket task <ID> close  [--notes "…"]
    ///   left_pocket task <ID> discard
    ///   left_pocket task <ID> describe [--raw]
    ///   left_pocket task reprefix --from OLD --to NEW
    #[command(name = "task")]
    Task {
        /// Task subcommand and its arguments (parsed by the task module)
        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            value_name = "ARGS"
        )]
        args: Vec<String>,
    },
}

/// Supported shells for tab-completion generation.
#[derive(Debug, Clone, ValueEnum)]
pub enum ShellChoice {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum MarkChoice {
    Temporary,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum CleanScope {
    Temporary,
}

impl From<ShellChoice> for Shell {
    fn from(s: ShellChoice) -> Self {
        match s {
            ShellChoice::Bash => Shell::Bash,
            ShellChoice::Zsh => Shell::Zsh,
            ShellChoice::Fish => Shell::Fish,
            ShellChoice::PowerShell => Shell::PowerShell,
            ShellChoice::Elvish => Shell::Elvish,
        }
    }
}

// ── Worktree subcommand actions ───────────────────────────────────────────────

#[derive(Subcommand, Debug)]
pub enum WorktreeAction {
    /// Register a worktree directory to share this left_pocket
    ///
    /// Run this from inside the main project directory (or any directory that
    /// already belongs to a left_pocket). left_pocket will also suggest any git worktrees
    /// it detects in the same repo if PATH is not provided explicitly.
    ///
    ///   left_pocket worktree add ~/dev/my-project-feature-x
    #[command(name = "add")]
    Add {
        #[arg(value_name = "PATH")]
        path: Option<String>,
    },

    /// Unregister a worktree directory from this left_pocket
    ///
    ///   left_pocket worktree remove ~/dev/my-project-feature-x
    #[command(name = "remove")]
    Remove {
        #[arg(value_name = "PATH")]
        path: String,
    },

    /// List all worktrees registered to this left_pocket
    #[command(name = "list")]
    List,
}
