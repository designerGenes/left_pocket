#SPOCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/.gitignore
#SPOCKET_QUIET_MERGE

.env
# Per-project AI agents are rendered into the pocket on every open; they are
# derived artifacts, not source, so keep them out of version control.
.opencode/
