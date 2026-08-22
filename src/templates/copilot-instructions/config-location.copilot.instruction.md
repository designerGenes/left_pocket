#LEFT_POCKET_TEMPLATE_DESTINATION: {{LEFT_POCKET_ROOT}}/.github/copilot-instructions.md
#LEFT_POCKET_MERGE_AT_RUNTIME

If an application will provide user-editable configuration files, the config files must live inside 

$HOME/.config/{{PROJECT_NAME}}/config.(json/yaml/toml)

If an application will create files as part of regular operation, the generated files should live inside 

$HOME/.{{PROJECT_NAME}}/
