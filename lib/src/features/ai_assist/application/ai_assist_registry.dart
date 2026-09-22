import 'dart:convert';

import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_snapshot.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';

part 'grok_ai_assist.dart';
part 'fx_ai_assist.dart';
part 'opencode_ai_assist.dart';
part 'claude_ai_assist.dart';
part 'ai_assist_model_labels.dart';
part 'ai_assist_output_capabilities.dart';

enum AiAssistDiffOnlyAccess { unsupported, toolFree, codexRestrictedFilesystem }

class const AiThinkingLevel({
  required final String id,
  required final String label,
});

class const AiAssistModel({
  required final String id,
  required final String label,
  final List<AiThinkingLevel> thinkingLevels = const <AiThinkingLevel>[],
  final String? defaultThinkingLevel,
}) {
  bool get supportsThinking => thinkingLevels.isNotEmpty;

  AiAssistDiscoveredModel toDiscovered() {
    return AiAssistDiscoveredModel(
      id: id,
      label: label,
      thinkingLevels: <AiAssistDiscoveredThinkingLevel>[
        for (final level in thinkingLevels)
          AiAssistDiscoveredThinkingLevel(id: level.id, label: level.label),
      ],
      defaultThinkingLevel: defaultThinkingLevel,
    );
  }
}

AiAssistModel modelFromDiscovered(AiAssistDiscoveredModel model) {
  return AiAssistModel(
    id: model.id,
    label: model.label,
    thinkingLevels: <AiThinkingLevel>[
      for (final level in model.thinkingLevels)
        AiThinkingLevel(id: level.id, label: level.label),
    ],
    defaultThinkingLevel: model.defaultThinkingLevel,
  );
}

abstract interface class AiAssistCapability {
  AgentType get agentType;
  AiAssistAgent get agent;
  String get id;
  String get label;
  String get binary;
  AiPromptDelivery get promptDelivery;
  List<String>? get modelsCommand;
  List<AiAssistModel> Function(String stdout) get parseModels;
  List<AiAssistModel> get models;
  String? get defaultModelId;
  bool get modelCanInherit;
  AiNativeStructuredOutput get nativeStructuredOutput;
  bool get supportsRepositoryRead;
  bool get readOnlyGuarantee;
  AiAssistDiffOnlyAccess get diffOnlyAccess;
  List<String> get diffOnlyArgs;
  int get maxPromptBytes;
  List<String> Function({
    required String prompt,
    required String model,
    String? thinkingLevel,
    required int timeoutSeconds,
  })
  get buildArgs;
}

class const AiAssistAgentSpec({
  required final AgentType agentType,
  required final AiPromptDelivery promptDelivery,
  required final List<String>? modelsCommand,
  required final List<AiAssistModel> Function(String stdout) parseModels,
  required final List<AiAssistModel> models,
  required final String? defaultModelId,
  required final List<String> Function({
    required String prompt,
    required String model,
    String? thinkingLevel,
    required int timeoutSeconds,
  })
  buildArgs,
  final bool modelCanInherit = false,
  final AiNativeStructuredOutput nativeStructuredOutput =
      AiNativeStructuredOutput.none,
  final bool supportsRepositoryRead = false,
  final bool readOnlyGuarantee = false,
  final AiAssistDiffOnlyAccess diffOnlyAccess =
      AiAssistDiffOnlyAccess.unsupported,
  final List<String> diffOnlyArgs = const <String>[],
  final int maxPromptBytes = 1024 * 1024,
}) implements AiAssistCapability {
  AgentDescriptorSnapshot get descriptor => agentDescriptorFor(agentType);

  @override
  String get id => descriptor.id;

  @override
  String get label => descriptor.displayName;

  @override
  String get binary => descriptor.defaultCommand;

  AiAssistAgent get agent => aiAssistAgentForType(agentType)!;
}

