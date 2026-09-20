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
part 'agent_status_hook_settings.dart';
part 'alera_settings.mapper.dart';
part 'alera_terminal_settings.dart';
part 'diagnostics_settings.dart';
part 'editor_settings.dart';

@MappableEnum()
enum AppLanguage {
  system,
  english,
  traditionalChinese,
  simplifiedChinese,
  japanese,
}

/// Mixed ownership: `confirmProjectRemoval`, `confirmWorkspaceRemoval`, and
/// `autoArchiveWorkspacesAfterDays` are both portable-cloud configuration and
/// runtime operational settings; `showTrayIcon`, `showDockBadge`, and
/// `showTrayBadge` are portable only; `workspaceDirectory` is runtime
/// operational only; the rest are local-only UI prefs.
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
  this.autoArchiveWorkspacesAfterDays = 30,
}) with GeneralSettingsMappable {
  /// Language used by the Alera interface. System resolves Chinese locales to
  /// the matching Chinese script, Japanese locales to Japanese, and falls back
  /// to English for unsupported locales.
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

  /// Days of inactivity before a workspace moves to the Archived section.
  /// `0` disables automatic archiving.
  final int autoArchiveWorkspacesAfterDays;

  static const GeneralSettings defaults = GeneralSettings();

  factory fromJson(Map<String, Object?> json) =>
      GeneralSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

/// Mixed ownership: `defaultAgentProfileId` is both portable-cloud and
/// runtime operational; `agentStatusHooks` and `quotas` (local host) are
/// runtime operational; the notification and sidebar flags are portable;
/// `keepComputerAwakeWhileAgentsWork` is a local-only UI pref.
@MappableClass()
class const AgentSettings({
  this.agentStatusHooks = AgentStatusHookSettings.defaults,
  this.agentStatusNotificationsEnabled = false,
  this.agentStatusFinishedNotificationsEnabled = false,
  this.keepComputerAwakeWhileAgentsWork = false,
  this.showTabTitlesInSidebar = false,
  this.defaultAgentProfileId,
  this.agentExecutablePaths = const <String, String>{},
  this.gitBashExecutablePath,
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

  /// Per-device executable overrides for supported agent CLIs. Empty or
  /// missing entries use the descriptor's default command from PATH.
  final Map<String, String> agentExecutablePaths;

  /// Optional per-device Git Bash executable override used by the Windows
  /// Devin external-terminal fallback. `null` keeps automatic detection.
  final String? gitBashExecutablePath;

  String? executablePathFor(String agentKey) {
    final value = agentExecutablePaths[agentKey]?.trim();
    return value == null || value.isEmpty ? null : value;
  }

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
