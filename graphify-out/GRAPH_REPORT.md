# Graph Report - .  (2026-06-06)

## Corpus Check
- cluster-only mode — file stats not available

## Summary
- 619 nodes · 1762 edges · 17 communities (16 shown, 1 thin omitted)
- Extraction: 100% EXTRACTED · 0% INFERRED · 0% AMBIGUOUS · INFERRED: 2 edges (avg confidence: 0.8)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `6dbcccef`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- [[_COMMUNITY_Community 0|Community 0]]
- [[_COMMUNITY_Community 1|Community 1]]
- [[_COMMUNITY_Community 2|Community 2]]
- [[_COMMUNITY_Community 3|Community 3]]
- [[_COMMUNITY_Community 4|Community 4]]
- [[_COMMUNITY_Community 5|Community 5]]
- [[_COMMUNITY_Community 6|Community 6]]
- [[_COMMUNITY_Community 7|Community 7]]
- [[_COMMUNITY_Community 8|Community 8]]
- [[_COMMUNITY_Community 9|Community 9]]
- [[_COMMUNITY_Community 10|Community 10]]
- [[_COMMUNITY_Community 11|Community 11]]
- [[_COMMUNITY_Community 12|Community 12]]
- [[_COMMUNITY_Community 13|Community 13]]
- [[_COMMUNITY_Community 14|Community 14]]
- [[_COMMUNITY_Community 15|Community 15]]
- [[_COMMUNITY_Community 16|Community 16]]

## God Nodes (most connected - your core abstractions)
1. `Result` - 37 edges
2. `Workspace` - 34 edges
3. `Result` - 30 edges
4. `Result` - 26 edges
5. `Manifest` - 25 edges
6. `Result` - 25 edges
7. `Path` - 25 edges
8. `Workspace` - 23 edges
9. `Result` - 22 edges
10. `apply_templates()` - 21 edges

## Surprising Connections (you probably didn't know these)
- `add_gitleaks_writes_project_guard_files()` --calls--> `WorkspaceFolder`  [EXTRACTED]
  tests/cli_behavior.rs → src/workspace.rs
- `with_tool_is_session_sidecar_only_and_add_tool_persists()` --calls--> `WorkspaceFolder`  [EXTRACTED]
  tests/cli_behavior.rs → src/workspace.rs
- `WorkspaceFolder` --references--> `PathBuf`  [EXTRACTED]
  src/workspace.rs → tests/cli_behavior.rs
- `isTodaysFeatureFile()` --calls--> `parse_dated_filename()`  [EXTRACTED]
  vscode-extension/src/extension.ts → src/feature.rs
- `parse_dated_filename()` --calls--> `pad2()`  [EXTRACTED]
  src/feature.rs → vscode-extension/src/extension.ts

## Import Cycles
- 1-file cycle: `build.rs -> build.rs`
- 1-file cycle: `src/template.rs -> src/template.rs`
- 1-file cycle: `src/agents.rs -> src/agents.rs`
- 1-file cycle: `src/config.rs -> src/config.rs`
- 1-file cycle: `src/event.rs -> src/event.rs`
- 1-file cycle: `src/feature.rs -> src/feature.rs`
- 1-file cycle: `src/hash.rs -> src/hash.rs`
- 1-file cycle: `src/main.rs -> src/main.rs`
- 1-file cycle: `src/task.rs -> src/task.rs`
- 1-file cycle: `src/manifest.rs -> src/manifest.rs`
- 1-file cycle: `src/registry.rs -> src/registry.rs`
- 1-file cycle: `tests/cli_behavior.rs -> tests/cli_behavior.rs`

## Communities (17 total, 1 thin omitted)

### Community 0 - "Community 0"
Cohesion: 0.06
Nodes (91): apply_merge_at_runtime(), apply_templates(), apply_templates_with_mode(), display_diff(), ensure_default_assets(), expand_install_roots(), expand_runtime_variables_in_content(), expand_runtime_variables_in_file() (+83 more)

