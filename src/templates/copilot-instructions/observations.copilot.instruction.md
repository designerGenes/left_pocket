#SPOCKET_TEMPLATE_DESTINATION: {{CORNER_ROOT}}/.github/copilot-instructions.md
#SPOCKET_MERGE_AT_RUNTIME

# Global Observations

Some applications will be frequently interacted with. Rather than having every project that needs Copilot to interact with those apps learn about the apps each time, we can have a global set of observations that are shared across all pockets. Whenever you intend to interact with an app, first check inside the global observations folder for a directory named like {{GLOBAL_OBSERVATIONS_PATH}}/name_of_app, to see if there are any relevant observations about the app from previous sessions. If there are, you can use those observations to inform your interactions with the app. If there aren't, you can create new observations based on your interactions with the app and save them to the global observations folder for future use.

# Observations Logging

As you work, you will inevitably discover significant insights about the project, codebase, patterns, bugs, conventions, and other noteworthy findings. You are required to actively log these as observation files in the pocket folder.

## What qualifies as an Observation

Log an observation whenever you discover any of the following:
- Architectural patterns or design decisions in the codebase
- Recurring bugs, anti-patterns, or footguns
- Non-obvious conventions or project-specific idioms
- Important constraints (e.g., dependency quirks, environment limitations)
- Useful techniques or shortcuts specific to this project
- Surprising or counter-intuitive behavior you encounter
- Decisions made during a session that future sessions should know about

When in doubt, log it. Observations are cheap to create and valuable to retain.

## Where to write Observations

Always write observation files to:
```
{{CORNER_ROOT}}/observations/
```

This is the pocket folder. Writing here is explicitly permitted for observation logging.

## Naming Convention

Name each file using the following format:
```
YYYY-MM-DD--<slug>.md
```

Where `<slug>` is a short, lowercase, hyphen-separated summary of the observation's subject derived from its content. The slug should be specific enough to be meaningful at a glance.

Examples:
- `2025-06-10--uv-env-not-activated-by-default.md`
- `2025-06-10--project-uses-ruff-not-black.md`
- `2025-06-11--api-auth-token-stored-in-dotenv.md`

Do NOT use generic slugs like `observation-1` or `misc-finding`.

## File Format

Each observation file should be a short Markdown file with the following structure:

```markdown
# <Title of Observation>

**Date:** YYYY-MM-DD  
**Context:** <Brief description of what you were doing when you made this observation>

## Finding

<Clear, concise description of what you observed.>

## Why It Matters

<Why this is worth knowing for future sessions or contributors.>

## Notes

<Any additional details, caveats, or related links. Omit if not needed.>
```

Keep observations focused. One observation per file. Split large findings into multiple files if needed.
