#LEFT_POCKET_TEMPLATE_DESTINATION: {{LEFT_POCKET_ROOT}}/.github/copilot-instructions.md
#LEFT_POCKET_MERGE_AT_RUNTIME

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
