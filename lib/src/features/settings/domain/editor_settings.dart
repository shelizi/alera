part of 'alera_settings.dart';

/// Lifts persisted keys written before the external editor abstraction into
/// the current shape: the Zed-only executable pair becomes the per-kind
/// `externalEditorExecutablePaths` map, `codeOpenTarget: zed` becomes
/// `external`, and the auto-open flag drops its Zed suffix.
class const _LegacyEditorSettingsHook() extends MappingHook {
  @override
  Object? beforeDecode(Object? value) {
    if (value is! Map) {
      return value;
    }
    final map = <String, dynamic>{
      for (final entry in value.entries) entry.key.toString(): entry.value,
    };
    if (map['codeOpenTarget'] == 'zed') {
      map['codeOpenTarget'] = 'external';
    }
    final legacyPath = map.remove('zedExecutablePath');
    final legacyMode = map.remove('zedExecutableMode');
    if (legacyMode == 'custom' &&
        legacyPath is String &&
        legacyPath.trim().isNotEmpty) {
      map.putIfAbsent(
        'externalEditorExecutablePaths',
        () => <String, dynamic>{'zed': legacyPath},
      );
    }
    final legacyAutoOpen = map.remove('autoOpenNewWorkspacesInZed');
    if (legacyAutoOpen != null) {
      map.putIfAbsent('autoOpenNewWorkspacesExternally', () => legacyAutoOpen);
    }
    return map;
  }
}

/// Mixed ownership: `tabSize`, `themeName`, `autosaveEnabled`, and
/// `autosaveDelaySeconds` are portable-cloud configuration; the external
/// editor and code-open fields are local-only UI prefs.
@MappableClass(hook: _LegacyEditorSettingsHook())
class const EditorSettings({
  this.tabSize = 4,
  this.themeName = EditorSyntaxThemeNames.alera,
  this.autosaveEnabled = false,
  this.autosaveDelaySeconds = defaultAutosaveDelaySeconds,
  this.externalEditor = ExternalEditorKind.zed,
  this.codeOpenTarget = CodeOpenTarget.alera,
  this.externalEditorExecutablePaths = const <String, String>{},
  this.externalEditorWorkspaceMode = ExternalEditorWorkspaceMode.newWindow,
  this.autoOpenNewWorkspacesExternally = false,
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

  /// Custom executable overrides keyed by [ExternalEditorKind] name. An
  /// absent entry means the editor resolves from the command environment.
  final Map<String, String> externalEditorExecutablePaths;

  final ExternalEditorWorkspaceMode externalEditorWorkspaceMode;
  final bool autoOpenNewWorkspacesExternally;

  /// Clamps persisted values before they are used to construct a timer.
  int get effectiveAutosaveDelaySeconds => autosaveDelaySeconds
      .clamp(minAutosaveDelaySeconds, maxAutosaveDelaySeconds)
      .toInt();

  Duration get autosaveDebounce =>
      Duration(seconds: effectiveAutosaveDelaySeconds);

  /// The configured executable override for [kind], if any.
  String? executablePathFor(ExternalEditorKind kind) =>
      externalEditorExecutablePaths[kind.name];

  static const EditorSettings defaults = EditorSettings();

  factory fromJson(Map<String, Object?> json) =>
      EditorSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}
