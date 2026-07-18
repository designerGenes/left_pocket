# What is corner about

Corner is about creating ephemeral borders around arbritrary files/folders, in a way that AI understands.  That's really it, but it has many more implications than may at first seem apparent.  

Whenever you work in VS Code, you have access to Copilot in the sidebar, or various terminal AI agents in the Terminal.  However you arrive at this, the context is limited to the immediate working directory.  In the real world, your projects often involve multiple folders, and the folders involved can change over time.  You can make short metaprojects involving one feature of a monorepo and another feature of the same monorepo.  Your project is defined as not just a folder: it's now "all the folders involved in this momentary project".

So if your AI scope is just a project folder, you've probably injected lots of custom instructions and agents and prompts into that folder (perhaps in its .github subdirectory).  When your next project involves that folder, and another folder, you would normally lose those custom details.  Corner lets you share your "template" customizations in any new or existing project, without overwriting them when you make your own customizations.


# What is a project