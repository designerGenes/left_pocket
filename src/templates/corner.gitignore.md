#CORNER_TEMPLATE_DESTINATION: {{CORNER_ROOT}}/.gitignore
#CORNER_QUIET_MERGE

.env
# Per-project AI agents are rendered into the corner on every open; they are
# derived artifacts, not source, so keep them out of version control.
.opencode/
