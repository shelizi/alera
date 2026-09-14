import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/forms/alera_checkbox.dart';
import 'package:alera/src/design_system/forms/alera_dropdown_field.dart';
import 'package:alera/src/design_system/forms/alera_setting_row.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_settings_group.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_snapshot.dart';
import 'package:alera/src/features/agent_profiles/domain/managed_agent_profile_options.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

class const AgentProfileManagedEditor({
  super.key,
  required final AgentType adapter,
  required final Map<String, Object?> config,
  required final List<ManagedAgentOption> models,
  required final List<ManagedAgentOption> personas,
  required final bool enabled,
  required final ValueChanged<Map<String, Object?>> onChanged,
  required final VoidCallback? onRefreshModels,
  required final VoidCallback? onRefreshPersonas,
  final bool modelsLoading = false,
  final bool personasLoading = false,
  final String? discoveryError,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final descriptor = agentDescriptorFor(adapter);
    final controls = <Widget>[
      if (descriptor.supportsCcsProfile &&
          descriptor.launchSpec.profileLauncher != null)
        _textRow(
          title: 'CCS Profile',
          description:
              'Leave empty to run Claude directly. A profile launches ccs with '
              'the profile first and the same flags after it. CCS points '
              'CLAUDE_CONFIG_DIR at its instance directory. Alera writes Claude '
              'status hooks into that instance settings.local.json.',
          keyName: 'ccsProfile',
          managedControlKey: false,
        ),
      if (descriptor.modelOverride != AgentModelOverrideSnapshot.unsupported) ...<Widget>[
        _choiceRow(
          title: 'Model',
          description: 'Leave as default to use the agent configuration.',
          keyName: 'model',
          options: models,
          filterable: true,
          managedControlKey: false,
          trailing: onRefreshModels == null
              ? null
              : AleraIconButton(
                  tooltip: 'Refresh Models',
                  icon: modelsLoading ? AleraIcons.loading : AleraIcons.refresh,
                  onPressed: enabled && !modelsLoading ? onRefreshModels : null,
                ),
        ),
        _textRow(
          title: 'Exact Model ID',
          description: 'Use a model ID that is not in the discovered list.',
          keyName: 'model',
          managedControlKey: false,
        ),
      ],
      if (descriptor.supportsPersona) ...<Widget>[
        _choiceRow(
          title: 'Persona',
          description: 'Select a known agent persona or enter an exact name.',
          keyName: 'agent',
          options: personas,
          filterable: true,
          managedControlKey: false,
          trailing: onRefreshPersonas == null
              ? null
              : AleraIconButton(
                  tooltip: 'Refresh Personas',
                  icon: personasLoading
                      ? AleraIcons.loading
                      : AleraIcons.refresh,
                  onPressed: enabled && !personasLoading
                      ? onRefreshPersonas
                      : null,
                ),
        ),
        _textRow(
          title: 'Exact Persona',
          description: 'Use a persona name that is not in the discovered list.',
          keyName: 'agent',
          managedControlKey: false,
        ),
      ],
      ..._adapterControls(),
    ];
    return AleraSettingsGroup(
      title: 'Managed Options',
      description: 'Alera builds the interactive command from these agent-specific settings.',
      children: <Widget>[
        ...controls,
        if (discoveryError != null)
          Padding(
            padding: const EdgeInsets.all(AleraTokens.space16),
            child: Text(
              discoveryError!,
              style: Theme.of(context).textTheme.bodySmall
                  ?.copyWith(color: AleraTokens.warning),
            ),
          ),
      ],
    );
  }

  List<Widget> _adapterControls() {
    final descriptor = agentDescriptorFor(adapter);
    return <Widget>[
      for (final rule in descriptor.launchSpec.rules)
        if (rule.key != 'agent' &&
            rule.key != 'model' &&
            !(rule.suppressedBy != null &&
                config[rule.suppressedBy] == true))
          _controlForRule(rule),
    ];
  }

  Widget _controlForRule(AgentLaunchRuleSpec rule) {
    final metadata = _managedControlMetadata(adapter.key, rule.key);
    final title = metadata.title ?? _titleForKey(rule.key);
    switch (rule.kind) {
      case AgentLaunchRuleKindSnapshot.stringOption:
        return _textRow(
          title: title,
          description: metadata.description,
          keyName: rule.key,
        );
      case AgentLaunchRuleKindSnapshot.enumOption:
      case AgentLaunchRuleKindSnapshot.configKv:
      case AgentLaunchRuleKindSnapshot.enumToFlag:
        return _choiceRow(
          title: title,
          description: metadata.description,
          keyName: rule.key,
          options: _optionsForRule(rule, metadata),
        );
      case AgentLaunchRuleKindSnapshot.boolFlag:
      case AgentLaunchRuleKindSnapshot.exclusiveToggle:
        return _boolRow(
          title: title,
          description: metadata.description,
          keyName: rule.key,
        );
      case AgentLaunchRuleKindSnapshot.numberOption:
        return _numberRow(
          title: title,
          keyName: rule.key,
          decimal: metadata.decimal,
        );
    }
  }

  List<ManagedAgentOption> _optionsForRule(
    AgentLaunchRuleSpec rule,
    _ManagedControlMetadata metadata,
  ) {
    return <ManagedAgentOption>[
      for (final value in rule.allowed)
        ManagedAgentOption(value, metadata.valueLabels[value] ?? _titleForKey(value)),
    ];
  }

  Widget _choiceRow({
    required String title,
    required String keyName,
    required List<ManagedAgentOption> options,
    String? description,
    bool filterable = false,
    Widget? trailing,
    bool managedControlKey = true,
  }) {
    final selected = config[keyName] is String ? config[keyName] as String : '';
    final entries = <ManagedAgentOption>[
      const ManagedAgentOption('', 'Agent Default'),
      ...options,
    ];
    final hasSelected = entries.any((option) => option.value == selected);
    if (selected.isNotEmpty && !hasSelected) {
      entries.add(ManagedAgentOption(selected, 'Custom: $selected'));
    }
    return AleraSettingRow(
      key: managedControlKey
          ? ValueKey<String>('ManagedControl:$keyName')
          : null,
      title: title,
      description: description,
      child: Row(
        children: <Widget>[
          Expanded(
            child: AleraDropdownField<String>(
              key: ValueKey<String>('Managed:$keyName:$selected'),
              value: selected,
              entries: <AleraDropdownFieldEntry<String>>[
                for (final option in entries)
                  AleraDropdownFieldEntry<String>(
                    value: option.value,
                    label: option.label,
                  ),
              ],
              enabled: enabled,
              filterable: filterable,
              onChanged: (value) =>
                  _setValue(keyName, value.isEmpty ? null : value),
            ),
          ),
          if (trailing != null) ...<Widget>[
            const SizedBox(width: AleraTokens.space8),
            trailing,
          ],
        ],
      ),
    );
  }

  Widget _textRow({
    required String title,
    required String? description,
    required String keyName,
    bool managedControlKey = true,
  }) {
    return AleraSettingRow(
      key: managedControlKey
          ? ValueKey<String>('ManagedControl:$keyName')
          : null,
      title: title,
      description: description,
      child: _ManagedTextField(
        value: config[keyName]?.toString() ?? '',
        enabled: enabled,
        onChanged: (value) =>
            _setValue(keyName, value.trim().isEmpty ? null : value.trim()),
      ),
    );
  }

  Widget _numberRow({
    required String title,
    required String keyName,
    bool decimal = false,
  }) {
    return AleraSettingRow(
      key: ValueKey<String>('ManagedControl:$keyName'),
      title: title,
      description: 'Leave empty to use the agent default.',
      child: _ManagedTextField(
        value: config[keyName]?.toString() ?? '',
        enabled: enabled,
        keyboardType: .numberWithOptions(decimal: decimal),
        inputFormatters: <TextInputFormatter>[
          FilteringTextInputFormatter.allow(
            decimal ? RegExp(r'[0-9.]') : RegExp(r'[0-9]'),
          ),
        ],
        onChanged: (value) {
          final trimmed = value.trim();
          if (trimmed.isEmpty) {
            _setValue(keyName, null);
            return;
          }
          final parsed = decimal
              ? double.tryParse(trimmed)
              : int.tryParse(trimmed);
          if (parsed != null) {
            _setValue(keyName, parsed);
          }
        },
      ),
    );
  }

  Widget _boolRow({
    required String title,
    required String keyName,
    String? description,
  }) {
    return AleraSettingRow(
      key: ValueKey<String>('ManagedControl:$keyName'),
      title: title,
      description: description,
      child: Align(
        alignment: Alignment.centerRight,
        child: AleraCheckbox(
          value: config[keyName] == true,
          enabled: enabled,
          onChanged: (value) => _setValue(keyName, value ? true : null),
        ),
      ),
    );
  }

  void _setValue(String key, Object? value) {
    final next = <String, Object?>{...config};
    if (value == null) {
      next.remove(key);
    } else {
      next[key] = value;
    }
    onChanged(next);
  }
}