const List<AiThinkingLevel> basicThinkingLevels = <AiThinkingLevel>[
  AiThinkingLevel(id: 'low', label: 'Low'),
  AiThinkingLevel(id: 'medium', label: 'Medium'),
  AiThinkingLevel(id: 'high', label: 'High'),
];

const List<AiThinkingLevel> openAiThinkingLevels = <AiThinkingLevel>[
  AiThinkingLevel(id: 'low', label: 'Low'),
  AiThinkingLevel(id: 'medium', label: 'Medium'),
  AiThinkingLevel(id: 'high', label: 'High'),
  AiThinkingLevel(id: 'xhigh', label: 'Extra High'),
];

const List<AiThinkingLevel> claudeThinkingLevels = <AiThinkingLevel>[
  AiThinkingLevel(id: 'low', label: 'Low'),
  AiThinkingLevel(id: 'medium', label: 'Medium'),
  AiThinkingLevel(id: 'high', label: 'High'),
  AiThinkingLevel(id: 'xhigh', label: 'Extra High'),
  AiThinkingLevel(id: 'max', label: 'Max'),
];

const List<AiThinkingLevel> onOffThinkingLevels = <AiThinkingLevel>[
  AiThinkingLevel(id: 'on', label: 'On'),
  AiThinkingLevel(id: 'off', label: 'Off'),
];

final Map<AgentType, AiAssistAgentSpec>
aiAssistCapabilities = <AgentType, AiAssistAgentSpec>{
  AgentType.claude: claudeAiAssistAgentSpec,
  AgentType.codex: AiAssistAgentSpec(
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
  ),
  AgentType.copilot: AiAssistAgentSpec(
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
  ),
  AgentType.cursor: AiAssistAgentSpec(
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
  ),
  AgentType.agy: AiAssistAgentSpec(
    agentType: .agy,
    promptDelivery: .stdin,
    modelsCommand: const <String>['models'],
    parseModels: parseAgyModels,
    models: const <AiAssistModel>[],
    defaultModelId: null,
    modelCanInherit: true,
    nativeStructuredOutput: .jsonSchemaArgument,
    buildArgs:
        ({
          required model,
          thinkingLevel,
          required prompt,
          required timeoutSeconds,
        }) => <String>[
          '--sandbox',
          '--print-timeout',
          '${timeoutSeconds}s',
          if (model.trim().isNotEmpty) ...<String>['--model', model],
        ],
  ),
  AgentType.opencode: openCodeAiAssistSpec(agentType: .opencode),
  AgentType.opencode2: openCodeAiAssistSpec(agentType: .opencode2),
  AgentType.pi: AiAssistAgentSpec(
    agentType: .pi,
    promptDelivery: .stdin,
    modelsCommand: const <String>['--list-models'],
    parseModels: parsePiModels,
    models: const <AiAssistModel>[
      AiAssistModel(
        id: 'github-copilot/gpt-5.4-mini',
        label: 'GitHub Copilot GPT-5.4 Mini',
        thinkingLevels: openAiThinkingLevels,
        defaultThinkingLevel: 'low',
      ),
    ],
    defaultModelId: 'github-copilot/gpt-5.4-mini',
    diffOnlyAccess: .toolFree,
    buildArgs:
        ({
          required model,
          thinkingLevel,
          required prompt,
          required timeoutSeconds,
        }) => <String>[
          '--print',
          '--no-session',
          '--no-tools',
          '--no-extensions',
          '--no-skills',
          '--no-context-files',
          '--mode',
          'text',
          '--model',
          model,
          if (thinkingLevel != null) ...<String>['--thinking', thinkingLevel],
        ],
  ),
  AgentType.amp: AiAssistAgentSpec(
    agentType: .amp,
    promptDelivery: .stdin,
    modelsCommand: null,
    parseModels: parseLineModels,
    models: const <AiAssistModel>[
      AiAssistModel(id: 'smart', label: 'Smart'),
      AiAssistModel(id: 'rush', label: 'Rush'),
      AiAssistModel(
        id: 'large',
        label: 'Large',
        thinkingLevels: basicThinkingLevels,
        defaultThinkingLevel: 'low',
      ),
      AiAssistModel(
        id: 'deep',
        label: 'Deep',
        thinkingLevels: basicThinkingLevels,
        defaultThinkingLevel: 'low',
      ),
    ],
    defaultModelId: 'smart',
    buildArgs:
        ({
          required model,
          thinkingLevel,
          required prompt,
          required timeoutSeconds,
        }) => <String>[
          '--execute',
          '--no-notifications',
          '--no-ide',
          '--no-jetbrains',
          '--mode',
          model,
          if (thinkingLevel != null) ...<String>['--effort', thinkingLevel],
        ],
  ),
  AgentType.grok: grokAiAssistAgentSpec,
  AgentType.devin: AiAssistAgentSpec(
    agentType: .devin,
    promptDelivery: .argv,
    modelsCommand: const <String>['models', 'list'],
    parseModels: parseDevinModels,
    models: const <AiAssistModel>[],
    defaultModelId: null,
    modelCanInherit: true,
    supportsRepositoryRead: true,
    buildArgs:
        ({
          required model,
          thinkingLevel,
          required prompt,
          required timeoutSeconds,
        }) => <String>[
          '--print',
          '--permission-mode',
          'auto',
          '--respect-workspace-trust',
          'false',
          if (model.trim().isNotEmpty) ...<String>['--model', model],
          prompt,
        ],
  ),
  AgentType.fx: fxAiAssistAgentSpec,
};

