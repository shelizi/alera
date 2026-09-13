part of 'alera_settings.dart';

/// Runtime operational settings: the enabled set is pushed to the runtime
/// host as `agentStatusHooks` in `runtimeSettings.update`.
@MappableClass(hook: _AgentStatusHookSettingsMappingHook())
class const AgentStatusHookSettings({this.values = const <String, bool>{}})
    with AgentStatusHookSettingsMappable {
  final Map<String, bool> values;

  bool isEnabled(String agentId) => values[agentId] ?? false;

  // Keep the old read API available to feature code that is migrated
  // separately. These getters are not mapped fields and do not affect the
  // flat wire representation.
  bool get codex => isEnabled('codex');
  bool get claude => isEnabled('claude');
  bool get copilot => isEnabled('copilot');
  bool get cursor => isEnabled('cursor');
  bool get agy => isEnabled('agy');
  bool get opencode => isEnabled('opencode');
  bool get opencode2 => isEnabled('opencode2');
  bool get pi => isEnabled('pi');
  bool get amp => isEnabled('amp');
  bool get grok => isEnabled('grok');
  bool get devin => isEnabled('devin');
  bool get fx => isEnabled('fx');

  bool get anyEnabled => values.values.any((enabled) => enabled);

  static const AgentStatusHookSettings defaults = AgentStatusHookSettings();

  factory fromJson(Map<String, Object?> json) =>
      AgentStatusHookSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

/// The settings model has one mapped value, but its wire representation is
/// the value map itself rather than a nested `values` object.
class const _AgentStatusHookSettingsMappingHook() extends MappingHook {
  @override
  Object? beforeDecode(Object? value) {
    if (value is! Map) {
      return value;
    }
    return <String, Object?>{'values': Map<String, dynamic>.from(value)};
  }

  @override
  Object? afterEncode(Object? value) {
    if (value is Map) {
      final values = value['values'];
      if (values is Map) {
        return values;
      }
    }
    return value;
  }
}