class _ManagedControlMetadata {
  const _ManagedControlMetadata({
    this.title,
    this.description,
    this.valueLabels = const <String, String>{},
    this.decimal = false,
  });

  final String? title;
  final String? description;
  final Map<String, String> valueLabels;
  final bool decimal;
}

const Map<String, _ManagedControlMetadata> _managedControlMetadataMap =
    <String, _ManagedControlMetadata>{
      'effort': _ManagedControlMetadata(
        title: 'Reasoning Effort',
        valueLabels: <String, String>{'xhigh': 'Extra High'},
      ),
      'planModeEffort': _ManagedControlMetadata(
        title: 'Plan Mode Reasoning Effort',
        description:
            'Applies only while Codex is in plan mode, which is entered with Shift+Tab or /plan. Codex has no way to start there.',
        valueLabels: <String, String>{'xhigh': 'Extra High'},
      ),
      'sandbox': _ManagedControlMetadata(
        title: 'Sandbox',
        valueLabels: <String, String>{
          'danger-full-access': 'Full Access',
          'workspace-write': 'Workspace Write',
        },
      ),
      'approvalPolicy': _ManagedControlMetadata(
        title: 'Approval Policy',
        valueLabels: <String, String>{'on-request': 'On Request', 'never': 'Never Ask'},
      ),
      'webSearch': _ManagedControlMetadata(
        title: 'Web Search',
        description: 'Allow Codex to search the web.',
      ),
      'bypassApprovalsAndSandbox': _ManagedControlMetadata(
        title: 'Bypass All Protections',
        description: 'Bypass both approval prompts and sandbox isolation.',
      ),
      'allowSkipPermissions': _ManagedControlMetadata(
        title: 'Allow Skip Permissions',
        description:
            'Make bypass available during the session without starting in it. Use the Bypass Permissions mode above to start in it.',
      ),
      'mode': _ManagedControlMetadata(title: 'Mode'),
      'context': _ManagedControlMetadata(
        title: 'Context',
        valueLabels: <String, String>{
          'default': 'Default Context',
          'long_context': 'Long Context',
        },
      ),
      'allowAll': _ManagedControlMetadata(
        title: 'Allow All',
        description: 'Allow tools and paths without individual prompts.',
      ),
      'maxAiCredits': _ManagedControlMetadata(
        title: 'Maximum AI Credits',
        decimal: true,
      ),
      'maxAutopilotContinues': _ManagedControlMetadata(
        title: 'Maximum Autopilot Continues',
      ),
      'noAskUser': _ManagedControlMetadata(
        title: 'Do Not Ask User',
        description: 'Continue without asking the user for input.',
      ),
      'permissionMode': _ManagedControlMetadata(
        title: 'Permission Mode',
        valueLabels: <String, String>{'dontAsk': 'Do Not Ask'},
      ),
      'cursor:permissionMode': _ManagedControlMetadata(title: 'Review Mode'),
      'trustWorkspace': _ManagedControlMetadata(
        title: 'Trust Workspace',
        description: 'Trust the workspace without an interactive prompt.',
      ),
      'skipPermissions': _ManagedControlMetadata(
        title: 'Skip Permissions',
        description: 'Run without Antigravity permission checks.',
      ),
      'agy:sandbox': _ManagedControlMetadata(
        title: 'Sandbox',
        description: 'Enable the Antigravity sandbox.',
      ),
      'autoApprove': _ManagedControlMetadata(
        title: 'Auto Approve',
        description: 'Approve OpenCode actions automatically.',
      ),
      'thinking': _ManagedControlMetadata(title: 'Thinking'),
      'projectTrust': _ManagedControlMetadata(title: 'Project Trust'),
      'amp:mode': _ManagedControlMetadata(
        title: 'Mode',
        description:
            'Amp permission rules continue to come from the global Amp configuration.',
      ),
      'fast': _ManagedControlMetadata(
        title: 'Fast Mode',
        description: 'Prefer lower latency responses.',
      ),
      'grok:permissionMode': _ManagedControlMetadata(
        title: 'Permission Mode',
      ),
      'devin:sandbox': _ManagedControlMetadata(
        title: 'Sandbox',
        description: 'Sandbox Devin exec-tool processes where supported.',
      ),
      'disableWebSearch': _ManagedControlMetadata(
        title: 'Disable Web Search',
        description: 'Disable Grok Build web search and web fetch tools.',
      ),
      'resumeLast': _ManagedControlMetadata(title: 'Resume Latest Session'),
      'noAdditionalDirs': _ManagedControlMetadata(
        title: 'Ignore Additional Directories',
        description: 'Do not load additional directories configured by fx.',
      ),
      'record': _ManagedControlMetadata(title: 'Record Session'),
    };

