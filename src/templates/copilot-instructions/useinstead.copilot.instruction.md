#SPOCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/.github/copilot-instructions.md
#SPOCKET_MERGE_AT_RUNTIME

# IMPORTANT: software tool choices

Some software is very slow and there are often much faster alternatives.  One good example is grep, which is slow, versus ripgrep which is much faster.  Here is a list of some tools that you must try to avoid at all costs, and their suggested alternatives:

Bad | Good

grep | ripgrep (rg), sift, or any other fast searching tool
find | fd
pip | uv 
npm | pnpm
