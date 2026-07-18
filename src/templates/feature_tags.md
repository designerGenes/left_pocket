#SPOCKET_INSTALL_DESTINATION: {{CORNER_CONFIG_ROOT}}/feature_tags.yaml

SPOCKET_MUST_RUN_INSTALL_COMMAND:
  description: "before this feature is considered complete, the agent must run this specific series of install commands line by line and verify that the full process was successful."
  type: "done hook"
SPOCKET_MUST_INSTALL:
  description: "before this feature is considered complete, the agent must run the the most logical install command and verify that it was successful."
  type: "done hook"
SPOCKET_MUST_BUMP_VERSION:
  description: "before this feature is considered complete, the agent must have bumped the version number in a logical way according to the changes made in the feature.  For example, if the feature includes breaking changes, then the agent should have bumped the major version number.  If the feature includes new functionality but no breaking changes, then the agent should have bumped the minor version number.  If the feature includes only bug fixes, then the agent should have bumped the patch version number.  The 'project version' must be determined by the agent, so it may be a toml file or it may be a python file with a __version__ variable, or it may be some other method of storing the version number.  The agent must be able to determine the current version number, and then decide how to bump it based on the changes made in the feature.  This must be done before significant commits are made as part of other hook actions."
  type: "done hook"
SPOCKET_MUST_ADD_NEW_TESTS:
  description: "before this feature is considered complete, the agent must have added new tests related to the feature, and those tests must have returned successful results when run."
  type: "done hook"
SPOCKET_MUST_UPDATE_DOCUMENTATION:
  description: "while working on this feature, the agent must look for opportunities to update or create useful documentation related to the feature.  This could include updating existing documentation to reflect changes made in the feature, or creating new documentation if none is found related to the feature."
  type: "during hook"
SPOCKET_MUST_NOT_UPDATE_DOCUMENTATION:
  description: "while working on this feature, the agent must not update or create any documentation related to the feature.  If any documentation is updated or created related to the feature, then the feature is not complete."
  type: "during hook"
SPOCKET_MUST_CREATE_NEW_FILES:
  description: "before this feature is considered complete, the agent must have created at least one new file related to the feature.  This is to encourage the agent to create new files when it makes sense to do so, rather than just modifying existing files."
  notes:
    - "This is to prevent the agent from returning just a question or a vague instruction without actually making any progress on the feature.  By requiring the creation of new files, we are encouraging the agent to make tangible progress on the feature."
  type: "done hook"
SPOCKET_MUST_BACKUP_BEFORE:
  andPush: true/false
  description: "before making any changes to the project, the agent must create a backup of the current state of the project.  This is to ensure that if anything goes wrong during the development of the feature, we can easily revert back to the previous state of the project.  If andPush is true, then the agent must also commit the backup to the pocket's local repository and push it to the larger $HOME/.corner folder repository."
  type: "start hook"
SPOCKET_MUST_BACKUP_AFTER:
  andPush: true/false
  description: "after making changes to the project, the agent must commit to the pocket's local repository and the larger $HOME/.corner folder repository.  If andPush is true, then the agent must also commit the backup to the pocket's local repository and push it to the larger $HOME/.corner folder repository."
  type: "done hook"
SPOCKET_CONVERSATION_ENABLED:
  description: "{{CORNER_CONFIG_ROOT}}/feature_tags/conversation.feature.tag.yaml"
  type: "during hook"
