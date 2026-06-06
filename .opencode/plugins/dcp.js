// OpenCode Dynamic Context Pruning (DCP) Plugin
// Compacts conversation history before it is sent to the LLM to save tokens and maintain focus.

export const DynamicContextPruningPlugin = async (input, options) => {
  const severity = options?.severity || "default";
  console.log(`[DCP] Dynamic Context Pruning plugin initialized (severity: ${severity})`);

  const config = {
    high: {
      keepLatestDuplicates: 1,
      maxTextLength: 1000,
      pruneFailedTurns: 0,
      compressOlderThanTurns: 3,
      pruneReasoning: true,
      pruneSnapshots: true,
    },
    default: {
      keepLatestDuplicates: 2,
      maxTextLength: 5000,
      pruneFailedTurns: 2,
      compressOlderThanTurns: 6,
      pruneReasoning: true,
      pruneSnapshots: true,
    },
    low: {
      keepLatestDuplicates: 4,
      maxTextLength: 20000,
      pruneFailedTurns: 5,
      compressOlderThanTurns: 12,
      pruneReasoning: false,
      pruneSnapshots: false,
    }
  }[severity] || {
    keepLatestDuplicates: 2,
    maxTextLength: 5000,
    pruneFailedTurns: 2,
    compressOlderThanTurns: 6,
    pruneReasoning: true,
    pruneSnapshots: true,
  };

  const compressText = (text, maxLength) => {
    if (!text || text.length <= maxLength) return text;
    const keep = Math.floor(maxLength / 2);
    const prunedCount = text.length - maxLength;
    return text.substring(0, keep) + `\n\n... [Content Pruned: ${prunedCount} characters omitted] ...\n\n` + text.substring(text.length - keep);
  };

  return {
    "experimental.chat.messages.transform": async (inputHook, outputHook) => {
      if (!outputHook.messages || outputHook.messages.length === 0) return;

      const totalMessages = outputHook.messages.length;

      // We process messages from newest (end) to oldest (beginning)
      const transformedMessages = outputHook.messages.map((msg, index) => {
        const turnsAgo = totalMessages - 1 - index;
        const role = msg.info?.role;
        const isLatestMessage = turnsAgo === 0;

        // Clone the parts array and part objects to avoid side-effects on internal agent memory
        const parts = msg.parts.map(part => {
          // Clone the part
          const newPart = { ...part };

          // 1. Pruning Reasoning Parts
          if (config.pruneReasoning && newPart.type === "reasoning" && role === "assistant" && !isLatestMessage) {
            return {
              id: newPart.id,
              sessionID: newPart.sessionID,
              messageID: newPart.messageID,
              type: "text",
              text: "[Reasoning pruned to save tokens]"
            };
          }

          // 2. Pruning Snapshots/Patches for older messages
          if (config.pruneSnapshots && (newPart.type === "snapshot" || newPart.type === "patch") && turnsAgo >= 2) {
            return {
              id: newPart.id,
              sessionID: newPart.sessionID,
              messageID: newPart.messageID,
              type: "text",
              text: `[${newPart.type === "patch" ? "File diffs" : "Workspace snapshot"} pruned]`
            };
          }

          return newPart;
        });

        return {
          ...msg,
          parts
        };
      });

      // Track duplicate tool calls
      const keptToolCalls = new Set();
      const toolCallKeyCounts = new Map();

      for (let i = totalMessages - 1; i >= 0; i--) {
        const msg = transformedMessages[i];
        for (const part of msg.parts) {
          if (part.type === "tool" && part.state) {
            const toolKey = `${part.tool}:${JSON.stringify(part.state.input || {})}`;
            let count = toolCallKeyCounts.get(toolKey) || 0;
            if (count < config.keepLatestDuplicates) {
              keptToolCalls.add(part.id);
              toolCallKeyCounts.set(toolKey, count + 1);
            }
          }
        }
      }

      // Apply final compression and duplicate pruning
      transformedMessages.forEach((msg, index) => {
        const turnsAgo = totalMessages - 1 - index;
        const limit = turnsAgo > config.compressOlderThanTurns ? Math.floor(config.maxTextLength / 4) : config.maxTextLength;

        msg.parts = msg.parts.map(part => {
          if (part.type === "text") {
            part.text = compressText(part.text, limit);
          } else if (part.type === "tool" && part.state) {
            // Clone state to mutate it safely
            part.state = { ...part.state };
            const state = part.state;
            const isKept = keptToolCalls.has(part.id);

            if (!isKept && state.status === "completed") {
              state.output = "[Output pruned: duplicate tool call output]";
              if (state.attachments) {
                state.attachments = [];
              }
            } else {
              // Apply normal pruning based on status
              if (state.status === "completed") {
                state.output = compressText(state.output, limit);
              } else if (state.status === "error") {
                if (turnsAgo > config.pruneFailedTurns) {
                  state.error = `[Error output pruned: failed tool call occurred ${turnsAgo} turns ago]`;
                } else {
                  state.error = compressText(state.error, limit);
                }
              }
            }
          }
          return part;
        });
      });

      outputHook.messages = transformedMessages;
    }
  };
};

export default DynamicContextPruningPlugin;
