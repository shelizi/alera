part of 'alera_settings.dart';

/// Mixed ownership: `tabSize`, `themeName`, `autosaveEnabled`, and
/// `autosaveDelaySeconds` are portable-cloud configuration; the external
/// editor and code-open fields are local-only UI prefs.
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
