part of 'alera_settings.dart';

@MappableEnum()
enum TerminalCursorShape { block, bar, underline }

@MappableEnum()
enum TerminalToolbarCorner {
  topLeft,
  topRight,
  bottomLeft,
  bottomRight;

  String get label => switch (this) {
    topLeft => 'Top Left',
    topRight => 'Top Right',
    bottomLeft => 'Bottom Left',
    bottomRight => 'Bottom Right',
  };
}

final _hexColorPattern = RegExp(r'^#?[0-9a-fA-F]{6}$');

String? normalizeTerminalHexColor(Object? value) {
  if (value is! String) {
    return null;
  }
  final trimmed = value.trim();
  if (trimmed.isEmpty || !_hexColorPattern.hasMatch(trimmed)) {
    return null;
  }
  final hex = trimmed.startsWith('#') ? trimmed.substring(1) : trimmed;
  return '#${hex.toLowerCase()}';
}

@MappableClass()
class const TerminalColorOverrides({
  this.foreground,
  this.background,
  this.cursor,
  this.selection,
}) with TerminalColorOverridesMappable {
  final String? foreground;
  final String? background;
  final String? cursor;
  final String? selection;

  bool get isEmpty =>
      foreground == null &&
      background == null &&
      cursor == null &&
      selection == null;

  factory fromJson(Map<String, Object?> json) =>
      TerminalColorOverridesMapper.fromMap(Map<String, dynamic>.from(json));
}

/// Mixed ownership: the appearance fields are portable-cloud configuration;
/// the `host*` fields, `bufferBudgetMegabytes`, `keepRuntimeOpenOnAppQuit`,
/// and `loginShell` are runtime operational settings delivered through the
/// runtime-host `configure` request; `scrollbackLines` and
/// `confirmCloseRunningProcesses` are local-only UI prefs.
@MappableClass(hook: _LegacyKeepRuntimeOpenHook())
class const TerminalSettings({
  required this.fontFamily,
  required this.fontSize,
  this.fontWeight = 400,
  required this.lineHeight,
  this.paddingX = AleraTokens.space12,
  this.paddingY = AleraTokens.space12,
  required this.cursorShape,
  this.cursorBlink = false,
  this.cursorOpacity = 1,
  this.themeName = TerminalThemeNames.aleraDark,
  this.backgroundOpacity = 1,
  this.wordSeparators,
  this.colorOverrides = const TerminalColorOverrides(),
  required this.scrollbackLines,
  this.tuiScrollSensitivity = 1,
  this.clipboardOnSelect = false,
  this.allowOsc52Clipboard = false,
  this.showComposerByDefault = false,
  this.toolbarCorner = TerminalToolbarCorner.topRight,
  this.hostEmptyShutdownDelaySeconds = 30,
  this.hostDetachedSessionShutdownDelaySeconds = 60 * 60,
  this.hostScrollbackBytes = 10 * 1000 * 1000,
  this.bufferBudgetMegabytes = 256,
  this.keepRuntimeOpenOnAppQuit = false,
  this.loginShell,
  this.confirmCloseRunningProcesses = true,
}) with TerminalSettingsMappable {
  final String fontFamily;
  final double fontSize;
  final int fontWeight;
  final double lineHeight;
  final double paddingX;
  final double paddingY;
  final TerminalCursorShape cursorShape;
  final bool cursorBlink;
  final double cursorOpacity;
  final String themeName;
  final double backgroundOpacity;
  final String? wordSeparators;
  final TerminalColorOverrides colorOverrides;
  final int scrollbackLines;
  final int tuiScrollSensitivity;
  final bool clipboardOnSelect;
  final bool allowOsc52Clipboard;

  /// Whether new terminal sessions open the prompt composer immediately.
  final bool showComposerByDefault;

  /// Where the terminal action-button cluster sits inside the tab.
  final TerminalToolbarCorner toolbarCorner;
  final int hostEmptyShutdownDelaySeconds;
  final int hostDetachedSessionShutdownDelaySeconds;
  final int hostScrollbackBytes;

  /// Terminal buffer ceiling; 0 is unbounded. See `TerminalBufferBudget`.
  final int bufferBudgetMegabytes;
  final bool keepRuntimeOpenOnAppQuit;

  /// Whether closing terminal tabs with active processes or agents requires confirmation.
  final bool confirmCloseRunningProcesses;

  /// `null` keeps the platform default resolved by [resolvedLoginShell].
  final bool? loginShell;

  /// Whether terminals start the user shell as a login shell.
  ///
  /// macOS GUI apps inherit a minimal `launchd` PATH and never read
  /// `~/.zprofile`, where Homebrew and similar prefixes are set up, so login
  /// shells are the default there. Other platforms keep the plain interactive
  /// shell their terminal emulators use.
  bool get resolvedLoginShell => loginShell ?? defaultLoginShell;

  static bool get defaultLoginShell =>
      !kIsWeb && defaultTargetPlatform == TargetPlatform.macOS;

  double get padding =>
      paddingX == paddingY ? paddingX : (paddingX + paddingY) / 2;

  static const TerminalSettings defaults = TerminalSettings(
    fontFamily: 'JetBrains Mono',
    fontSize: 13,
    fontWeight: 400,
    lineHeight: 1.3,
    paddingX: AleraTokens.space12,
    paddingY: AleraTokens.space12,
    cursorShape: .block,
    cursorBlink: false,
    cursorOpacity: 1,
    themeName: TerminalThemeNames.aleraDark,
    backgroundOpacity: 1,
    scrollbackLines: 10000,
    tuiScrollSensitivity: 1,
    clipboardOnSelect: false,
    allowOsc52Clipboard: false,
    showComposerByDefault: false,
    toolbarCorner: .topRight,
    hostEmptyShutdownDelaySeconds: 30,
    hostDetachedSessionShutdownDelaySeconds: 60 * 60,
    hostScrollbackBytes: 10 * 1000 * 1000,
    keepRuntimeOpenOnAppQuit: false,
    confirmCloseRunningProcesses: true,
  );

  factory fromJson(Map<String, Object?> json) =>
      TerminalSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

/// Maps the inverted legacy `stopRuntimeOnAppQuit` flag to
/// `keepRuntimeOpenOnAppQuit` so older settings blobs keep their intent.
///
/// Blobs that never stored either flag used the old implicit default (leave
/// the runtime open on quit), so they migrate to `keepRuntimeOpenOnAppQuit:
/// true`. Fresh installs use [TerminalSettings.defaults] without decoding.
class const _LegacyKeepRuntimeOpenHook() extends MappingHook {
  @override
  Object? beforeDecode(Object? value) {
    if (value is! Map) {
      return value;
    }
    if (value.containsKey('keepRuntimeOpenOnAppQuit')) {
      return value;
    }
    if (value.containsKey('stopRuntimeOnAppQuit')) {
      return <String, dynamic>{
        for (final entry in value.entries)
          if (entry.key.toString() != 'stopRuntimeOnAppQuit')
            entry.key.toString(): entry.value,
        'keepRuntimeOpenOnAppQuit': value['stopRuntimeOnAppQuit'] != true,
      };
    }
    return <String, dynamic>{
      for (final entry in value.entries) entry.key.toString(): entry.value,
      'keepRuntimeOpenOnAppQuit': true,
    };
  }
}
