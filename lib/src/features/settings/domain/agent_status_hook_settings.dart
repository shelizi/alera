part of 'alera_settings.dart';

/// Runtime operational settings: the enabled set is pushed to the runtime
/// host as `agentStatusHooks` in `runtimeSettings.update`.
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
