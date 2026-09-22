import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:dart_mappable/dart_mappable.dart';

part 'ai_assist_settings.mapper.dart';

@MappableEnum()
enum AiAssistOperation(this.key) {
  commitMessage('commitMessage'),
  pullRequestDetails('pullRequestDetails'),
  branchName('branchName'),
  readingDiff('readingDiff'),
  workspaceIdentity('workspaceIdentity'),
  agentTitle('agentTitle'),
  speechMessage('speechMessage');

  final String key;

  String get label => switch (this) {
    AiAssistOperation.commitMessage => 'Commit Messages',
    AiAssistOperation.pullRequestDetails => 'Pull Request Details',
    AiAssistOperation.branchName => 'Branch Names',
    AiAssistOperation.readingDiff => 'Reading Diffs',
    AiAssistOperation.workspaceIdentity => 'Workspace Identity',
    AiAssistOperation.agentTitle => 'Agent Titles',
    AiAssistOperation.speechMessage => 'Speech Messages',
  };
}

@MappableEnum()
enum AiAssistAgent(this.agentType) {
  codex(AgentType.codex),
  claude(AgentType.claude),
  copilot(AgentType.copilot),
  cursor(AgentType.cursor),
  agy(AgentType.agy),
  opencode(AgentType.opencode),
  opencode2(AgentType.opencode2),
  pi(AgentType.pi),
  amp(AgentType.amp),
  grok(AgentType.grok),
  devin(AgentType.devin),
  fx(AgentType.fx),
  custom(null);

  final AgentType? agentType;

  String get key => agentType?.key ?? 'custom';

  String get label => agentType == null
      ? 'Custom Command'
      : agentDescriptorFor(agentType!).displayName;

  static AiAssistAgent? fromAgentType(AgentType agentType) {
    for (final agent in AiAssistAgent.values) {
      if (agent.agentType == agentType) {
        return agent;
      }
    }
    return null;
  }

  static List<AiAssistAgent> optionsForTypes(
    Iterable<AgentType> agentTypes, {
    bool includeCustom = true,
  }) => <AiAssistAgent>[
    ...agentTypes.map(fromAgentType).whereType<AiAssistAgent>(),
    if (includeCustom) AiAssistAgent.custom,
  ];
}

@MappableClass()
class const AiAssistDiscoveredThinkingLevel({
  required this.id,
  required this.label,
}) with AiAssistDiscoveredThinkingLevelMappable {
  final String id;
  final String label;
}

@MappableClass()
class const AiAssistDiscoveredModel({
  required this.id,
  required this.label,
  this.thinkingLevels = const <AiAssistDiscoveredThinkingLevel>[],
  this.defaultThinkingLevel,
}) with AiAssistDiscoveredModelMappable {
  final String id;
  final String label;
  final List<AiAssistDiscoveredThinkingLevel> thinkingLevels;
  final String? defaultThinkingLevel;
}

@MappableClass()
class const AiAssistPromptSettings({this.agent, this.model})
    with AiAssistPromptSettingsMappable {
  final AiAssistAgent? agent;
  final String? model;

  AgentType? get agentType => agent?.agentType;

  bool get inheritsAgent => agent == null;

  bool get inheritsModel => model == null || model!.trim().isEmpty;
}

@MappableClass()
class const AiAssistSettings({
  this.enabled = true,
  this.autoGenerateAgentTitles = true,
  this.agent = AiAssistAgent.codex,
  this.selectedModelByAgent = const <String, String>{},
  this.selectedThinkingByModel = const <String, String>{},
  this.selectedThinkingByOperation =
      const <AiAssistOperation, Map<String, String>>{},
  this.discoveredModelsByAgent =
      const <String, List<AiAssistDiscoveredModel>>{},
  this.discoveredDefaultModelByAgent = const <String, String>{},
  this.customCommand = '',
  this.instructionsByOperation = const <AiAssistOperation, String>{},
  this.promptSettingsByOperation =
      const <AiAssistOperation, AiAssistPromptSettings>{},
  this.timeoutSeconds = 120,
}) with AiAssistSettingsMappable {
  final bool enabled;
  final bool autoGenerateAgentTitles;
  final AiAssistAgent agent;
  final Map<String, String> selectedModelByAgent;
  final Map<String, String> selectedThinkingByModel;
  final Map<AiAssistOperation, Map<String, String>> selectedThinkingByOperation;
  final Map<String, List<AiAssistDiscoveredModel>> discoveredModelsByAgent;
  final Map<String, String> discoveredDefaultModelByAgent;
  final String customCommand;
  final Map<AiAssistOperation, String> instructionsByOperation;
  final Map<AiAssistOperation, AiAssistPromptSettings>
  promptSettingsByOperation;
  final int timeoutSeconds;

  AgentType? get agentType => agent.agentType;

  String? modelForType(AgentType agentType) {
    final value = selectedModelByAgent[agentType.name]?.trim();
    return value == null || value.isEmpty ? null : value;
  }

  String? thinkingForModel(String? model) {
    if (model == null || model.trim().isEmpty) {
      return null;
    }
    final value = selectedThinkingByModel[model]?.trim();
    return value == null || value.isEmpty ? null : value;
  }

  String? thinkingForOperation(AiAssistOperation operation, String? model) {
    if (model == null || model.trim().isEmpty) {
      return null;
    }
    final operationValue = selectedThinkingByOperation[operation]?[model]
        ?.trim();
    if (operationValue != null && operationValue.isNotEmpty) {
      return operationValue;
    }
    return thinkingForModel(model);
  }

  String instructionsFor(AiAssistOperation operation) {
    return instructionsByOperation[operation]?.trim() ?? '';
  }

  AiAssistPromptSettings promptSettingsFor(AiAssistOperation operation) {
    return promptSettingsByOperation[operation] ??
        const AiAssistPromptSettings();
  }

  AiAssistAgent agentFor(AiAssistOperation operation) {
    return promptSettingsFor(operation).agent ?? agent;
  }

  AgentType? agentTypeFor(AiAssistOperation operation) {
    return agentFor(operation).agentType;
  }

  String? modelForOperation(AiAssistOperation operation) {
    final promptSettings = promptSettingsFor(operation);
    final override = promptSettings.model?.trim();
    if (override != null && override.isNotEmpty) {
      return override;
    }
    final agentType = agentTypeFor(operation);
    return agentType == null ? null : modelForType(agentType);
  }

  List<AiAssistDiscoveredModel> discoveredModelsForType(AgentType agentType) {
    return discoveredModelsByAgent[agentType.name] ??
        const <AiAssistDiscoveredModel>[];
  }

  String? discoveredDefaultModelForType(AgentType agentType) {
    final value = discoveredDefaultModelByAgent[agentType.name]?.trim();
    return value == null || value.isEmpty ? null : value;
  }

  static const AiAssistSettings defaults = AiAssistSettings();

  factory fromJson(Map<String, Object?> json) =>
      AiAssistSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}
