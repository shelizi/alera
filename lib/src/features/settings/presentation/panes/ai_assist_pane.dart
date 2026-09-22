import 'dart:async';

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/layout/alera_settings_group.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_providers.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_diff_only_execution.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_registry.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_model_discovery_service.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/ai_assist/presentation/ai_assist_agent_choice.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/presentation/panes/ai_assist_setting_rows.dart';
import 'package:alera/src/features/settings/presentation/panes/ai_assist_custom_command_dialog.dart';
import 'package:alera/src/features/settings/presentation/rows/settings_rows.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class const AiAssistSettingsPane({
  super.key,
  required final AiAssistSettings settings,
  required final ValueChanged<AiAssistSettings Function(AiAssistSettings)>
  onChanged,
  final Map<String, GlobalKey> groupKeys = const <String, GlobalKey>{},
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<AiAssistSettingsPane> createState() =>
      _AiAssistSettingsPaneState();
}

class _AiAssistSettingsPaneState extends ConsumerState<AiAssistSettingsPane> {
  static const List<AiAssistOperation> _configuredOperations =
      <AiAssistOperation>[
        AiAssistOperation.commitMessage,
        AiAssistOperation.pullRequestDetails,
        AiAssistOperation.readingDiff,
        AiAssistOperation.workspaceIdentity,
        AiAssistOperation.agentTitle,
        AiAssistOperation.speechMessage,
      ];

