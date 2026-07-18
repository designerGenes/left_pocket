#SPOCKET_TEMPLATE_DESTINATION: {{CORNER_ROOT}}/.github/copilot-instructions.md
#SPOCKET_MERGE_AT_RUNTIME


- You must always use full paths whenever you reference any file or directory. NEVER use relative paths.
- If instructed to use a "cli app" or "terminal command", you should run this command in the context of the project folder, not the corner folder. You should also always try to run the literal command you are told to use, before searching for python files or source code. For example, if I tell you "use the cli app sponge_bob to do X", you must first attempt to run the command "sponge_bob" in the terminal, and only if that fails should you search for a python file or source code that might be relevant.
