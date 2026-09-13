// GENERATED FILE. Do not edit by hand.
// Regenerate with: cargo run -p alera-cli -- export-agent-descriptors

enum AgentStartupPromptKindSnapshot {
  positionalAfterTerminator,
  positional,
  longOption,
  stdinScript,
  terminalAfterReady,
}

enum AgentHookStrategySnapshot {
  none,
  configJson,
  pluginScript,
  runtimeHome,
  sessionOverlay,
  herdrSocket,
}

enum AgentStatusStrategySnapshot {
  hookEvents,
  hookEventsWithTranscriptWatch,
  herdrSocket,
}

enum AgentModelOverrideSnapshot {
  supported,
  unsupported,
  profileOnly,
}

enum AgentLaunchRuleKindSnapshot {
  stringOption,
  enumOption,
  boolFlag,
  numberOption,
  configKv,
  enumToFlag,
  exclusiveToggle,
}

class AgentStartupPromptSnapshot {
  const AgentStartupPromptSnapshot({
    required this.kind,
    required this.option,
  });

  final AgentStartupPromptKindSnapshot kind;
  final String? option;
}

class AgentRiskRuleSnapshot {
  const AgentRiskRuleSnapshot({
    required this.key,
    required this.expectedBool,
    required this.expectedString,
    required this.marker,
    required this.score,
  });

  final String key;
  final bool? expectedBool;
  final String? expectedString;
  final String marker;
  final int score;
}

class AgentProfileLauncherSnapshot {
  const AgentProfileLauncherSnapshot({
    required this.key,
    required this.executable,
  });

  final String key;
  final String executable;
}

class AgentLaunchRuleSpec {
  const AgentLaunchRuleSpec({
    required this.key,
    required this.suppressedBy,
    required this.kind,
    required this.flag,
    required this.allowed,
    required this.positiveOnly,
    required this.rustKey,
    required this.flags,
    required this.conflicts,
  });

  final String key;
  final String? suppressedBy;
  final AgentLaunchRuleKindSnapshot kind;
  final String? flag;
  final List<String> allowed;
  final bool positiveOnly;
  final String? rustKey;
  final Map<String, String> flags;
  final List<String> conflicts;
}

class AgentLaunchSpecSnapshot {
  const AgentLaunchSpecSnapshot({
    required this.allowedKeys,
    required this.profileLauncher,
    required this.rules,
  });

  final List<String> allowedKeys;
  final AgentProfileLauncherSnapshot? profileLauncher;
  final List<AgentLaunchRuleSpec> rules;
}

class AgentDescriptorSnapshot {
  const AgentDescriptorSnapshot({
    required this.id,
    required this.aliases,
    required this.displayName,
    required this.defaultCommand,
    required this.forceSubmit,
    required this.interruptBytes,
    required this.startupPrompt,
    required this.hookStrategy,
    required this.statusStrategy,
    required this.quotaProviderId,
    required this.transcriptUsage,
    required this.modelOverride,
    required this.supportsPersona,
    required this.supportsCcsProfile,
    required this.riskWarning,
    required this.riskWarningSevere,
    required this.riskRules,
    required this.launchSpec,
  });

  final String id;
  final List<String> aliases;
  final String displayName;
  final String defaultCommand;
  final bool forceSubmit;
  final List<int> interruptBytes;
  final AgentStartupPromptSnapshot startupPrompt;
  final AgentHookStrategySnapshot hookStrategy;
  final AgentStatusStrategySnapshot statusStrategy;
  final String? quotaProviderId;
  final bool transcriptUsage;
  final AgentModelOverrideSnapshot modelOverride;
  final bool supportsPersona;
  final bool supportsCcsProfile;
  final String riskWarning;
  final String? riskWarningSevere;
  final List<AgentRiskRuleSnapshot> riskRules;
  final AgentLaunchSpecSnapshot launchSpec;
}

