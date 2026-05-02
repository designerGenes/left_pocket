# Safe Pocket

Safe Pocket is a CLI tool for managing ad hoc VS Code workspaces based on directory combinations, enabling persistent AI copilot customization across different project contexts.

## The Problem

When working across multiple projects or features in a monorepo, you often need:
- Custom Copilot instructions specific to your workflow
- Context-aware AI assistance without bloating the main codebase
- Ability to combine directories from different projects into one workspace
- Persistent customization that doesn't require approval from repository maintainers

Traditional approaches fail because:
- Workspace names don't work well for combinations of multiple projects
- Custom copilot instructions are hard to get merged into shared codebases
- You lose your customizations when switching between features

## The Solution

Safe Pocket creates "safe pockets" - version-controlled directories that contain:
- Custom Copilot instructions (`.github/copilot-instructions.md`)
- Feature documentation (`FEATURES/`)
- Observations and learnings (`observations/`)
- VS Code workspace configuration

Each safe pocket is identified by a hash of the directories it contains, making it deterministic and reusable.

## Installation

### From Source

```bash
git clone <repository-url>
cd safe_pocket
cargo build --release
```

The primary binary will be at `target/release/safe_pocket`.

### Add to PATH

```bash
# Copy to a location in your PATH
cp target/release/safe_pocket /usr/local/bin/

# Optional alias if you prefer the short command name
echo 'alias spocket="/path/to/safe_pocket/target/release/safe_pocket"' >> ~/.zshrc
```

## Usage

### Register Directory Aliases

Register frequently-used directories with memorable names:

```bash
safe_pocket register myproject="$HOME/dev/myproject"
safe_pocket register backend="$HOME/dev/api"
```

### List Aliases

```bash
safe_pocket list
```

### Create/Open Workspaces

Create a workspace from one or more directories:

```bash
# Using an alias
safe_pocket -i myproject

# Using multiple directories
safe_pocket -i myproject -i backend

# Using full paths
safe_pocket -i ~/dev/project1 -i ~/dev/project2

# Using shell variables (expand before passing)
devBin="$HOME/dev/bin"
safe_pocket -i $devBin -i ~/dev/personal
```

### Sidecar Directories

Add temporary directories that won't be saved to the workspace:

```bash
# Add ~/dev/tools as a sidecar (temporary)
safe_pocket -i myproject -i backend --sidecar ~/dev/tools
```

Sidecars are useful for directories you need occasionally but don't want permanently in the workspace. They're added when opening but not saved to the `.code-workspace` file.

### Clone Safe Pockets

Copy safe pocket contents (copilot instructions, features, etc.) from one workspace to another:

```bash
# Clone from workspace containing myproject to a new workspace
safe_pocket -i newproject --clone-from myproject
```

### Smart Cloning (Automatic)

When creating a new workspace, Safe Pocket automatically detects similar existing workspaces based on directory overlap. If a similar workspace is found (with at least 30% similarity), you'll be prompted to clone from it:

```bash
# Create workspace with dir1 and dir2
safe_pocket -i dir1 -i dir2 -i dir3

# Later, create a similar workspace (dir1 and dir2 only)
safe_pocket -i dir1 -i dir2

# Output:
# 🔍 Similar workspaces found!
# These workspaces share directories with your new workspace:
#
#   1. abc123def456 (66% similarity)
#      → /path/to/dir1
#      → /path/to/dir2
#      → /path/to/dir3
#
#   0. Don't clone (create fresh workspace)
#
# Select a workspace to clone from (0-1, or press Enter to skip):
```

This allows you to:
- Reuse customizations when working on related features
- Avoid manually tracking which workspaces have useful copilot instructions
- Quickly bootstrap new workspaces with proven configurations

**Similarity Calculation:** Safe Pocket uses Jaccard similarity (intersection over union) to measure how similar workspace directories are. A workspace with 2 out of 3 directories matching has 66% similarity.

### List All Workspaces

```bash
safe_pocket list-workspaces
```

### Unregister Aliases

```bash
safe_pocket unregister myproject
```

### Skip README Generation

By default, Safe Pocket creates helpful README files in empty directories explaining their purpose. To skip these:

