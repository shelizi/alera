part of 'alera_settings.dart';

/// Log levels offered in Settings, kept as a closed set so the stored value
/// cannot drift into something `package:logging` will not accept.
@MappableEnum()
enum DiagnosticsLogLevel { error, warning, info, debug }

/// Local-only UI prefs: diagnostics never leave the machine through the
/// runtime-host or portable sync channels.
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
