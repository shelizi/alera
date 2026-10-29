part of 'ai_assist_registry.dart';

final AiAssistCapability cursorAiAssistAgentSpec = AiAssistCapabilitySpec(
  agentType: .cursor,
  promptDelivery: .argv,
  modelsCommand: const <String>['--list-models'],
  parseModels: parseCursorModels,
  models: const <AiAssistModel>[AiAssistModel(id: 'auto', label: 'Auto')],
  defaultModelId: 'auto',
  maxPromptBytes: 24000,
  buildArgs:
      ({
        required prompt,
        required model,
        thinkingLevel,
        required timeoutSeconds,
      }) => <String>[
        '--print',
        '--mode',
        'ask',
        '--trust',
        '--output-format',
        'text',
        '--model',
        model,
        prompt,
      ],
);
