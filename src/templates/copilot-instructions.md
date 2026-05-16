#SPOCKET_TEMPLATE_DESTINATION: {{SPOCKET_ROOT}}/.github/copilot-instructions.md
#SPOCKET_MERGE_AT_RUNTIME

# Project folder versus safe pocket folder

This file is contained inside a subdirectory of
{{SPOCKET_ROOT}}

That folder is a "safe pocket" folder. It is NOT the "project folder". The safe pocket folder contains "meta files" which are relevant only to the actual project folder. The actual project folder is
{{PROJECT_ROOT}}

All commands made to the agent are intended to be applied to the project folder, not the safe pocket folder. The safe pocket folder is only for storing meta files that are relevant to the project folder. The agent should never make any changes to the safe pocket folder, only read from it.

For example, if the agent is asked to review our codebase, it should read the code files from the project folder, not the safe pocket folder. The safe pocket folder may contain instructions or other meta files that are relevant to the project folder, but the actual code files are in the project folder.

# Rules

1. You must always use full paths whenever you reference any file or directory. NEVER use relative paths.
2. If instructed to use a "cli app" or "terminal command", you should run this command in the context of the project folder, not the safe pocket folder. You should also always try to run the literal command you are told to use, before searching for python files or source code. For example, if I tell you "use the cli app sponge_bob to do X", you must first attempt to run the command "sponge_bob" in the terminal, and only if that fails should you search for a python file or source code that might be relevant.


# the "<--- Make it so" directive

Whenever you are given a link or path to a markdown file, followed by "<--- make it so", you must interpret this to mean:

```markdown
consume the contents of the given file, and
- fix all of the bugs it mentions
- incorporate/create all of the features it describes
- add unit tests for any new functionality that is added in this process
- do NOT return until all of the above has been completed, tested, and committed locally in Git

```

the number of "-"s after the "<" is not relevant and can be any reasonable number of dashes or no dashes.  The important part is that the file is given, followed by a left-facing "arrow" the phrase "make it so". 