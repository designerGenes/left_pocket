use clap::{Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

// ── Top-level CLI ─────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(name = "safe_pocket", visible_alias = "spocket")]
#[command(version)]
#[command(
    about = "Safe Pocket — ad hoc VS Code workspace manager with AI copilot support",
    long_about = "\
Safe Pocket keeps \"meta\" files (copilot instructions, prompts, observations, \
feature notes) in a dedicated pocket directory (~/.safe_pocket/<hash>/) so they \
never pollute your project repo, yet VS Code opens them together with your project \
as a single multi-root workspace.

QUICK START

  # Create or open a workspace for the current directory
  safe_pocket -i .

  # Create a workspace spanning two projects
  safe_pocket -i ~/dev/frontend -i ~/dev/backend

  # Upgrade the pocket's template files to match your latest templates
  safe_pocket -u ~/dev/myproject

  # Generate and install shell completions (zsh example)
  safe_pocket completions zsh > ~/.zsh/completions/_safe_pocket

ALIAS

  `spocket` is the short alias for the `safe_pocket` binary.

POCKET DIRECTORY

  Pockets are stored in ~/.safe_pocket/<hash>/.

TEMPLATES

  Customise the files written into every new pocket by editing templates in
  ~/.config/safe_pocket/templates/.  Each template file must begin with:

    #SPOCKET_TEMPLATE_DESTINATION: <relative-path>

  Supported variables: {{SPOCKET_ROOT}}, {{PROJECT_ROOT}}, {{SPOCKET_NAME}}

  Directory structure is controlled by ~/.config/safe_pocket/directory_structure.md."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Add a directory to the workspace (repeatable)
    ///
    /// Resolves aliases registered with `safe_pocket register`. Can be specified
    /// multiple times to create a multi-root workspace:
    ///
    ///   safe_pocket -i ~/dev/api -i ~/dev/frontend
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

    /// Add a tool for this safe_pocket session only
    ///
    /// Tools are exposed as managed sidecar folders when the workspace opens, but
    /// are not installed into the project for future sessions.
    ///
    ///   spocket -i . --with gitleaks
    ///   spocket -i . --with graphify
    #[arg(long = "with", value_name = "TOOL")]
    pub with_tools: Vec<String>,

    /// Install a tool into the project and safe pocket for future sessions
    ///
    /// If the tool is already installed this is a no-op. Supported tools:
    /// gitleaks, graphify.
    ///
    ///   spocket -i . --add gitleaks
    ///   spocket -i . --add graphify
    #[arg(long = "add", value_name = "TOOL")]
    pub add_tools: Vec<String>,

    /// Clone the pocket from the workspace that contains this path
    ///
    /// Copies all meta files (copilot instructions, prompts, observations, etc.)
    /// from the source pocket into the new one, then tracks lineage in both
    /// manifests. Useful when starting a new project that should inherit the
    /// AI configuration of a related one.
    ///
    ///   safe_pocket -i ~/dev/new-project --clone-from ~/dev/existing-project
    #[arg(long = "clone-from", value_name = "PATH")]
    pub clone_from: Option<String>,

    /// Create or reuse the pocket under ~/.safe_pocket/temporary/
    ///
    /// Temporary pockets are tracked separately so test suites and other
    /// short-lived workflows can be cleaned up without touching normal pockets.
    #[arg(long = "temporary")]
    pub temporary: bool,

    /// Skip creating README files in empty directories
    ///
    /// By default safe_pocket writes helpful README.md files into new empty
    /// directories (observations/, .github/prompts/, etc.).  Pass this flag
    /// to suppress them, e.g. when cloning a pocket for a minimal setup.
    #[arg(long = "no-readme")]
    pub no_readme: bool,

    /// Upgrade an existing pocket to match current templates (does not open VS Code)
    ///
    /// Reads every template from ~/.config/safe_pocket/templates/, expands
    /// {{SPOCKET_ROOT}} / {{PROJECT_ROOT}} / {{SPOCKET_NAME}} variables, and
    /// writes the result to the pocket.  If a file already exists with different
    /// content you are shown a diff and asked to confirm before overwriting.
    ///
    /// PATH may be either:
    ///   • The pocket directory itself  (~/.safe_pocket/abc123)
    ///   • Any project directory whose pocket you want to upgrade
    ///
    ///   safe_pocket -u ~/dev/myproject
    ///   safe_pocket -u ~/.safe_pocket/abc123
    #[arg(short = 'u', long = "upgrade", value_name = "PATH")]
    pub upgrade: Option<String>,

    /// Force creation of a new workspace even if one already exists
    ///
    /// By default, if `safe_pocket -i .` detects that the current directory (or any
    /// included path) already belongs to an existing safe pocket, it opens that
    /// pocket instead of creating a duplicate.  Pass `--new` to override this
    /// behaviour and always create a fresh workspace.
    ///
    ///   safe_pocket -i . --new
    #[arg(long = "new")]
    pub force_new: bool,

    /// Enable verbose output
    ///
    /// Shows informational messages that are hidden by default, such as runtime
    /// merge notifications, template installation notices, and other
    /// non-error/non-warning details.
    ///
    ///   safe_pocket -i . --verbose
    #[arg(long = "verbose")]
    pub verbose: bool,

    /// Inject runtime content into destination files without launching VS Code
    ///
    /// Simulates how files would look at VS Code runtime. Every file that would
    /// normally gain inject-at-runtime content (wrapped in
    /// #SPOCKET_RUNTIME_CONTENT_START / #SPOCKET_RUNTIME_CONTENT_END markers)
    /// gains that content even though the runtime (VS Code) is never started.
    /// Implies --silent: VS Code is not opened.
    ///
    ///   safe_pocket -i . --simulate-runtime --temporary --silent
    #[arg(long = "simulate-runtime")]
    pub simulate_runtime: bool,

    /// Perform every step except opening VS Code at the end (debug)
    ///
    /// Useful for exercising safe_pocket setup without launching the editor. If
    /// run inside a directory already associated with a safe pocket this is
    /// effectively a no-op, since the normal action would simply have opened the
    /// related project.
    ///
    ///   safe_pocket -i . --silent
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
    /// safe_pocket command that accepts a PATH argument.
    ///
    ///   safe_pocket register api="~/dev/my-api-project"
    ///   safe_pocket -i api      # same as -i ~/dev/my-api-project
    #[command(name = "register")]
    Register {
        /// Alias definition in format: name="path"
        #[arg(value_name = "NAME=PATH")]
        alias: String,
    },

    /// Remove a previously registered directory alias
    ///
    ///   safe_pocket unregister api
    #[command(name = "unregister")]
    Unregister {
        /// Name of the alias to remove
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// List all registered directory aliases
    #[command(name = "list-aliases")]
    ListAliases,

    /// List all known safe pockets with their project paths
    #[command(name = "list-workspaces")]
    ListWorkspaces,

    /// Sync system-wide assets, or the manifest after the workspace file changes
    ///
    /// With a TARGET, synchronizes system-wide safe_pocket assets that every
    /// pocket draws from:
    ///
    ///   safe_pocket sync agents   Write the unified agent definitions from
    ///                             ~/.config/safe_pocket/templates/agents into the
    ///                             places OpenCode looks for agents.
    ///   safe_pocket sync all      Run every system-wide sync (currently agents).
    ///
    /// Without a TARGET (the legacy form used by the VS Code extension), updates
    /// the pocket manifest to reflect the current .code-workspace folders and
    /// prints JSON. Requires --pocket. You rarely need to run this manually.
    #[command(name = "sync")]
    Sync {
        /// What to sync: "agents" or "all". Omit for the manifest sync.
        #[arg(value_name = "TARGET")]
        target: Option<String>,

        /// Path to the pocket directory (required for the manifest sync)
        #[arg(long = "pocket", value_name = "PATH")]
        pocket: Option<String>,
    },

    /// Add or remove project directories from the current workspace in-place
    ///
    /// Rewrites the .code-workspace file and manifest without moving the pocket
    /// directory. Run this from inside a pocket or project directory that
    /// belongs to an existing workspace.
    ///
    ///   safe_pocket augment --add ~/dev/new-service
    ///   safe_pocket augment --remove ~/dev/old-service
    ///   safe_pocket augment --add ~/dev/new-service --no-open
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

    /// Mark an existing safe pocket with additional metadata
    #[command(name = "mark")]
    Mark {
        #[arg(value_enum, value_name = "MARK")]
        mark: MarkChoice,

        #[arg(value_name = "POCKET")]
        pocket: String,
    },

    /// Remove registry entries or pocket directories in bulk
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

    /// Reconnect a project directory to an existing safe pocket
    ///
    /// Moves the selected pocket's contents into the deterministic pocket path
    /// for PROJECT. If that target pocket already exists, it is moved aside under
    /// ~/.safe_pocket/unhoused/ before replacement.
    ///
    ///   safe_pocket heal --project ~/dev/app --pocket abc123
    ///   safe_pocket heal --project . --pocket ~/.safe_pocket/oldhash
    #[command(name = "heal")]
    Heal {
        #[arg(long = "project", value_name = "PATH", conflicts_with = "alias")]
        project: Option<String>,

        #[arg(long = "alias", value_name = "ALIAS", conflicts_with = "project")]
        alias: Option<String>,

        #[arg(long = "pocket", value_name = "POCKET")]
        pocket: Option<String>,
    },

    /// Locate the safe pocket associated with a project or pocket path
    ///
    /// Outputs JSON for editor integrations.
    #[command(name = "locate")]
    Locate {
        #[arg(long = "path", value_name = "PATH", default_value = ".")]
        path: String,
    },

    /// Refresh the top-level ~/.safe_pocket git snapshot
    ///
    /// Copies every safe pocket into ~/.safe_pocket/snapshots without nested
    /// `.git` directories so the registry root repository can version all pocket
    /// contents together.
    #[command(name = "sync-registry-git")]
    SyncRegistryGit,

    /// Configure a cron job that backs up ~/.safe_pocket to a git remote
    ///
    /// The backup mirror lives at ~/.safe_pocket_backup_repo and is pushed by cron.
    ///
    ///   safe_pocket backup --repo git@github.com:you/safe-pocket-backup.git
    #[command(name = "backup")]
    Backup {
        #[arg(long = "repo", value_name = "GIT_URL")]
        repo: String,

        #[arg(long = "schedule", value_name = "CRON", default_value = "0 * * * *")]
        schedule: String,
    },

    /// Inject runtime content into destination files (called by the VS Code extension on open)
    ///
    /// For each template marked with `#SPOCKET_MERGE_AT_RUNTIME`, injects the expanded
    /// template content into the destination file wrapped in runtime markers.
    #[command(name = "runtime-merge-start", hide = true)]
    RuntimeMergeStart {
        /// Path to the pocket directory containing the manifest
        #[arg(long = "pocket", value_name = "PATH")]
        pocket: String,
    },

    /// Strip runtime content from destination files (called by the VS Code extension on close)
    ///
    /// Removes any content between `#SPOCKET_RUNTIME_CONTENT_START` and
    /// `#SPOCKET_RUNTIME_CONTENT_END` markers from destination files.
    #[command(name = "runtime-merge-stop", hide = true)]
    RuntimeMergeStop {
        /// Path to the pocket directory containing the manifest
        #[arg(long = "pocket", value_name = "PATH")]
        pocket: String,
    },

    /// Print a shell completion script to stdout
    ///
    /// Generates tab-completion definitions for your shell.  Pipe the output to
    /// the appropriate location for your shell, then source it.
    ///
    /// BASH
    ///   safe_pocket completions bash > ~/.local/share/bash-completion/completions/safe_pocket
    ///
    /// ZSH  (add ~/.zsh/completions to fpath first)
    ///   safe_pocket completions zsh > ~/.zsh/completions/_safe_pocket
    ///
    /// FISH
    ///   safe_pocket completions fish > ~/.config/fish/completions/safe_pocket.fish
    ///
    /// POWERSHELL
    ///   safe_pocket completions powershell >> $PROFILE
    ///
    /// ELVISH
    ///   safe_pocket completions elvish >> ~/.config/elvish/rc.elv
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
        /// Path to the pocket directory containing the FEATURES folder
        #[arg(long = "pocket", value_name = "PATH")]
        pocket: String,

        /// Always create a new numbered daily feature file
        #[arg(long = "new")]
        new: bool,

        /// Subfolder under FEATURES where new daily files are created
        #[arg(long = "subpath", value_name = "PATH")]
        subpath: Option<String>,
    },

    /// Manage git worktrees that share this safe pocket
    ///
    /// Worktrees are additional project directories (typically git worktrees of
    /// the same repo) that share the same safe pocket — meaning the same copilot
    /// instructions, FEATURES notes, and observations apply to all of them.
    ///
    ///   # Register a worktree to share the pocket for the current directory
    ///   safe_pocket worktree add ~/dev/my-project-feature-x
    ///
    ///   # Remove a previously registered worktree
    ///   safe_pocket worktree remove ~/dev/my-project-feature-x
    ///
    ///   # List all worktrees sharing this pocket
    ///   safe_pocket worktree list
    #[command(name = "worktree")]
    Worktree {
        #[command(subcommand)]
        action: WorktreeAction,
    },

    /// Track project tasks in a fast, built-in issue tracker (replaces Beads)
    ///
    /// Tasks live in a global SQLite database at
    /// ~/.safe_pocket/global_data/tasks.db and are grouped per safe pocket by a
    /// prefix derived from the pocket directory name. When run from inside a
    /// registered project, the correct prefix is detected automatically.
    ///
    ///   spocket task list [--priority N] [--project PATH] [--raw]
    ///   spocket task create --named "Implement X" --description "…" --priority 1
    ///   spocket task <ID> assign --agent "Builder"
    ///   spocket task <ID> start  [--notes "…"]
    ///   spocket task <ID> log     --notes "…"
    ///   spocket task <ID> close  [--notes "…"]
    ///   spocket task <ID> discard
    ///   spocket task <ID> describe [--raw]
    ///   spocket task reprefix --from OLD --to NEW
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
    /// Register a worktree directory to share this pocket
    ///
    /// Run this from inside the main project directory (or any directory that
    /// already belongs to a pocket). Safe Pocket will also suggest any git worktrees
    /// it detects in the same repo if PATH is not provided explicitly.
    ///
    ///   safe_pocket worktree add ~/dev/my-project-feature-x
    #[command(name = "add")]
    Add {
        #[arg(value_name = "PATH")]
        path: Option<String>,
    },

    /// Unregister a worktree directory from this pocket
    ///
    ///   safe_pocket worktree remove ~/dev/my-project-feature-x
    #[command(name = "remove")]
    Remove {
        #[arg(value_name = "PATH")]
        path: String,
    },

    /// List all worktrees registered to this pocket
    #[command(name = "list")]
    List,
}