AiAssistCapability? aiAssistCapabilityFor(AgentType? agentType) =>
    agentType == null ? null : aiAssistCapabilities[agentType];

List<AiAssistAgent> get selectableAiAssistAgents => <AiAssistAgent>[
  ...aiAssistCapabilities.values.map((capability) => capability.agent),
  AiAssistAgent.custom,
];

AiAssistAgent? aiAssistAgentForType(AgentType type) {
  for (final agent in AiAssistAgent.values) {
    if (agent.agentType == type) {
      return agent;
    }
  }
  return null;
}

List<AiAssistModel> parseDevinModels(String stdout) {
  final models = <AiAssistModel>[];
  final seen = <String>{};
  final modelLine = RegExp(
    r'^\s{2}([A-Za-z0-9][A-Za-z0-9._-]*)\s{2,}(.+?)\s{2,}\[',
  );
  for (final line in const LineSplitter().convert(stdout)) {
    final match = modelLine.firstMatch(line);
    if (match == null) continue;
    final id = match.group(1)!;
    if (!seen.add(id)) continue;
    models.add(AiAssistModel(id: id, label: match.group(2)!.trim()));
  }
  return models;
}

List<AiAssistModel> discoveredModelsForAgent(
  AiAssistSettings settings,
  AiAssistAgent agent,
) {
  return settings
      .discoveredModelsFor(agent)
      .map(modelFromDiscovered)
      .toList(growable: false);
}

List<AiAssistModel> modelsForAgent(
  AiAssistAgent agent,
  AiAssistSettings settings,
) {
  final spec = aiAssistCapabilityFor(agent.agentType);
  if (spec == null) {
    return const <AiAssistModel>[];
  }
  return _uniqueModels(<AiAssistModel>[
    if (spec.modelCanInherit)
      const AiAssistModel(id: '', label: 'Agent Default'),
    ...discoveredModelsForAgent(settings, agent),
    ...spec.models,
  ]);
}

String defaultModelIdForAgent(AiAssistAgent agent, AiAssistSettings settings) {
  final spec = aiAssistCapabilityFor(agent.agentType);
  if (spec == null) {
    return 'custom';
  }
  if (!spec.modelCanInherit) {
    final discoveredDefault = settings.discoveredDefaultModelFor(agent);
    if (discoveredDefault != null) {
      return discoveredDefault;
    }
  }
  return spec.defaultModelId ?? '';
}

