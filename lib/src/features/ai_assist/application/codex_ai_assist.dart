part of 'ai_assist_registry.dart';

final AiAssistCapability codexAiAssistAgentSpec = AiAssistCapabilitySpec(
  agentType: .codex,
  promptDelivery: .stdin,
  modelsCommand: const <String>['debug', 'models'],
  parseModels: parseCodexModels,
  models: const <AiAssistModel>[
    AiAssistModel(
      id: 'gpt-5.5',
      label: 'GPT-5.5',
      thinkingLevels: openAiThinkingLevels,
      defaultThinkingLevel: 'low',
    ),
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
  defaultModelId: 'gpt-5.5',
  nativeStructuredOutput: .codexSchemaFile,
  supportsRepositoryRead: true,
  readOnlyGuarantee: true,
  diffOnlyAccess: .codexRestrictedFilesystem,
  maxPromptBytes: 1024 * 1024,
  buildArgs:
      ({
        required model,
        thinkingLevel,
        required prompt,
        required timeoutSeconds,
      }) => <String>[
        'exec',
        '--ephemeral',
        '--skip-git-repo-check',
        '-s',
        'read-only',
        '--model',
        model,
        if (thinkingLevel != null) '-c' else '',
        if (thinkingLevel != null) 'model_reasoning_effort=$thinkingLevel',
      ].where((arg) => arg.isNotEmpty).toList(growable: false),
);
