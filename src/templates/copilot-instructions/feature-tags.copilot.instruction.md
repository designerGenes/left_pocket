#SPOCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/.github/copilot-instructions.md
#SPOCKET_MERGE_AT_RUNTIME

# Feature tags

A user may include an arbitrary number of "feature tags" inside a feature file.  A feature tag can live either at the top of the file or above a specific feature description, and this determines its scope.  A feature tag can look like this:

#SPOCKET_MUST_INSTALL(
    cd {{PROJECT_ROOT}}
    uv tool install --force --editable .
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
A tag definition may also set `place_automatically: true`. When a tag is marked this way, that tag (with its leading `#`) is automatically written at the very top of any new feature file that safe_pocket creates (for example, the daily feature files opened via the VS Code hotkey).  For example:

```yaml
SPOCKET_MUST_TALK_LIKE_A_CAT:
    description: "You must talk like a cat at least once while working on this feature."
    place_automatically: true
    type: "during hook"
```

would cause every newly created feature file to begin with `#SPOCKET_MUST_TALK_LIKE_A_CAT`.

If the user later removes an auto-placed tag from a feature file, it must NOT be automatically re-added; `place_automatically` only governs placement at the moment a new feature file is created, not re-insertion afterward.
