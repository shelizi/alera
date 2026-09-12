import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/ai_dictation/domain/ai_dictation_settings.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_shortcut_settings.dart';
import 'package:alera/src/features/settings/domain/editor_syntax_theme_catalog.dart';
import 'package:alera/src/features/settings/domain/terminal_theme_catalog.dart';
import 'package:alera/src/features/text_actions/domain/text_actions_settings.dart';
import 'package:dart_mappable/dart_mappable.dart';
import 'package:flutter/foundation.dart';

part 'agent_quota_settings.dart';
part 'alera_settings.mapper.dart';
part 'alera_terminal_settings.dart';

@MappableEnum()
enum AppLanguage { system, english, traditionalChinese }

@MappableClass()
class const AgentStatusHookSettings({
  this.codex = false,
  this.claude = false,
  this.copilot = false,
  this.cursor = false,
  this.agy = false,
  this.opencode = false,
  this.opencode2 = false,
  this.pi = false,
  this.amp = false,
  this.grok = false,
  this.devin = false,
  this.fx = false,
}) with AgentStatusHookSettingsMappable {
  final bool codex;
  final bool claude;
  final bool copilot;
  final bool cursor;
  final bool agy;
  final bool opencode;
  final bool opencode2;
  final bool pi;
  final bool amp;
  final bool grok;
  final bool devin;
  final bool fx;

  bool get anyEnabled =>
      codex ||
      claude ||
      copilot ||
      cursor ||
      agy ||
      opencode ||
      opencode2 ||
      pi ||
      amp ||
      grok ||
      devin ||
      fx;

  static const AgentStatusHookSettings defaults = AgentStatusHookSettings();

  factory fromJson(Map<String, Object?> json) =>
      AgentStatusHookSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

@MappableClass()
class const EditorSettings({
  this.tabSize = 4,
  this.themeName = EditorSyntaxThemeNames.alera,
  this.autosaveEnabled = false,
  this.autosaveDelaySeconds = defaultAutosaveDelaySeconds,
  this.externalEditor = ExternalEditorKind.zed,
  this.codeOpenTarget = CodeOpenTarget.alera,
  this.zedExecutableMode = ExternalEditorExecutableMode.automatic,
  this.zedExecutablePath,
  this.externalEditorWorkspaceMode = ExternalEditorWorkspaceMode.newWindow,
  this.autoOpenNewWorkspacesInZed = false,
}) with EditorSettingsMappable {
  static const int minAutosaveDelaySeconds = 1;
  static const int maxAutosaveDelaySeconds = 60;
  static const int defaultAutosaveDelaySeconds = 1;

  /// Number of spaces inserted when the editor handles a Tab key press.
  final int tabSize;

  /// Syntax highlighting theme used by editor tabs.
  final String themeName;

  /// Save dirty editor tabs after they have been idle for the configured delay.
  final bool autosaveEnabled;

  /// Number of idle seconds before an automatic editor save.
  final int autosaveDelaySeconds;

  final ExternalEditorKind externalEditor;
  final CodeOpenTarget codeOpenTarget;
  final ExternalEditorExecutableMode zedExecutableMode;
  final String? zedExecutablePath;
  final ExternalEditorWorkspaceMode externalEditorWorkspaceMode;
  final bool autoOpenNewWorkspacesInZed;

  /// Clamps persisted values before they are used to construct a timer.
  int get effectiveAutosaveDelaySeconds => autosaveDelaySeconds
      .clamp(minAutosaveDelaySeconds, maxAutosaveDelaySeconds)
      .toInt();

  Duration get autosaveDebounce =>
      Duration(seconds: effectiveAutosaveDelaySeconds);

  static const EditorSettings defaults = EditorSettings();

  factory fromJson(Map<String, Object?> json) =>
      EditorSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

@MappableClass()
class const GeneralSettings({
  this.language = AppLanguage.system,
  this.workspaceDirectory,
  this.starClicked = false,
  this.confirmProjectRemoval = true,
  this.confirmWorkspaceRemoval = true,
  this.keepAliveEnabled = false,
  this.showTrayIcon = true,
  this.showDockBadge = true,
  this.showTrayBadge = true,
  this.showPullRequestStatusInSidebar = true,
  this.pullRequestFailureNotificationsEnabled = false,
}) with GeneralSettingsMappable {
  /// Language used by the Alera interface. System resolves Chinese locales to
  /// Traditional Chinese and falls back to English for unsupported locales.
  final AppLanguage language;

  /// User-configured root directory where new linked workspaces are created.
  /// `null` falls back to the platform default (`~/.alera/workspaces`).
  final String? workspaceDirectory;

  /// Local flag after the GitHub star flow; Settings still uses the live `gh` check.
  final bool starClicked;

  /// Ask before unregistering a project and deleting its workspace metadata.
  final bool confirmProjectRemoval;

  /// Ask before removing a linked workspace and its Git worktree.
  final bool confirmWorkspaceRemoval;

  /// Prevent idle and display sleep while Alera is running.
  final bool keepAliveEnabled;

  /// Tray icon. Close hides; Quit still exits.
  final bool showTrayIcon;

  /// Pending-review Dock / taskbar badge.
  final bool showDockBadge;

  /// Pending-review count drawn onto the tray icon itself.
  final bool showTrayBadge;

  /// Show compact hosted pull-request and check state beside each workspace.
  final bool showPullRequestStatusInSidebar;

  /// Keep monitoring while hidden and notify when checks enter a failed state.
  final bool pullRequestFailureNotificationsEnabled;

  static const GeneralSettings defaults = GeneralSettings();

  factory fromJson(Map<String, Object?> json) =>
      GeneralSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

@MappableClass()
class const AgentSettings({
  this.agentStatusHooks = AgentStatusHookSettings.defaults,
  this.agentStatusNotificationsEnabled = false,
  this.agentStatusFinishedNotificationsEnabled = false,
  this.keepComputerAwakeWhileAgentsWork = false,
  this.showTabTitlesInSidebar = false,
  this.defaultAgentProfileId,
  this.quotas = AgentQuotaSettings.defaults,
}) with AgentSettingsMappable {
  /// Install managed agent hooks for terminal status. Each agent is
  /// default-off because enabling it writes into that agent's user config area.
  final AgentStatusHookSettings agentStatusHooks;

  /// Show native desktop notifications when an agent needs attention.
  final bool agentStatusNotificationsEnabled;

  /// Also notify when an agent reports it finished.
  ///
  /// Default-off: every supported agent reports the end of a turn rather than
  /// the end of a task, so a normal back-and-forth notifies on every reply.
  final bool agentStatusFinishedNotificationsEnabled;

  /// Keep the local computer awake while local hook-reported agents are working.
  final bool keepComputerAwakeWhileAgentsWork;

  /// Use each agent tab title in the workspace sidebar instead of latest activity.
  final bool showTabTitlesInSidebar;

  /// Runtime profile selected for flows that need an initial agent choice.
  final String? defaultAgentProfileId;

  /// Per-host quota providers, Claude CCS profiles, and environment names.
  final AgentQuotaSettings quotas;

  static const AgentSettings defaults = AgentSettings();

  factory fromJson(Map<String, Object?> json) =>
      AgentSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

/// Lifts the agent-related keys that historically lived under `general` into
/// the `agents` sub-map so settings blobs written before the `AgentSettings`
/// extraction keep decoding with their values intact.
class const _LegacyAgentSettingsHook() extends MappingHook {
  static const List<String> _legacyKeys = <String>[
    'agentStatusHooks',
    'agentStatusNotificationsEnabled',
    'keepComputerAwakeWhileAgentsWork',
  ];

  @override
  Object? beforeDecode(Object? value) {
    if (value is! Map) {
      return value;
    }
    if (value.containsKey('agents')) {
      return value;
    }
    final general = value['general'];
    if (general is! Map) {
      return value;
    }
    final agents = <String, dynamic>{
      for (final key in _legacyKeys)
        if (general.containsKey(key)) key: general[key],
    };
    if (agents.isEmpty) {
      return value;
    }
    return <String, dynamic>{
      for (final entry in value.entries) entry.key.toString(): entry.value,
      'agents': agents,
    };
  }
}

/// Log levels offered in Settings, kept as a closed set so the stored value
/// cannot drift into something `package:logging` will not accept.
@MappableEnum()
enum DiagnosticsLogLevel { error, warning, info, debug }

@MappableClass()
class const DiagnosticsSettings({
  this.logLevel = DiagnosticsLogLevel.info,
  this.crashReportingEnabled = false,
}) with DiagnosticsSettingsMappable {
  /// Detail written to the app and runtime log files.
  final DiagnosticsLogLevel logLevel;

  /// Send crashes to Sentry. Default-off because it leaves the machine; the
  /// local log file is what makes diagnosis possible without it.
  final bool crashReportingEnabled;

  static const DiagnosticsSettings defaults = DiagnosticsSettings();

  factory fromJson(Map<String, Object?> json) =>
      DiagnosticsSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

@MappableClass(hook: _LegacyAgentSettingsHook())
class const AleraSettings({
  required this.general,
  this.agents = AgentSettings.defaults,
  this.aiAssist = AiAssistSettings.defaults,
  this.aiDictation = AiDictationSettings.defaults,
  this.textActions = TextActionsSettings.defaults,
  this.editor = EditorSettings.defaults,
  this.diagnostics = DiagnosticsSettings.defaults,
  required this.terminal,
  required this.keyboard,
}) with AleraSettingsMappable {
  final GeneralSettings general;
  final AgentSettings agents;
  // Wire and persisted key stays `aiTextGeneration` so older hosts and settings keep working.
  @MappableField(key: 'aiTextGeneration')
  final AiAssistSettings aiAssist;
  final AiDictationSettings aiDictation;
  final TextActionsSettings textActions;
  final EditorSettings editor;
  final DiagnosticsSettings diagnostics;
  final TerminalSettings terminal;
  final KeyboardShortcutSettings keyboard;

  static const AleraSettings defaults = AleraSettings(
    general: .defaults,
    agents: .defaults,
    aiAssist: .defaults,
    aiDictation: .defaults,
    editor: .defaults,
    diagnostics: .defaults,
    terminal: .defaults,
    keyboard: .defaults,
  );

  factory fromJson(Map<String, Object?> json) =>
      AleraSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}
