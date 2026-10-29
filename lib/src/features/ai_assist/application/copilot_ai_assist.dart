part of 'ai_assist_registry.dart';

final AiAssistCapability copilotAiAssistAgentSpec = AiAssistCapabilitySpec(
  agentType: .copilot,
  promptDelivery: .argv,
  modelsCommand: null,
  parseModels: parseLineModels,
  models: const <AiAssistModel>[
    AiAssistModel(id: 'auto', label: 'Auto'),
    AiAssistModel(
      id: 'gpt-5.4',
      label: 'GPT-5.4',
      thinkingLevels: openAiThinkingLevels,
      defaultThinkingLevel: 'low',
    ),
    AiAssistModel(
      id: 'gpt-5.4-mini',
      label: 'GPT-5.4 Mini',
      thinkingLevels: openAiThinkingLevels,
      defaultThinkingLevel: 'low',
    ),
  ],
  defaultModelId: 'gpt-5.4',
  diffOnlyAccess: .toolFree,
  diffOnlyArgs: const <String>[
    '--available-tools=',
    '--excluded-tools=*',
    '--disable-builtin-mcps',
    '--no-ask-user',
    '--no-auto-update',
  ],
  maxPromptBytes: 24000,
  buildArgs:
      ({
        required prompt,
        required model,
        thinkingLevel,
        required timeoutSeconds,
      }) => <String>[
        '--prompt',
        prompt,
        '--silent',
        '--stream',
        'off',
        '--no-custom-instructions',
        '--model',
        model,
        if (thinkingLevel != null) ...<String>['--effort', thinkingLevel],
      ],
);
