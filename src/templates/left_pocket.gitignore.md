#LEFT_POCKET_TEMPLATE_DESTINATION: {{LEFT_POCKET_ROOT}}/.gitignore
#LEFT_POCKET_QUIET_MERGE

.env
# Per-project AI agents are rendered into the left_pocket on every open; they are
# derived artifacts, not source, so keep them out of version control.
.opencode/
