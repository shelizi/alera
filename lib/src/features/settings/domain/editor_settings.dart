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

/// Mixed ownership: `tabSize`, `themeName`, `autosaveEnabled`,
/// `autosaveDelaySeconds`, and `quickOpenExcludedDirectories` are
/// portable-cloud configuration; the external editor and code-open fields are
/// local-only UI prefs. `languageIntelligence` is also local-only because it
/// may contain per-device executable paths.
@MappableClass(hook: _LegacyEditorSettingsHook())
class const EditorSettings({
  this.tabSize = 4,
  this.themeName = EditorSyntaxThemeNames.alera,
  this.autosaveEnabled = false,
  this.autosaveDelaySeconds = defaultAutosaveDelaySeconds,
  this.quickOpenExcludedDirectories = defaultQuickOpenExcludedDirectories,
  this.externalEditor = ExternalEditorKind.zed,
  this.codeOpenTarget = CodeOpenTarget.alera,
  this.externalEditorExecutablePaths = const <String, String>{},
  this.externalEditorWorkspaceMode = ExternalEditorWorkspaceMode.newWindow,
  this.autoOpenNewWorkspacesExternally = false,
  this.languageIntelligence = LanguageIntelligenceSettings.defaults,
}) with EditorSettingsMappable {
  static const int minAutosaveDelaySeconds = 1;
  static const int maxAutosaveDelaySeconds = 60;
  static const int defaultAutosaveDelaySeconds = 1;
  static const List<String> defaultQuickOpenExcludedDirectories = <String>[
    'node_modules',
    '.dart_tool',
    'vendor',
    'vendor-bin',
    '.phpunit.cache',
    'coverage',
    'Pods',
    '.gradle',
    '.venv',
    'venv',
    '.tox',
    '__pypackages__',
    'bower_components',
    'jspm_packages',
    '.pnpm-store',
    '.pub-cache',
    'target',
    'bin',
    'obj',
    '.vs',
    'packages',
    'TestResults',
    'BenchmarkDotNet.Artifacts',
    'artifacts',
  ];

  /// Number of spaces inserted when the editor handles a Tab key press.
  final int tabSize;

  /// Syntax highlighting theme used by editor tabs.
  final String themeName;

  /// Save dirty editor tabs after they have been idle for the configured delay.
  final bool autosaveEnabled;

  /// Number of idle seconds before an automatic editor save.
  final int autosaveDelaySeconds;

  /// Directory names that Quick Open never indexes, even when Git-ignored
  /// files are included. Matching is case-insensitive in the native indexer.
  final List<String> quickOpenExcludedDirectories;

  final ExternalEditorKind externalEditor;
  final CodeOpenTarget codeOpenTarget;

  /// Custom executable overrides keyed by [ExternalEditorKind] name. An
  /// absent entry means the editor resolves from the command environment.
  final Map<String, String> externalEditorExecutablePaths;

  final ExternalEditorWorkspaceMode externalEditorWorkspaceMode;
  final bool autoOpenNewWorkspacesExternally;

  /// Per-language parser/semantic activation and local executable overrides.
  /// Semantic providers are opt-in. Structural parser defaults come from the
  /// registered language descriptor and remain independently configurable.
  final LanguageIntelligenceSettings languageIntelligence;

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
