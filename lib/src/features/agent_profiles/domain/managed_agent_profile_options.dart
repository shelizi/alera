import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_snapshot.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_profile_adapters.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';

class const ManagedAgentOption(final String value, final String label);

const List<ManagedAgentOption> codexEffortOptions = <ManagedAgentOption>[
  ManagedAgentOption('minimal', 'Minimal'),
  ManagedAgentOption('low', 'Low'),
  ManagedAgentOption('medium', 'Medium'),
  ManagedAgentOption('high', 'High'),
  ManagedAgentOption('xhigh', 'Extra High'),
  ManagedAgentOption('max', 'Max'),
  ManagedAgentOption('ultra', 'Ultra'),
];

const List<ManagedAgentOption> claudeEffortOptions = <ManagedAgentOption>[
  ManagedAgentOption('low', 'Low'),
  ManagedAgentOption('medium', 'Medium'),
  ManagedAgentOption('high', 'High'),
  ManagedAgentOption('xhigh', 'Extra High'),
  ManagedAgentOption('max', 'Max'),
];

const List<ManagedAgentOption> copilotEffortOptions = <ManagedAgentOption>[
  ManagedAgentOption('none', 'None'),
  ManagedAgentOption('minimal', 'Minimal'),
  ManagedAgentOption('low', 'Low'),
  ManagedAgentOption('medium', 'Medium'),
  ManagedAgentOption('high', 'High'),
  ManagedAgentOption('xhigh', 'Extra High'),
  ManagedAgentOption('max', 'Max'),
];

const List<ManagedAgentOption> basicEffortOptions = <ManagedAgentOption>[
  ManagedAgentOption('low', 'Low'),
  ManagedAgentOption('medium', 'Medium'),
  ManagedAgentOption('high', 'High'),
];

const List<ManagedAgentOption> codexSandboxOptions = <ManagedAgentOption>[
  ManagedAgentOption('read-only', 'Read Only'),
  ManagedAgentOption('workspace-write', 'Workspace Write'),
  ManagedAgentOption('danger-full-access', 'Full Access'),
];

const List<ManagedAgentOption> codexApprovalOptions = <ManagedAgentOption>[
  ManagedAgentOption('untrusted', 'Untrusted'),
  ManagedAgentOption('on-request', 'On Request'),
  ManagedAgentOption('never', 'Never Ask'),
];

const List<ManagedAgentOption> claudePermissionOptions = <ManagedAgentOption>[
  ManagedAgentOption('acceptEdits', 'Accept Edits'),
  ManagedAgentOption('auto', 'Auto'),
  ManagedAgentOption('bypassPermissions', 'Bypass Permissions'),
  ManagedAgentOption('manual', 'Manual'),
  ManagedAgentOption('dontAsk', 'Do Not Ask'),
  ManagedAgentOption('plan', 'Plan'),
];

const List<ManagedAgentOption> copilotModeOptions = <ManagedAgentOption>[
  ManagedAgentOption('interactive', 'Interactive'),
  ManagedAgentOption('plan', 'Plan'),
  ManagedAgentOption('autopilot', 'Autopilot'),
];

const List<ManagedAgentOption> copilotContextOptions = <ManagedAgentOption>[
  ManagedAgentOption('default', 'Default Context'),
  ManagedAgentOption('long_context', 'Long Context'),
];

const List<ManagedAgentOption> cursorModeOptions = <ManagedAgentOption>[
  ManagedAgentOption('plan', 'Plan'),
  ManagedAgentOption('ask', 'Ask'),
];

const List<ManagedAgentOption> cursorPermissionOptions = <ManagedAgentOption>[
  ManagedAgentOption('autoReview', 'Auto Review'),
  ManagedAgentOption('force', 'Force'),
];

const List<ManagedAgentOption> cursorSandboxOptions = <ManagedAgentOption>[
  ManagedAgentOption('enabled', 'Enabled'),
  ManagedAgentOption('disabled', 'Disabled'),
];

const List<ManagedAgentOption> agyModeOptions = <ManagedAgentOption>[
  ManagedAgentOption('accept-edits', 'Accept Edits'),
  ManagedAgentOption('plan', 'Plan'),
];

const List<ManagedAgentOption> piThinkingOptions = <ManagedAgentOption>[
  ManagedAgentOption('off', 'Off'),
  ManagedAgentOption('minimal', 'Minimal'),
  ManagedAgentOption('low', 'Low'),
  ManagedAgentOption('medium', 'Medium'),
  ManagedAgentOption('high', 'High'),
  ManagedAgentOption('xhigh', 'Extra High'),
  ManagedAgentOption('max', 'Max'),
];

const List<ManagedAgentOption> piTrustOptions = <ManagedAgentOption>[
  ManagedAgentOption('approve', 'Approve'),
  ManagedAgentOption('ignore', 'Ignore'),
];

const List<ManagedAgentOption> ampModeOptions = <ManagedAgentOption>[
  ManagedAgentOption('low', 'Low'),
  ManagedAgentOption('medium', 'Medium'),
  ManagedAgentOption('high', 'High'),
  ManagedAgentOption('ultra', 'Ultra'),
];

const List<ManagedAgentOption> grokEffortOptions = <ManagedAgentOption>[
  ManagedAgentOption('none', 'None'),
  ManagedAgentOption('minimal', 'Minimal'),
  ManagedAgentOption('low', 'Low'),
  ManagedAgentOption('medium', 'Medium'),
  ManagedAgentOption('high', 'High'),
  ManagedAgentOption('xhigh', 'Extra High'),
  ManagedAgentOption('max', 'Max'),
];