AiAssistModel modelForAgent(
  AiAssistAgent agent,
  String? modelId, {
  List<AiAssistModel> extraModels = const <AiAssistModel>[],
}) {
  final spec = aiAssistCapabilityFor(agent.agentType);
  if (spec == null) {
    return const AiAssistModel(id: 'custom', label: 'Custom');
  }
  final id = modelId?.trim().isNotEmpty == true
      ? modelId!.trim()
      : spec.defaultModelId ?? '';
  if (id.isEmpty && spec.modelCanInherit) {
    return const AiAssistModel(id: '', label: 'Agent Default');
  }
  return <AiAssistModel>[...extraModels, ...spec.models].firstWhere(
    (model) => model.id == id,
    orElse: () => AiAssistModel(id: id, label: labelFromModelId(id)),
  );
}

List<AiAssistModel> parseLineModels(String stdout) {
  return _uniqueModels(
    stdout
        .split(RegExp(r'\r?\n'))
        .map((line) => line.trim())
        .where((line) => line.isNotEmpty)
        .map((line) => AiAssistModel(id: line, label: labelFromModelId(line)))
        .toList(growable: false),
  );
}

List<AiAssistModel> parseAgyModels(String stdout) {
  return _uniqueModels(
    stdout
        .split(RegExp(r'\r?\n'))
        .map((line) => line.trim())
        .map((line) => line.replaceFirst(RegExp(r'^[*-]\s+'), ''))
        .where(
          (line) =>
              line.isNotEmpty &&
              !line.toLowerCase().startsWith('available model'),
        )
        .map(
          (line) => AiAssistModel(id: line, label: labelFromAgyModelId(line)),
        )
        .toList(growable: false),
  );
}

List<AiAssistModel> parseCursorModels(String stdout) {
  return _uniqueModels(
    stdout
        .split(RegExp(r'\r?\n'))
        .map((line) => RegExp(r'^([^\s]+)\s+-\s+(.+)$').firstMatch(line.trim()))
        .whereType<RegExpMatch>()
        .map(
          (match) => AiAssistModel(
            id: match.group(1)!,
            label: match
                .group(2)!
                .replaceFirst(
                  RegExp(r'\s+\((?:default|current)\)$', caseSensitive: false),
                  '',
                ),
          ),
        )
        .toList(growable: false),
  );
}

List<AiAssistModel> parseCodexModels(String stdout) {
  try {
    final decoded = jsonDecode(stdout);
    if (decoded is! Map) {
      return const <AiAssistModel>[];
    }
    final models = decoded['models'];
    if (models is! List) {
      return const <AiAssistModel>[];
    }
    return _uniqueModels(
      models
          .whereType<Map>()
          .map((item) {
            final slug = item['slug'];
            final label = item['display_name'];
            return slug is String && label is String
                ? AiAssistModel(
                    id: slug,
                    label: label,
                    thinkingLevels: openAiThinkingLevels,
                    defaultThinkingLevel:
                        item['default_reasoning_level'] is String
                        ? item['default_reasoning_level'] as String
                        : 'low',
                  )
                : null;
          })
          .whereType<AiAssistModel>()
          .toList(growable: false),
    );
  } catch (_) {
    return const <AiAssistModel>[];
  }
}

List<AiAssistModel> parsePiModels(String stdout) {
  return _uniqueModels(
    stdout
        .split(RegExp(r'\r?\n'))
        .map((line) => line.trim().split(RegExp(r'\s+')))
        .where((parts) => parts.length >= 2 && parts.first != 'provider')
        .map((parts) {
          final id = '${parts[0]}/${parts[1]}';
          return AiAssistModel(id: id, label: labelFromModelId(id));
        })
        .toList(growable: false),
  );
}

List<AiAssistModel> _uniqueModels(List<AiAssistModel> models) {
  final seen = <String>{};
  return models.where((model) => seen.add(model.id)).toList(growable: false);
}
