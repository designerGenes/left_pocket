#SPOCKET_INSTALL_DESTINATION: {{CORNER_CONFIG_ROOT}}/feature_tags/conversation.feature.tag.yaml

description: |
  sometimes copilot will need a response from the user or need the user to conduct some form of input which copilot cannot mimic (such as pressing buttons in an iOS simulator)
  In this case, copilot usually stops the conversation and a new token is needed to continue after the user has given their response.  We should add the Conversation file tag, like this

  #SPOCKET_CONVERSATION_ENABLED(
      "{{CORNER_ROOT}}/path/to/general_conversation_file.json": "General conversation",
      "{{CORNER_ROOT}}/path/to/more_specific_conversation_file.json": "Some specific conversation file"
  )
  then the agent should interpret this to mean that if it needs user input, it should print its question into the file at that location (choosing contextually based on the potentially multiple files and their roles provided in the #SPOCKET_CONVERSATION_ENABLED directive), and then it should begin a timer for about 1 minute, checking every 3 seconds for the contents of that file to have been supplemented by a user response.  Whenever the agent writes to the file, it should have a specifc format, such as 

  # agent section.  do not change this part.
  question: "What is the airspeed of a laden swallow?"

  # place your answer here and save
  answer:

  --- or ---

  # agent section.  do not change this part.
  question: "Select one option from below."

  # uncomment your selection here and save
  answer:
  # A
  # B 
  # C
  # D
  # All of the above


  --- or  ---

  # agent section.  do not change this part.
  question: "Perform this action and save any value below when you have finished."

  # Put any value at all here and save when you have completed the action.


  If the loop detects the user has added to the file and saved, the agent should cut the loop short early and analyze the response, before getting back to work.
  If no value is given after the #SPOCKET_CONVERSATION_ENABLED directive, then the conversation file should just be named CONVERSATION.md and be placed at the top level FEATURES directory of the project's corner.  If there are multiple files given, the agent should use its judgement to determine which file is most appropriate for the current question it has, based on the descriptions given for each file in the #SPOCKET_CONVERSATION_ENABLED directive.  If there is a tie in terms of which file is most appropriate, it can choose any of the tied files to write its question into and wait for a response in that file.
