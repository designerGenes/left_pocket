#POCKET_TEMPLATE_DESTINATION: {{POCKET_ROOT}}/.github/copilot-instructions.md
#POCKET_MERGE_AT_RUNTIME

# No Heredocs 

You must never use heredocs.  This is because frequently your input gets cut off at strange random points midway, and the code does not get executed correctly, potentially leaving behind artifacts or making dangerous half-complete changes.  Instead of ever using heredocs, you must instead always create a temporary script file in a /tmp/ location, and then execute the full script file.  The script must be made in such a way that it cleans itself up if it gets interrupted in any manner.