_ManagedControlMetadata _managedControlMetadata(String agentId, String key) {
  return _managedControlMetadataMap['$agentId:$key'] ??
      _managedControlMetadataMap[key] ??
      const _ManagedControlMetadata();
}

String _titleForKey(String key) {
  final words = key
      .replaceAll(RegExp(r'([a-z])([A-Z])'), r'$1 $2')
      .replaceAll('_', ' ')
      .replaceAll('-', ' ')
      .split(' ')
      .where((word) => word.isNotEmpty)
      .map((word) => '${word[0].toUpperCase()}${word.substring(1)}');
  return words.join(' ');
}

class const _ManagedTextField({
  required final String value,
  required final bool enabled,
  required final ValueChanged<String> onChanged,
  final TextInputType? keyboardType,
  final List<TextInputFormatter>? inputFormatters,
}) extends StatefulWidget {
  @override
  State<_ManagedTextField> createState() => _ManagedTextFieldState();
}

class _ManagedTextFieldState extends State<_ManagedTextField> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.value);
  }

  @override
  void didUpdateWidget(_ManagedTextField oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.value != oldWidget.value && widget.value != _controller.text) {
      _controller.text = widget.value;
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AleraTextField(
      controller: _controller,
      enabled: widget.enabled,
      dense: true,
      keyboardType: widget.keyboardType,
      inputFormatters: widget.inputFormatters,
      onChanged: widget.onChanged,
    );
  }
}
