#SPOCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/.github/copilot-instructions.md
#SPOCKET_MERGE_AT_RUNTIME

# Feature Tags

A feature file may contain feature tags. A tag can apply to the whole file when it appears near the top of the file, or to a specific subfeature when it appears immediately before that subfeature heading.

Predefined tags include:

```markdown
 #SPOCKET_MUST_INSTALL
 #SPOCKET_MUST_ADD_NEW_TESTS
 #SPOCKET_MUST_UPDATE_DOCUMENTATION
 #SPOCKET_MUST_NOT_UPDATE_DOCUMENTATION
 #SPOCKET_MUST_CREATE_NEW_FILES
 #SPOCKET_MUST_BACKUP
```

Custom feature tags may be defined in `$HOME/.config/safe_pocket/feature_tags.yaml`:

```yaml
SPOCKET_MUST_RUN_INSTALL_COMMAND:
  description: "before this feature is considered complete, the agent must run the install command and verify that it was successful. The install command is 'uv tool install --force --editable .'"
  type: "done hook"
```

If you encounter an unknown tag beginning with `#SPOCKET`, check `$HOME/.config/safe_pocket/feature_tags.yaml` before deciding what it means. Supported tag types are:

- `done hook` or `done`: perform this action before considering the feature or subfeature complete.
- `start hook` or `start`: perform this action before starting the feature or subfeature.
- `rule`, `while hook`, or `while`: obey this rule while working on the feature or subfeature.

Example done hook:

```markdown
 #SPOCKET_MUST_INSTALL(
     cd {{PROJECT_ROOT}}
     uv tool install --force --editable .
 )
```

If this appears at the top of a feature file, then before you consider that feature complete, run the commands inside the parentheses and ensure that they succeed. If they fail, the feature is not complete.

if a feature file looks like


```markdown
# Subfeature 1
some details

 #SPOCKET_MUST_UPDATE_DOCUMENTATION
# Subfeature 2
some other details
```

then before subfeature 2 is considered done (and thus before the feature itself can be considered finished) you must ensure that documentation related to the subfeature is updated or generated if none is found.  However, subfeature 1 does not have this requirement, so you can consider subfeature 1 to be complete without updating documentation, but you cannot consider subfeature 2 to be complete without updating documentation.

Conversely if you have a feature file like this:

```markdown
 #SPOCKET_MUST_NOT_UPDATE_DOCUMENTATION
# Subfeature 1
some details
# Subfeature 2
some other details
```

Then before the feature is considered complete, do not update or add documentation related to either subfeature.
