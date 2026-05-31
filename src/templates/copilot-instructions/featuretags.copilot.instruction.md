#SPOCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/.github/copilot-instructions.md
#SPOCKET_MERGE_AT_RUNTIME

# Feature tags

A user may include an arbitrary number of "feature tags" inside a feature file.  A feature tag can live either at the top of the file or above a specific feature description, and this determines its scope.  A feature tag can look like this:

#SPOCKET_MUST_RUN_INSTALL_COMMAND(
    uv tool install --force --editable {{PROJECT_ROOT}}
)
#SPOCKET_MUST_NOT_ADD_DOCUMENTATION
#SPOCKET_MUST_UPDATE_DOCUMENTATION

Or the user can create their own custom feature tags which may be placed in $HOME/.config/safe_pocket/feature_tags.yaml.  If you encounter a tag beginning with #SPOCKET that you do not recognize, you should check the feature_tags.yaml file to see if it is defined there.  Each tag applies a rule to the associated scope.  

For example, if 

```markdown
#SPOCKET_MUST_INSTALL(
    cd {{PROJECT_ROOT}}
    uv tool install --force --editable .
)
```

is at the top of a feature file, then before you consider a feature to be complete, you must run the commands inside the parentheses and ensure that they complete successfully.  If the commands fail, then the feature is not complete and you should not return until you have resolved the issue causing the failure and the commands complete successfully.

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
#SPOCKET_MUST_NOT_ADD_DOCUMENTATION
# Subfeature 1
some details
# Subfeature 2
some other details
```

Then before the feature is considered complete, you must be sure to have not updated or added any documentation related to either subfeature.

---
Custom user feature tags must be defined like this in the feature_tags.yaml file:

```yaml
SPOCKET_SAY_MEOW:
    description: "You must say meow a random number of times, but no less than 5 times."
    type: "done hook"
SPOCKET_LOG_EVERYTHING:
    description: "The feature you are working on will theoretically emit events relevant to observability.  You must identify these events and instrument the code to log these events in a structured way using our chosen logging framework."
    type: "while"
```

The "type" field determines when the rule is applied.  
- "done hook / done": the rule is applied when determining whether a feature is done.  If the rule is not satisfied, the feature is not done and you should not return until the rule is satisfied.  
- "while hook / while":  the rule is applied during the whole process of working on the feature, and you should defer to it when making any related decisions. 
- "start hook / start": the rule is applied at the start of working on the feature, and you should use it to inform your initial approach to the feature, as well as choices you intend to make relating to this feature.

---
## Importing a tag's description from a file

A custom feature tag's `description` value may be a **file path** instead of inline text.  When the `description` resolves to a file (for example it begins with `$HOME`, `~`, or `/`, or otherwise names an existing file), this means: **import the real description from that file.**  Read the file at that path and use its top-level `description:` field as the effective rule text for the tag.  That field is normally a YAML block scalar (`description: |`) spanning many lines, so import the entire block, not just the first line, and then apply it exactly as if it had been written inline in `feature_tags.yaml`.

The path may be written against either of two roots, so try the following locations in order and use the first one that exists:

1. The path exactly as written, expanding `$HOME`/`~` (e.g. `$HOME/.safe_pocket/feature_tags/conversation.feature.tag.yaml`).
2. The same trailing path resolved under the safe_pocket config directory `$HOME/.config/safe_pocket/` (e.g. `$HOME/.config/safe_pocket/feature_tags/conversation.feature.tag.yaml`).

For example:

```yaml
SPOCKET_CONVERSATION_ENABLED:
  description: "$HOME/.safe_pocket/feature_tags/conversation.feature.tag.yaml"
  type: "during hook"
```

Here the `description` is a path, so load `conversation.feature.tag.yaml` (trying both roots above) and treat the `description:` block inside that file as the complete rule for `SPOCKET_CONVERSATION_ENABLED`.  The imported file is the single source of truth for what the tag means, which keeps long, evolving instructions in their own file instead of inlining them here.