const List<AgentDescriptorSnapshot> agentDescriptorSnapshots = <AgentDescriptorSnapshot>[
  AgentDescriptorSnapshot(id: 'codex', aliases: <String>[], displayName: 'Codex', defaultCommand: 'codex', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.positionalAfterTerminator, option: null), hookStrategy: AgentHookStrategySnapshot.runtimeHome, statusStrategy: AgentStatusStrategySnapshot.hookEventsWithTranscriptWatch, quotaProviderId: 'codex', transcriptUsage: true, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: false, supportsCcsProfile: false, riskWarning: 'This profile reduces Codex approval or sandbox protections.', riskWarningSevere: 'This profile will bypass Codex approvals and sandbox protections.', riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'bypassApprovalsAndSandbox', expectedBool: true, expectedString: null, marker: 'bypassApprovalsAndSandbox', score: 100), AgentRiskRuleSnapshot(key: 'sandbox', expectedBool: null, expectedString: 'danger-full-access', marker: 'dangerFullAccess', score: 40), AgentRiskRuleSnapshot(key: 'approvalPolicy', expectedBool: null, expectedString: 'never', marker: 'neverAsk', score: 30)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'effort', 'planModeEffort', 'sandbox', 'approvalPolicy', 'webSearch', 'bypassApprovalsAndSandbox'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'effort', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.configKv, flag: null, allowed: <String>['minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra'], positiveOnly: false, rustKey: 'model_reasoning_effort', flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'planModeEffort', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.configKv, flag: null, allowed: <String>['minimal', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra'], positiveOnly: false, rustKey: 'plan_mode_reasoning_effort', flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'bypassApprovalsAndSandbox', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.exclusiveToggle, flag: '--dangerously-bypass-approvals-and-sandbox', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>['sandbox', 'approvalPolicy']), AgentLaunchRuleSpec(key: 'sandbox', suppressedBy: 'bypassApprovalsAndSandbox', kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--sandbox', allowed: <String>['read-only', 'workspace-write', 'danger-full-access'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'approvalPolicy', suppressedBy: 'bypassApprovalsAndSandbox', kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--ask-for-approval', allowed: <String>['untrusted', 'on-request', 'never'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'webSearch', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--search', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'claude', aliases: <String>[], displayName: 'Claude Code', defaultCommand: 'claude', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.positionalAfterTerminator, option: null), hookStrategy: AgentHookStrategySnapshot.runtimeHome, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: 'claude', transcriptUsage: true, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: true, supportsCcsProfile: true, riskWarning: 'This profile lets Claude continue with reduced permission prompts.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'permissionMode', expectedBool: null, expectedString: 'bypassPermissions', marker: 'bypassPermissions', score: 100), AgentRiskRuleSnapshot(key: 'permissionMode', expectedBool: null, expectedString: 'dontAsk', marker: 'dontAsk', score: 40), AgentRiskRuleSnapshot(key: 'allowSkipPermissions', expectedBool: true, expectedString: null, marker: 'allowSkipPermissions', score: 30)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'effort', 'agent', 'permissionMode', 'allowSkipPermissions', 'ccsProfile'], profileLauncher: AgentProfileLauncherSnapshot(key: 'ccsProfile', executable: 'ccs'), rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'effort', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--effort', allowed: <String>['low', 'medium', 'high', 'xhigh', 'max'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'agent', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.stringOption, flag: '--agent', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'permissionMode', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--permission-mode', allowed: <String>['acceptEdits', 'auto', 'bypassPermissions', 'manual', 'dontAsk', 'plan'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'allowSkipPermissions', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--allow-dangerously-skip-permissions', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'copilot', aliases: <String>[], displayName: 'GitHub Copilot', defaultCommand: 'copilot', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.longOption, option: '--interactive'), hookStrategy: AgentHookStrategySnapshot.configJson, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: null, transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: true, supportsCcsProfile: false, riskWarning: 'This profile lets Copilot take broader actions with less supervision.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'allowAll', expectedBool: true, expectedString: null, marker: 'allowAll', score: 70), AgentRiskRuleSnapshot(key: 'mode', expectedBool: null, expectedString: 'autopilot', marker: 'autopilot', score: 40), AgentRiskRuleSnapshot(key: 'noAskUser', expectedBool: true, expectedString: null, marker: 'noAskUser', score: 30)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'effort', 'agent', 'mode', 'context', 'allowAll', 'maxAiCredits', 'maxAutopilotContinues', 'noAskUser'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'effort', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--effort', allowed: <String>['none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'agent', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.stringOption, flag: '--agent', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'mode', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--mode', allowed: <String>['interactive', 'plan', 'autopilot'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'context', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--context', allowed: <String>['default', 'long_context'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'allowAll', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--allow-all', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'maxAiCredits', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.numberOption, flag: '--max-ai-credits', allowed: <String>[], positiveOnly: true, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'maxAutopilotContinues', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.numberOption, flag: '--max-autopilot-continues', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'noAskUser', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--no-ask-user', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'cursor', aliases: <String>[], displayName: 'Cursor', defaultCommand: 'cursor-agent', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.positionalAfterTerminator, option: null), hookStrategy: AgentHookStrategySnapshot.sessionOverlay, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: 'cursor', transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: false, supportsCcsProfile: false, riskWarning: 'This profile reduces Cursor review, sandbox, or trust protections.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'permissionMode', expectedBool: null, expectedString: 'force', marker: 'force', score: 70), AgentRiskRuleSnapshot(key: 'sandbox', expectedBool: null, expectedString: 'disabled', marker: 'sandboxDisabled', score: 50), AgentRiskRuleSnapshot(key: 'trustWorkspace', expectedBool: true, expectedString: null, marker: 'trustWorkspace', score: 20)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'mode', 'permissionMode', 'sandbox', 'trustWorkspace'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'mode', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--mode', allowed: <String>['plan', 'ask'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'permissionMode', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumToFlag, flag: null, allowed: <String>['autoReview', 'force'], positiveOnly: false, rustKey: null, flags: <String, String>{'autoReview': '--auto-review', 'force': '--force'}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'sandbox', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--sandbox', allowed: <String>['enabled', 'disabled'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'trustWorkspace', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--trust', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'agy', aliases: <String>['antigravity'], displayName: 'Antigravity', defaultCommand: 'agy', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.longOption, option: '--prompt-interactive'), hookStrategy: AgentHookStrategySnapshot.configJson, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: 'agy', transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: true, supportsCcsProfile: false, riskWarning: 'This profile lets Antigravity skip permission checks.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'skipPermissions', expectedBool: true, expectedString: null, marker: 'skipPermissions', score: 100)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'effort', 'agent', 'mode', 'skipPermissions', 'sandbox'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'effort', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--effort', allowed: <String>['low', 'medium', 'high'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'agent', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.stringOption, flag: '--agent', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'mode', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--mode', allowed: <String>['accept-edits', 'plan'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'skipPermissions', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--dangerously-skip-permissions', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'sandbox', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--sandbox', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'opencode', aliases: <String>[], displayName: 'OpenCode', defaultCommand: 'opencode', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.longOption, option: '--prompt'), hookStrategy: AgentHookStrategySnapshot.pluginScript, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: 'opencode', transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: true, supportsCcsProfile: false, riskWarning: 'This profile lets OpenCode approve actions automatically.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'autoApprove', expectedBool: true, expectedString: null, marker: 'autoApprove', score: 60)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'agent', 'autoApprove'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'agent', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.stringOption, flag: '--agent', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'autoApprove', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--auto', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'opencode2', aliases: <String>[], displayName: 'OpenCode 2', defaultCommand: 'opencode2', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.longOption, option: '--prompt'), hookStrategy: AgentHookStrategySnapshot.pluginScript, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: null, transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.profileOnly, supportsPersona: true, supportsCcsProfile: false, riskWarning: 'This profile lets OpenCode approve actions automatically.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'autoApprove', expectedBool: true, expectedString: null, marker: 'autoApprove', score: 60)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'agent', 'autoApprove'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'autoApprove', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--auto', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'pi', aliases: <String>[], displayName: 'Pi', defaultCommand: 'pi', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.positional, option: null), hookStrategy: AgentHookStrategySnapshot.pluginScript, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: null, transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: false, supportsCcsProfile: false, riskWarning: 'This profile pre-approves project trust for Pi.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'projectTrust', expectedBool: null, expectedString: 'approve', marker: 'projectTrust', score: 30)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'thinking', 'projectTrust'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'thinking', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--thinking', allowed: <String>['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'projectTrust', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumToFlag, flag: null, allowed: <String>['approve', 'ignore'], positiveOnly: false, rustKey: null, flags: <String, String>{'approve': '--approve', 'ignore': '--no-approve'}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'amp', aliases: <String>[], displayName: 'Amp', defaultCommand: 'amp', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.stdinScript, option: null), hookStrategy: AgentHookStrategySnapshot.pluginScript, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: null, transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.unsupported, supportsPersona: false, supportsCcsProfile: false, riskWarning: '', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['mode', 'fast'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'mode', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--mode', allowed: <String>['low', 'medium', 'high', 'ultra'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'fast', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--fast', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'grok', aliases: <String>[], displayName: 'Grok Build', defaultCommand: 'grok', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.positionalAfterTerminator, option: null), hookStrategy: AgentHookStrategySnapshot.configJson, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: 'grok', transcriptUsage: true, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: true, supportsCcsProfile: false, riskWarning: 'This profile lets Grok Build continue with reduced permission prompts.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'permissionMode', expectedBool: null, expectedString: 'bypassPermissions', marker: 'bypassPermissions', score: 100), AgentRiskRuleSnapshot(key: 'permissionMode', expectedBool: null, expectedString: 'dontAsk', marker: 'dontAsk', score: 40)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'effort', 'agent', 'permissionMode', 'sandbox', 'disableWebSearch'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'effort', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--effort', allowed: <String>['none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'agent', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.stringOption, flag: '--agent', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'permissionMode', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--permission-mode', allowed: <String>['default', 'acceptEdits', 'auto', 'dontAsk', 'bypassPermissions', 'plan'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'sandbox', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--sandbox', allowed: <String>['off', 'workspace', 'devbox', 'read-only', 'strict'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'disableWebSearch', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--disable-web-search', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'devin', aliases: <String>[], displayName: 'Devin', defaultCommand: 'devin', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.positionalAfterTerminator, option: null), hookStrategy: AgentHookStrategySnapshot.configJson, statusStrategy: AgentStatusStrategySnapshot.hookEvents, quotaProviderId: 'devin', transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.supported, supportsPersona: false, supportsCcsProfile: false, riskWarning: 'This profile lets Devin take broader actions with less supervision.', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[AgentRiskRuleSnapshot(key: 'permissionMode', expectedBool: null, expectedString: 'dangerous', marker: 'dangerous', score: 100), AgentRiskRuleSnapshot(key: 'permissionMode', expectedBool: null, expectedString: 'smart', marker: 'smart', score: 30)], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['model', 'permissionMode', 'sandbox'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'permissionMode', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.enumOption, flag: '--permission-mode', allowed: <String>['auto', 'accept-edits', 'smart', 'dangerous'], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'sandbox', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--sandbox', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
  AgentDescriptorSnapshot(id: 'fx', aliases: <String>[], displayName: 'fx', defaultCommand: 'fx', forceSubmit: true, interruptBytes: <int>[3], startupPrompt: AgentStartupPromptSnapshot(kind: AgentStartupPromptKindSnapshot.terminalAfterReady, option: null), hookStrategy: AgentHookStrategySnapshot.herdrSocket, statusStrategy: AgentStatusStrategySnapshot.herdrSocket, quotaProviderId: null, transcriptUsage: false, modelOverride: AgentModelOverrideSnapshot.unsupported, supportsPersona: false, supportsCcsProfile: false, riskWarning: '', riskWarningSevere: null, riskRules: <AgentRiskRuleSnapshot>[], launchSpec: AgentLaunchSpecSnapshot(allowedKeys: <String>['resumeLast', 'noAdditionalDirs', 'record'], profileLauncher: null, rules: <AgentLaunchRuleSpec>[AgentLaunchRuleSpec(key: 'resumeLast', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--continue', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'noAdditionalDirs', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--no-additional-dirs', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[]), AgentLaunchRuleSpec(key: 'record', suppressedBy: null, kind: AgentLaunchRuleKindSnapshot.boolFlag, flag: '--record', allowed: <String>[], positiveOnly: false, rustKey: null, flags: <String, String>{}, conflicts: <String>[])])),
];