  final Map<AgentType, _AiAssistModelDiscoveryState> _discovery =
      <AgentType, _AiAssistModelDiscoveryState>{};
  final Set<AgentType> _autoDiscovered = <AgentType>{};

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      _autoDiscoverConfiguredAgents();
    });
  }

  @override
  void didUpdateWidget(covariant AiAssistSettingsPane oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.settings.agent != widget.settings.agent ||
        oldWidget.settings.enabled != widget.settings.enabled ||
        oldWidget.settings.promptSettingsByOperation !=
            widget.settings.promptSettingsByOperation) {
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _autoDiscoverConfiguredAgents();
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final settings = widget.settings;
    final agentType = settings.agentType;
    final agentChoice = agentType == null
        ? const AiAssistAgentChoice.custom()
        : AiAssistAgentChoice.agent(agentType);
    final spec = aiAssistCapabilityFor(agentType);
    final models = agentType == null
        ? const <AiAssistModel>[]
        : modelsForAgentType(agentType, settings);
    final model = agentType == null
        ? const AiAssistModel(id: 'custom', label: 'Custom')
        : modelForAgentType(
            agentType,
            settings.modelForType(agentType) ??
                defaultModelIdForAgentType(agentType, settings),
            extraModels: discoveredModelsForAgentType(settings, agentType),
          );
    final thinkingLevels = model.thinkingLevels;
    final discovery = agentType == null
        ? const _AiAssistModelDiscoveryState()
        : _discovery[agentType] ?? const _AiAssistModelDiscoveryState();
    final canDiscoverModels = spec?.modelsCommand != null;
    return Column(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        KeyedSubtree(
          key: widget.groupKeys['generation'],
          child: AleraSettingsGroup(
            title: 'Generation',
            description: 'Local agent CLIs run short background jobs from source control and workspace context.',
            children: <Widget>[
              SettingsSwitchRow(
                title: 'Enable AI Assist',
                description: 'Generate text for source control, workspaces, and agent conversations.',
                value: widget.settings.enabled,
                onChanged: (value) => widget.onChanged(
                  (settings) => settings.copyWith(enabled: value),
                ),
              ),
              SettingsSwitchRow(
                title: 'Auto-Generate Agent Titles',
                description: 'Name new agent conversations from their first prompt or recent context.',
                value: settings.autoGenerateAgentTitles,
                onChanged: (value) => widget.onChanged(
                  (settings) =>
                      settings.copyWith(autoGenerateAgentTitles: value),
                ),
              ),
              AiAssistAgentRow(
                value: agentChoice,
                onChanged: (value) => unawaited(_selectAgent(value)),
              ),
              if (agentType == null)
                SettingsTextRow(
                  title: 'Custom Command',
                  description: 'Use {prompt} to pass the prompt as an argument; otherwise Alera sends it on stdin.',
                  value: settings.customCommand,
                  hintText: 'llm --system commit-message',
                  onChanged: (value) => widget.onChanged(
                    (settings) => settings.copyWith(customCommand: value),
                  ),
                )
              else if (spec != null)
                AiAssistModelRow(
                  agentType: agentType,
                  models: models,
                  value: model.id,
                  canDiscoverModels: canDiscoverModels,
                  discovering: discovery.loading,
                  discoveryError: discovery.error,
                  onRefreshModels: canDiscoverModels
                      ? () => unawaited(_discoverModels(agentType))
                      : null,
                  onChanged: (value) => widget.onChanged((settings) {
                    final selectedModels = <String, String>{
                      ...settings.selectedModelByAgent,
                    };
                    if (value.trim().isEmpty) {
                      selectedModels.remove(agentType.key);
                    } else {
                      selectedModels[agentType.key] = value;
                    }
                    return settings.copyWith(
                      selectedModelByAgent: selectedModels,
                    );
                  }),
                ),
              if (thinkingLevels.isNotEmpty)
                AiAssistThinkingRow(
                  levels: thinkingLevels,
                  value:
                      settings.thinkingForModel(model.id) ??
                      model.defaultThinkingLevel ??
                      thinkingLevels.first.id,
                  onChanged: (value) => widget.onChanged(
                    (settings) => settings.copyWith(
                      selectedThinkingByModel: <String, String>{
                        ...settings.selectedThinkingByModel,
                        model.id: value,
                      },
                    ),
                  ),
                ),
              if (agentType != null &&
                  _configuredOperations.any(
                    (operation) => settings.agentTypeFor(operation) == null,
                  ))
                SettingsTextRow(
                  title: 'Custom Command',
                  description: 'Used by prompts that override the global agent with custom command.',
                  value: settings.customCommand,
                  hintText: 'llm --system commit-message',
                  onChanged: (value) => widget.onChanged(
                    (settings) => settings.copyWith(customCommand: value),
                  ),
                ),
            ],
          ),
        ),
        for (final operation in _configuredOperations) ...<Widget>[
          KeyedSubtree(
            key: widget.groupKeys[operation.key],
            child: AleraSettingsGroup(
              title: operation.label,
              description: 'Configure the agent, model, reasoning and instructions for this prompt.',
              children: <Widget>[
                ..._promptOverrideRows(settings, operation),
                ..._thinkingRows(settings, operation),
                _instructionRow(settings, operation),
              ],
            ),
          ),
          if (operation != _configuredOperations.last)
            const SizedBox(height: AleraTokens.space16),
        ],
      ],
    );
  }

  List<Widget> _thinkingRows(
    AiAssistSettings settings,
    AiAssistOperation operation,
  ) {
    final agentType = operation == AiAssistOperation.readingDiff
        ? readingDiffAgentTypeForSettings(settings)
        : settings.agentTypeFor(operation);
    if (agentType == null) {
      return const <Widget>[];
    }
    final model = modelForAgentType(
      agentType,
      (operation == AiAssistOperation.readingDiff
              ? readingDiffModelForSettingsType(settings, agentType)
              : settings.modelForOperation(operation)) ??
          defaultModelIdForAgentType(agentType, settings),
      extraModels: discoveredModelsForAgentType(settings, agentType),
    );
    if (model.thinkingLevels.isEmpty) {
      return const <Widget>[];
    }
    return <Widget>[
      AiAssistThinkingRow(
        controlKey: '${operation.key}-reasoning',
        levels: model.thinkingLevels,
        value:
            settings.thinkingForOperation(operation, model.id) ??
            model.defaultThinkingLevel ??
            model.thinkingLevels.first.id,
        onChanged: (value) => widget.onChanged(
          (settings) => settings.copyWith(
            selectedThinkingByOperation:
                <AiAssistOperation, Map<String, String>>{
                  ...settings.selectedThinkingByOperation,
                  operation: <String, String>{
                    ...settings.selectedThinkingByOperation[operation] ??
                        const <String, String>{},
                    model.id: value,
                  },
                },
          ),
        ),
      ),
    ];
  }

  Widget _instructionRow(
    AiAssistSettings settings,
    AiAssistOperation operation,
  ) {
    return InstructionSettingRow(
      title: 'Instructions',
      value: settings.instructionsFor(operation),
      onChanged: (value) => widget.onChanged(
        (settings) => settings.copyWith(
          instructionsByOperation: <AiAssistOperation, String>{
            ...settings.instructionsByOperation,
            operation: value,
          },
        ),
      ),
    );
  }

  List<Widget> _promptOverrideRows(
    AiAssistSettings settings,
    AiAssistOperation operation,
  ) {
    final promptSettings = settings.promptSettingsFor(operation);
    final isReadingDiff = operation == AiAssistOperation.readingDiff;
    final globalSupported = supportsDiffOnlyAiAssistAgentType(
      settings.agentType,
    );
    final agentType = isReadingDiff
        ? readingDiffAgentTypeForSettings(settings)
        : settings.agentTypeFor(operation);
    final effectiveChoice = agentType == null
        ? const AiAssistAgentChoice.custom()
        : AiAssistAgentChoice.agent(agentType);
    final configuredAgentType = promptSettings.agentType ?? settings.agentType;
    final usesReadingDiffFallback =
        isReadingDiff &&
        !supportsDiffOnlyAiAssistAgentType(configuredAgentType);
    final promptChoice = usesReadingDiffFallback
        ? effectiveChoice
        : _choiceForPersistedAgent(promptSettings.agent);
    final effectivePromptModel = usesReadingDiffFallback
        ? null
        : promptSettings.model;
    final inheritedModel = agentType == null
        ? const AiAssistModel(id: 'custom', label: 'Custom')
        : modelForAgentType(
            agentType,
            settings.modelForType(agentType) ??
                defaultModelIdForAgentType(agentType, settings),
            extraModels: discoveredModelsForAgentType(settings, agentType),
          );
    final spec = aiAssistCapabilityFor(agentType);
    final discovery = agentType == null
        ? const _AiAssistModelDiscoveryState()
        : _discovery[agentType] ?? const _AiAssistModelDiscoveryState();
    return <Widget>[
      AiAssistPromptAgentRow(
        operation: operation,
        globalAgent: settings.agentType == null
            ? const AiAssistAgentChoice.custom()
            : AiAssistAgentChoice.agent(settings.agentType!),
        value: promptChoice,
        allowedAgentTypes: isReadingDiff ? diffOnlyAiAssistAgentTypes : null,
        allowGlobal: !isReadingDiff || globalSupported,
        allowCustom: operation != AiAssistOperation.speechMessage,
        onChanged: (choice) =>
            unawaited(_selectAgent(choice, operation: operation)),
      ),
      if (agentType != null)
        AiAssistPromptModelRow(
          operation: operation,
          agentType: agentType,
          models: modelsForAgentType(agentType!, settings),
          inheritedModel: inheritedModel,
          value: effectivePromptModel,
          discovering: discovery.loading,
          discoveryError: discovery.error,
          onRefreshModels: spec?.modelsCommand == null
              ? null
              : () => unawaited(_discoverModels(agentType)),
          onChanged: (model) => widget.onChanged(
            (settings) => _withPromptSettings(
              settings,
              operation,
              AiAssistPromptSettings(
                agent: _persistedAgentForChoice(promptChoice),
                model: model,
              ),
            ),
          ),
        ),
    ];
  }

  Future<void> _selectAgent(
    AiAssistAgentChoice choice, {
    AiAssistOperation? operation,
  }) async {
    String? command;
    if (choice.isCustom && widget.settings.customCommand.trim().isEmpty) {
      command = await showDialog<String>(
        context: context,
        builder: (_) => const AiAssistCustomCommandDialog(),
      );
      if (!mounted || command == null) return;
    }
    widget.onChanged((settings) {
      if (command != null) settings = settings.copyWith(customCommand: command);
      if (operation == null) {
        return _withGlobalAgent(settings, choice);
      }
      final previousAgentType = settings.agentTypeFor(operation);
      final usesFallback =
          operation == AiAssistOperation.readingDiff &&
          !supportsDiffOnlyAiAssistAgentType(previousAgentType);
      final selectedAgent = _persistedAgentForChoice(choice);
      final selectedAgentType = choice.isGlobal
          ? settings.agentType
          : choice.agentType;
      return _withPromptSettings(
        settings,
        operation,
        AiAssistPromptSettings(
          agent: selectedAgent,
          model: !usesFallback && previousAgentType == selectedAgentType
              ? settings.promptSettingsFor(operation).model
              : null,
        ),
      );
    });
  }

  AiAssistSettings _withPromptSettings(
    AiAssistSettings settings,
    AiAssistOperation operation,
    AiAssistPromptSettings promptSettings,
  ) {
    final updated = <AiAssistOperation, AiAssistPromptSettings>{
      ...settings.promptSettingsByOperation,
    };
    if (promptSettings.inheritsAgent && promptSettings.inheritsModel) {
      updated.remove(operation);
    } else {
      updated[operation] = promptSettings;
    }
    return settings.copyWith(promptSettingsByOperation: updated);
  }

  AiAssistSettings _withGlobalAgent(
    AiAssistSettings settings,
    AiAssistAgentChoice choice,
  ) {
    final updated = <AiAssistOperation, AiAssistPromptSettings>{
      for (final entry in settings.promptSettingsByOperation.entries)
        if (entry.value.agent != null) entry.key: entry.value,
    };
    return settings.copyWith(
      agent: _persistedAgentForChoice(choice)!,
      promptSettingsByOperation: updated,
    );
  }

  void _autoDiscoverConfiguredAgents() {
    if (!mounted || !widget.settings.enabled) {
      return;
    }
    final agentTypes = aiAssistAgentTypesForModelDiscovery(
      widget.settings,
      _configuredOperations,
    );
    for (final agentType in agentTypes) {
      _autoDiscoverAgent(agentType);
    }
  }

  void _autoDiscoverAgent(AgentType agentType) {
    final spec = aiAssistCapabilityFor(agentType);
    if (spec?.modelsCommand == null ||
        _autoDiscovered.contains(agentType) ||
        (_discovery[agentType]?.loading ?? false)) {
      return;
    }
    _autoDiscovered.add(agentType);
    unawaited(_discoverModels(agentType));
  }

  Future<void> _discoverModels(AgentType agentType) async {
    final spec = aiAssistCapabilityFor(agentType);
    if (spec?.modelsCommand == null) {
      return;
    }
    setState(() {
      _discovery[agentType] = const _AiAssistModelDiscoveryState(loading: true);
    });
    final AiAssistModelDiscoveryResult result;
    try {
      result = await ref
          .read(aiAssistModelDiscoveryServiceProvider)
          .discover(agentType);
    } catch (error) {
      if (!mounted) {
        return;
      }
      setState(() {
        _discovery[agentType] = _AiAssistModelDiscoveryState(
          error: error.toString(),
        );
      });
      return;
    }
    if (!mounted) {
      return;
    }
    if (!result.success) {
      setState(() {
        _discovery[agentType] = _AiAssistModelDiscoveryState(
          error: result.error,
        );
      });
      return;
    }
    widget.onChanged((latest) {
      final discoveredDefaults = <String, String>{
        ...latest.discoveredDefaultModelByAgent,
      };
      if (result.defaultModelId == null) {
        discoveredDefaults.remove(agentType.key);
      } else {
        discoveredDefaults[agentType.key] = result.defaultModelId!;
      }
      return latest.copyWith(
        discoveredModelsByAgent: <String, List<AiAssistDiscoveredModel>>{
          ...latest.discoveredModelsByAgent,
          agentType.key: <AiAssistDiscoveredModel>[
            for (final model in result.models) model.toDiscovered(),
          ],
        },
        discoveredDefaultModelByAgent: discoveredDefaults,
      );
    });
    setState(() {
      _discovery[agentType] = const _AiAssistModelDiscoveryState();
    });
  }

  AiAssistAgentChoice _choiceForPersistedAgent(AiAssistAgent? agent) {
    if (agent == null) return const AiAssistAgentChoice.global();
    final type = agent.agentType;
    return type == null
        ? const AiAssistAgentChoice.custom()
        : AiAssistAgentChoice.agent(type);
  }

  AiAssistAgent? _persistedAgentForChoice(AiAssistAgentChoice choice) {
    if (choice.isGlobal) return null;
    if (choice.isCustom) return AiAssistAgent.custom;
    return AiAssistAgent.fromAgentType(choice.agentType!);
  }
}

class const _AiAssistModelDiscoveryState({
  final bool loading = false,
  final String? error,
});
