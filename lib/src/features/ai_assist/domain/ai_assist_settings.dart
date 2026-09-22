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

const String aiAssistCustomAgentId = 'custom';

String aiAssistAgentIdForType(AgentType type) => type.key;

bool isCustomAiAssistAgentId(String? id) => id == aiAssistCustomAgentId;

AgentType? aiAssistAgentTypeForId(String? id) {
  if (id == null || id == aiAssistCustomAgentId) return null;
  for (final type in AgentType.values) {
    if (type.key == id) return type;
  }
  throw StateError('Unknown AI Assist agent id: $id');
}

class const AiAssistAgentIdHook() extends MappingHook {
  @override
  Object? beforeDecode(Object? value) {
    if (value is String) {
      aiAssistAgentTypeForId(value);
    }
    return value;
  }

  @override
  Object? beforeEncode(Object? value) {
    if (value is String) {
      aiAssistAgentTypeForId(value);
    }
    return value;
  }
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
  @MappableField(hook: AiAssistAgentIdHook())
  final String? agent;
  final String? model;

  AgentType? get agentType => aiAssistAgentTypeForId(agent);

  bool get usesCustomAgent => isCustomAiAssistAgentId(agent);

  bool get inheritsAgent => agent == null;

  bool get inheritsModel => model == null || model!.trim().isEmpty;
}

@MappableClass()
class const AiAssistSettings({
  this.enabled = true,
  this.autoGenerateAgentTitles = true,
  this.agent = 'codex',
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
  @MappableField(hook: AiAssistAgentIdHook())
  final String agent;
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

  AgentType? get agentType => aiAssistAgentTypeForId(agent);

  bool get usesCustomAgent => isCustomAiAssistAgentId(agent);

  String? modelForType(AgentType agentType) {
    final value = selectedModelByAgent[agentType.key]?.trim();
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

  AgentType? agentTypeFor(AiAssistOperation operation) {
    final override = promptSettingsFor(operation).agent;
    return override == null ? agentType : aiAssistAgentTypeForId(override);
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
    return discoveredModelsByAgent[agentType.key] ??
        const <AiAssistDiscoveredModel>[];
  }

  String? discoveredDefaultModelForType(AgentType agentType) {
    final value = discoveredDefaultModelByAgent[agentType.key]?.trim();
    return value == null || value.isEmpty ? null : value;
  }

  static const AiAssistSettings defaults = AiAssistSettings();

  factory fromJson(Map<String, Object?> json) =>
      AiAssistSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}