const List<ManagedAgentOption> grokPermissionOptions = <ManagedAgentOption>[
  ManagedAgentOption('default', 'Default'),
  ManagedAgentOption('acceptEdits', 'Accept Edits'),
  ManagedAgentOption('auto', 'Auto'),
  ManagedAgentOption('dontAsk', 'Do Not Ask'),
  ManagedAgentOption('bypassPermissions', 'Bypass Permissions'),
  ManagedAgentOption('plan', 'Plan'),
];

const List<ManagedAgentOption> grokSandboxOptions = <ManagedAgentOption>[
  ManagedAgentOption('off', 'Off'),
  ManagedAgentOption('workspace', 'Workspace'),
  ManagedAgentOption('devbox', 'Devbox'),
  ManagedAgentOption('read-only', 'Read Only'),
  ManagedAgentOption('strict', 'Strict'),
];

const List<ManagedAgentOption> devinPermissionOptions = <ManagedAgentOption>[
  ManagedAgentOption('auto', 'Auto'),
  ManagedAgentOption('accept-edits', 'Accept Edits'),
  ManagedAgentOption('smart', 'Smart'),
  ManagedAgentOption('dangerous', 'Dangerous'),
];

/// The profile switcher a Claude Code profile may launch through. It takes the
/// profile as its first positional argument and forwards the rest to `claude`
/// unchanged. Mirrors `CCS_EXECUTABLE` in the Rust launch builder.
const String ccsExecutable = 'ccs';

bool agentProfileSupportsCcsProfile(AgentType adapter) {
  return agentDescriptorFor(adapter).supportsCcsProfile;
}

bool agentProfileSupportsModel(AgentType adapter) {
  return agentDescriptorFor(adapter).modelOverride !=
      AgentModelOverrideSnapshot.unsupported;
}

bool agentProfileSupportsPersona(AgentType adapter) {
  return agentDescriptorFor(adapter).supportsPersona;
}

int managedAgentRiskScore(AgentType adapter, Map<String, Object?> config) {
  final descriptor = agentDescriptorFor(adapter);
  var score = 0;
  for (final rule in descriptor.riskRules) {
    if (_managedAgentRiskRuleTriggered(rule, config)) {
      score += rule.score;
    }
  }
  return score;
}

Set<String> managedAgentRiskMarkers(
  AgentType adapter,
  Map<String, Object?> config,
) {
  final descriptor = agentDescriptorFor(adapter);
  final markers = <String>{};
  for (final rule in descriptor.riskRules) {
    if (_managedAgentRiskRuleTriggered(rule, config)) {
      markers.add(rule.marker);
    }
  }
  return markers;
}

String managedAgentRiskWarning(AgentType adapter, Map<String, Object?> config) {
  final descriptor = agentDescriptorFor(adapter);
  final severeWarning = descriptor.riskWarningSevere;
  if (severeWarning != null &&
      descriptor.riskRules.any(
        (rule) =>
            rule.score == 100 && _managedAgentRiskRuleTriggered(rule, config),
      )) {
    return severeWarning;
  }
  return descriptor.riskWarning;
}

bool _managedAgentRiskRuleTriggered(
  AgentRiskRuleSnapshot rule,
  Map<String, Object?> config,
) {
  final value = config[rule.key];
  return (rule.expectedBool != null && value == rule.expectedBool) ||
      (rule.expectedString != null && value == rule.expectedString);
}

String managedAgentCommandPreview(
  AgentType adapter,
  Map<String, Object?> config,
) {
  final descriptor = agentDescriptorFor(adapter);
  var executable = agentProfileDefaultCommands[adapter] ?? adapter.key;
  final arguments = <String>[];
  void stringOption(String key, String flag) {
    final value = config[key];
    if (value is String && value.trim().isNotEmpty) {
      arguments.addAll(<String>[flag, value.trim()]);
    }
  }

  final launcher = descriptor.launchSpec.profileLauncher;
  if (launcher != null) {
    final value = config[launcher.key];
    if (value is String && value.trim().isNotEmpty) {
      executable = launcher.executable;
      arguments.add(value.trim());
    }
  }
  if (descriptor.modelOverride == AgentModelOverrideSnapshot.supported) {
    stringOption('model', '--model');
  }
  for (final rule in descriptor.launchSpec.rules) {
    if (rule.suppressedBy != null && config[rule.suppressedBy] == true) {
      continue;
    }
    switch (rule.kind) {
      case AgentLaunchRuleKindSnapshot.stringOption:
      case AgentLaunchRuleKindSnapshot.enumOption:
        final value = config[rule.key];
        if (value is String && value.trim().isNotEmpty) {
          arguments.addAll(<String>[rule.flag!, value.trim()]);
        }
      case AgentLaunchRuleKindSnapshot.numberOption:
        final value = config[rule.key];
        if (value is num) {
          arguments.addAll(<String>[rule.flag!, value.toString()]);
        }
      case AgentLaunchRuleKindSnapshot.configKv:
        final value = config[rule.key];
        if (value is String && value.isNotEmpty) {
          arguments.addAll(<String>['--config', '${rule.rustKey}=$value']);
        }
      case AgentLaunchRuleKindSnapshot.boolFlag:
      case AgentLaunchRuleKindSnapshot.exclusiveToggle:
        if (config[rule.key] == true) {
          arguments.add(rule.flag!);
        }
      case AgentLaunchRuleKindSnapshot.enumToFlag:
        final value = config[rule.key];
        if (value is String && rule.flags.containsKey(value)) {
          arguments.add(rule.flags[value]!);
        }
    }
  }
  return <String>[
    executable,
    ...arguments.map(_quotePreviewArgument),
  ].join(' ');
}

String _quotePreviewArgument(String value) {
  if (RegExp(r'^[A-Za-z0-9_./:=+-]+$').hasMatch(value)) {
    return value;
  }
  return "'${value.replaceAll("'", "'\"'\"'")}'";
}