### Community 1 - "Community 1"
Cohesion: 0.12
Nodes (58): ColoredString, Connection, Flags, Row, add_log(), assign_task(), close_task(), cmd_create() (+50 more)

### Community 2 - "Community 2"
Cohesion: 0.07
Nodes (52): activate(), DailyFeatureResult, deactivate(), featureTagsYamlPath(), getSpocketDir(), handleFolderChange(), isSpocketWorkspace(), isTodaysFeatureFile() (+44 more)

### Community 3 - "Community 3"
Cohesion: 0.11
Nodes (58): CleanScope, Cli, MarkChoice, add_persistent_workspace_folder(), apply_project_tools(), bridge_graphify_dir(), build_template_context(), confirm_hard_clean() (+50 more)

### Community 4 - "Community 4"
Cohesion: 0.12
Nodes (29): F, HashSet, Map, hash_paths(), PathBuf, String, test_hash_length(), copy_dir_all() (+21 more)

### Community 5 - "Community 5"
Cohesion: 0.12
Nodes (53): Default, Item, Iterator, aliases_path(), cache_path_for(), cache_tmp_path_for(), collect_pockets_from_dir(), copy_dir_all() (+45 more)

### Community 6 - "Community 6"
Cohesion: 0.17
Nodes (30): Drop, Output, S, add_gitleaks_writes_project_guard_files(), assert_contains(), assert_failure(), assert_success(), clean_hard_removes_temporary_pocket_and_registry_entry() (+22 more)

### Community 7 - "Community 7"
Cohesion: 0.12
Nodes (35): agents_template_dir(), load_unified_agents(), opencode_agent_dir(), OpenCodePermissions, parse_inline_list(), parse_unified_agent(), Permission, pocket_agent_dir() (+27 more)

### Community 8 - "Community 8"
Cohesion: 0.16
Nodes (20): Manifest, DateTime, Option, Path, PathBuf, Result, Self, String (+12 more)

### Community 9 - "Community 9"
Cohesion: 0.07
Nodes (32): activationEvents, categories, properties, title, contributes, commands, configuration, keybindings (+24 more)

### Community 10 - "Community 10"
Cohesion: 0.30
Nodes (7): Config, HashMap, PathBuf, Result, Self, String, Vec

### Community 11 - "Community 11"
Cohesion: 0.18
Nodes (12): From, CleanScope, Cli, Commands, MarkChoice, Option, Self, String (+4 more)

### Community 12 - "Community 12"
Cohesion: 0.17
Nodes (11): compilerOptions, esModuleInterop, lib, module, outDir, rootDir, skipLibCheck, sourceMap (+3 more)

### Community 13 - "Community 13"
Cohesion: 0.43
Nodes (6): BTreeMap, collect_files(), main(), Path, PathBuf, String

### Community 14 - "Community 14"
Cohesion: 0.29
Nodes (6): command, enabled, type, mcp, graphify, $schema

### Community 15 - "Community 15"
Cohesion: 0.71
Nodes (6): append_event(), append_pocket_event(), append_registry_event(), Path, Result, Value

## Knowledge Gaps
- **76 isolated node(s):** `String`, `PathBuf`, `install.sh script`, `$schema`, `type` (+71 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Workspace` connect `Community 4` to `Community 3`?**
  _High betweenness centrality (0.089) - this node is a cross-community bridge._
- **Why does `HashSet` connect `Community 4` to `Community 1`?**
  _High betweenness centrality (0.054) - this node is a cross-community bridge._
- **Why does `Flags` connect `Community 1` to `Community 4`?**
  _High betweenness centrality (0.054) - this node is a cross-community bridge._
- **What connects `String`, `PathBuf`, `install.sh script` to the rest of the system?**
  _76 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Community 0` be split into smaller, more focused modules?**
  _Cohesion score 0.06314432989690721 - nodes in this community are weakly interconnected._
- **Should `Community 1` be split into smaller, more focused modules?**
  _Cohesion score 0.11805555555555555 - nodes in this community are weakly interconnected._
- **Should `Community 2` be split into smaller, more focused modules?**
  _Cohesion score 0.06927551560021153 - nodes in this community are weakly interconnected._