```
+-------------------+
| left_pocket       |
+-------------------+
|                   |
|                   |
|                   |
|                   |
 \                 /
  `---------------'
```

# What is left_pocket about

left_pocket is about creating ephemeral borders around arbitrary files/folders, in a way that AI understands.  That's really it, but it has many more implications than may at first seem.

Whenever you work in VS Code, you have access to Copilot in the sidebar, or various terminal AI agents in the Terminal.  However you arrive at this, the context is limited to the immediate working directory.  In the real world, your projects often involve multiple folders, and the folders involved can change over time.  You can make short metaprojects involving one feature of a monorepo and another feature of the same monorepo.  Your project is defined as not just a folder: it's now "all the folders involved in this momentary project".

So if your AI scope is just a project folder, you've probably injected lots of custom instructions and agents and prompts into that folder (perhaps in its .github subdirectory).  When your next project involves that folder, and another folder, you would normally lose those custom details.  left_pocket lets you share your "template" customizations in any new or existing project, without overwriting them when you make your own customizations.

These customizations are very important for a project, because they allow you to make AI much more deterministic and computer-like.  The baseline Copilot that you begin with is a language model, and does not instinctively know where project files are located, or should be located, or the baseline rules of interacting with a project (such as not creating new documentation without being asked).  Normally you have to put these instructions manually in every new folder-based project.  But with left_pocket, a folder isn't a project.  A project is just a border of files and folders, and so everything in that border is a project and gets your templates and custom controls for AI automatically applied.


# left_pocket ships with these features
- flexible arbitrary border definition
    - store/recall files related to multiple repos or features, without needing to get it merged/PR'd into the remote repo
    - metaprojects:  you can define a project as a border around multiple folders, and left_pocket will treat it as a single project for AI purposes
    - scratch pad directory for creating files about a project which you will need later, but don't want to commit to the shared repo.
    - AI automatically gets the context of both the project directory and the pocket directory, seamlessly working in one while orchestrated by the other.
- templating engine
    - "pocket destination" tags to place templates with interpolated parameters into any specific place or file in the resulting pocket
    - copilot-instructions and AGENTS.md runtime insertion.  This allows you to customize instructions for any project, but also apply your baseline instructions to any project.
    - feature-based development workflow by default 
    - template tags


# What is a project

- a project is an arbitrary border around files and folders.  

a monorepo is an example of a project.  Usually within a monorepo are many subprojects (features).  Often, people only ever see the inside of their particular sub-project of the monorepo.  But if they need to do some work that involves multiple features (sub-projects) of the monorepo, they have to combine these projects in their IDE some way.  If they return this work frequently, this arbitrary context of "feature1" + "feature2", this is itself a project.  This happens more than you'd think.

In IDE's like XCode, creating a new "project" automatically assembles a large number of files for you immediately.  When interacting with AI in VS Code, you also need some files assembled precisely for you, in order to 

- use AI in the right context.
    Our pocket lives outside of the project folder itself, so we need some way of deterministically pointing the AI at the project folder, so it knows where to work.
- use AI in a deterministic way
    Our pocket should introduce some basic rules for all new projects, which we (the user) believe should apply to just about every new project, no matter the size or contents.  left_pocket ships with default rules but these can be overridden to any degree, at the global or project level.

So in order for a "project" of any size to automatically get these AI baelines, we need to define some files and put them in certain locations.  If we do it right, we can put them outside of your project folder, inside a generated and carefully managed "pocket" folder, stored usually at $HOME/.left_pocket/(some hash id).  This pocket folder is a border around your project, and it contains the files that define the project for AI purposes.

You don't want to lose your pocket folder, but if you do, you can use `left_pocket heal` in the cli from inside of your project folder.  All pockets are their own git repo, in case you want to back especially important ones up.