```bash
# Create workspace without READMEs
safe_pocket -i myproject --no-readme
```

**Note:** READMEs are never created when cloning from an existing workspace (they're preserved from the source).

### Version

Check which version of Safe Pocket you have:

```bash
safe_pocket -v
safe_pocket --version
```

### Verbose Output

By default, informational messages (such as runtime merge notifications) are suppressed for a clean experience. Enable them when you want to see what's happening under the hood:

```bash
safe_pocket -i myproject --verbose
```

## How It Works

1. **Hashing**: When you specify directories with `-i`, Safe Pocket sorts and hashes their full paths to create a unique 12-character identifier.

2. **Safe Pocket Creation**: A directory is created at `~/.safe_pocket/<hash>/` containing:
   - `.github/prompts/` - Custom prompt templates
   - `.github/copilot-instructions.md` - Copilot instructions
   - `FEATURES/00.md` - Feature documentation
   - `observations/` - AI-generated insights
   - `<hash>.code-workspace` - VS Code workspace file
   - `.git/` - Git repository for version control

3. **Workspace Structure**: The workspace includes:
   - All your specified directories
   - The safe pocket directory itself

4. **Mismatch Detection**: If the workspace file contains different directories than the hash suggests, Safe Pocket warns you.

## Directory Structure

```
~/.safe_pocket/
└── abc123def456/              # Hash of included directories
    ├── .git/                  # Git repository
    ├── .github/
    │   ├── prompts/
    │   └── copilot-instructions.md
    ├── FEATURES/
    │   └── 00.md
    ├── observations/
    └── abc123def456.code-workspace

~/.safe_pocket/
├── aliases                    # Shared alias registry
├── registry_cache.json        # Safe pocket registry cache
└── observations/              # Global observations shared across pockets
```

## Configuration

Aliases are stored at `~/.safe_pocket/aliases`:

```json
{
  "aliases": {
    "myproject": "/Users/username/dev/myproject",
    "backend": "/Users/username/dev/backend"
  }
}
```

## Examples

### Monorepo Feature Development

```bash
# Work on a specific feature with custom copilot instructions
safe_pocket -i ~/monorepo/features/auth

# Add observability tools as a sidecar
safe_pocket -i ~/monorepo/features/auth --sidecar ~/dev/tools
```

### Multi-Project Workspace

```bash
# Combine iOS project with backend API
safe_pocket -i ~/projects/ios-app -i ~/projects/backend-api

# Your custom copilot instructions work across both projects
```

### Reusing Customizations

```bash
# Created great copilot instructions for feature-a
safe_pocket -i ~/monorepo/feature-a

# Clone them to feature-b
safe_pocket -i ~/monorepo/feature-b --clone-from ~/monorepo/feature-a
```

## Recent Features (v0.4.0)

- `safe_pocket` is now the primary binary name, with `spocket` kept as an alias
- Temporary pockets can live under `~/.safe_pocket/temporary/`
- `clean` and `mark temporary` commands were added for safe pocket lifecycle management
- Beads now defaults on unless `--without-beads` is passed

### Previous: v0.2.1

- ✅ **No emojis**: Cleaner, more professional CLI output
- ✅ **README files** in safe pocket directories explaining their purpose
- ✅ **`--no-readme` flag** to skip README generation

### Previous: v0.2.0

- ✅ **Smart Cloning**: Automatic detection of similar workspaces with interactive selection
- ✅ **Similarity Calculation**: Jaccard-based similarity scoring for workspace matching
- ✅ **Comprehensive Testing**: Unit tests for core functionality

## Future Features

- GitHub repo integration for safe pockets (push/pull safe pocket as a repo)
- Workspace editing without breaking hash associations (add/remove directories)
- Custom templates for safe pocket contents
- Beads integration (Ralph Wiggum loops and custom API interactions)
- Workspace cleanup utilities (remove unused workspaces, fix mismatches)
- Configurable similarity threshold for smart cloning
- Workspace tagging and search
- Export/import safe pocket configurations

## Contributing

This project is in early development. Feedback and contributions are welcome!

## License

[Specify your license here]
