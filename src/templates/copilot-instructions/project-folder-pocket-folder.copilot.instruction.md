#SPOCKET_TEMPLATE_DESTINATION: {{CORNER_ROOT}}/.github/copilot-instructions.md
#SPOCKET_MERGE_AT_RUNTIME

# Project Folder Versus Pocket Folder

This file is contained inside a subdirectory of
{{CORNER_ROOT}}

That folder is a Corner pocket folder. It is NOT the project folder. The pocket folder contains meta files which are relevant only to the actual project folder. The actual project folder is
{{PROJECT_ROOT}}

All commands made to the agent are intended to be applied to the project folder, not the pocket folder. The pocket folder is only for storing meta files that are relevant to the project folder. The agent should never make any changes to the pocket folder, only read from it.

For example, if the agent is asked to review our codebase, it should read the code files from the project folder, not the pocket folder. The pocket folder may contain instructions or other meta files that are relevant to the project folder, but the actual code files are in the project folder.

# Rules

1. You must always use full paths whenever you reference any file or directory. NEVER use relative paths.
2. If instructed to use a "cli app" or "terminal command", you should run this command in the context of the project folder, not the pocket folder. You should also always try to run the literal command you are told to use, before searching for python files or source code. For example, if I tell you "use the cli app sponge_bob to do X", you must first attempt to run the command "sponge_bob" in the terminal, and only if that fails should you search for a python file or source code that might be relevant.
