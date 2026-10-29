part of 'ai_assist_registry.dart';

final AiAssistCapability fxAiAssistAgentSpec = AiAssistCapabilitySpec(
  agentType: .fx,
  promptDelivery: .stdin,
  modelsCommand: const <String>['models', '--json'],
  parseModels: parseFxModels,
  models: const <AiAssistModel>[],
  defaultModelId: null,
  modelCanInherit: true,
  environmentOverridesForModel: (modelId) => <String, String>{
    'FX_PERMISSION_MODE': 'ask',
    'FX_AUTO_UPGRADE': '0',
    'FX_HERDR': '0',
    if (modelId.trim().isNotEmpty) 'FX_MODEL': modelId.trim(),
  },
  buildArgs: ({
    required model,
    thinkingLevel,
    required prompt,
    required timeoutSeconds,
  }) => const <String>['ask', '--no-save'],
);

List<AiAssistModel> parseFxModels(String stdout) {
  try {
    final decoded = jsonDecode(stdout);
    if (decoded is! Map || decoded['models'] is! List) {
      return const <AiAssistModel>[];
    }
    return _uniqueModels(
      (decoded['models'] as List)
          .whereType<Map>()
          .map((model) => model['id'])
          .whereType<String>()
          .map((id) => id.trim())
          .where((id) => id.isNotEmpty)
          .map((id) => AiAssistModel(id: id, label: labelFromModelId(id)))
          .toList(growable: false),
    );
  } catch (_) {
    return const <AiAssistModel>[];
  }
}
