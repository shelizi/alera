part of 'alera_settings.dart';

// Ownership: the per-host quota fields the runtime host needs are pushed as
// `agentQuotas` in `runtimeSettings.update` (runtime operational), while
// `selectedClaudeProfile` and `unpinnedQuotaKeys` stay local-only UI prefs and
// are merged back from the local repository on every load.

/// Canonicalizes quota provider ids until the agent descriptor registry is
/// available through FRB on the Dart side.
String? canonicalAgentQuotaProviderId(Object? value) {
  if (value is! String) return null;
  return value == 'antigravity' ? 'agy' : value;
}

class _CanonicalAgentQuotaProviderListHook extends MappingHook {
  const _CanonicalAgentQuotaProviderListHook();

  @override
  Object? beforeDecode(Object? value) {
    if (value is! List) return value;
    return value
        .map((entry) => canonicalAgentQuotaProviderId(entry) ?? entry)
        .toList();
  }

  @override
  Object? afterEncode(Object? value) {
    if (value is! List) return value;
    return value
        .map((entry) => canonicalAgentQuotaProviderId(entry) ?? entry)
        .toList();
  }
}

@MappableEnum()
enum AgentQuotaProviderId {
  claude,
  codex,
  kimi,
  grok,
  cursor,
  @MappableValue('agy')
  agy,
  minimax,
  zai,
  devin,
  opencode;

  // Source compatibility for settings presentation code that is migrated in
  // a later batch. This is not an enum value and is absent from `values`.
  static const AgentQuotaProviderId antigravity = AgentQuotaProviderId.agy;
}

extension AgentQuotaProviderIdLabel on AgentQuotaProviderId {
  String get label => switch (this) {
    AgentQuotaProviderId.claude => 'Claude Code',
    AgentQuotaProviderId.codex => 'Codex',
    AgentQuotaProviderId.kimi => 'Kimi',
    AgentQuotaProviderId.grok => 'Grok Build',
    AgentQuotaProviderId.cursor => 'Cursor',
    AgentQuotaProviderId.agy => 'Antigravity',
    AgentQuotaProviderId.minimax => 'MiniMax',
    AgentQuotaProviderId.zai => 'Z.ai',
    AgentQuotaProviderId.devin => 'Devin',
    AgentQuotaProviderId.opencode => 'OpenCode',
  };
}

@MappableClass()
class const ClaudeQuotaProfileSettings({
  required this.alias,
  required this.profile,
  this.showInUsage = true,
  this.usageDisplayName,
}) with ClaudeQuotaProfileSettingsMappable {
  final String alias;
  final String profile;
  final bool showInUsage;
  final String? usageDisplayName;

  String get usageLabel {
    final configured = usageDisplayName?.trim();
    return configured == null || configured.isEmpty ? alias : configured;
  }

  factory fromJson(Map<String, Object?> json) =>
      ClaudeQuotaProfileSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

@MappableClass()
class const AgentQuotaEnvironmentSettings({
  this.kimiApiKey = 'KIMI_API_KEY',
  this.zaiApiKey = 'ZAI_API_KEY',
  this.zaiBaseUrl = 'ZAI_BASE_URL',
  this.minimaxApiKey = 'MINIMAX_API_KEY',
  this.minimaxApiHost = 'MINIMAX_API_HOST',
}) with AgentQuotaEnvironmentSettingsMappable {
  final String kimiApiKey;
  final String zaiApiKey;
  final String zaiBaseUrl;
  final String minimaxApiKey;
  final String minimaxApiHost;

  static const AgentQuotaEnvironmentSettings defaults =
      AgentQuotaEnvironmentSettings();

  factory fromJson(Map<String, Object?> json) =>
      AgentQuotaEnvironmentSettingsMapper.fromMap(
        Map<String, dynamic>.from(json),
      );
}

@MappableClass()
class const AgentQuotaHostSettings({
  this.enabledProviders = AgentQuotaProviderId.values,
  this.providerDefaultsVersion = 2,
  this.claudeDefaultEnabled = true,
  this.claudeDefaultShowInUsage = true,
  this.claudeProfiles = const <ClaudeQuotaProfileSettings>[],
  this.selectedClaudeProfile = 'default',
  this.environment = AgentQuotaEnvironmentSettings.defaults,
  this.unpinnedQuotaKeys = const <String>[],
}) with AgentQuotaHostSettingsMappable {
  @MappableField(hook: _CanonicalAgentQuotaProviderListHook())
  final List<AgentQuotaProviderId> enabledProviders;
  final int providerDefaultsVersion;
  final bool claudeDefaultEnabled;
  final bool claudeDefaultShowInUsage;
  final List<ClaudeQuotaProfileSettings> claudeProfiles;
  final String selectedClaudeProfile;
  final AgentQuotaEnvironmentSettings environment;

  /// Quotas hidden from the status bar (still visible in the overview panel).
  /// Absence means pinned, so older settings blobs keep today's behavior.
  @MappableField(hook: _CanonicalAgentQuotaProviderListHook())
  final List<String> unpinnedQuotaKeys;

  static const AgentQuotaHostSettings defaults = AgentQuotaHostSettings();

  /// Stable pin key: provider name for single-account providers, and
  /// `provider:<accountId>` for providers with multiple accounts.
  static String quotaPinKey(
    AgentQuotaProviderId provider, {
    String claudeAccountId = 'default',
  }) {
    final providerId = canonicalAgentQuotaProviderId(provider.name)!;
    if (provider == AgentQuotaProviderId.claude ||
        provider == AgentQuotaProviderId.opencode) {
      return '$providerId:$claudeAccountId';
    }
    return providerId;
  }

  bool isQuotaPinned(
    AgentQuotaProviderId provider, {
    String claudeAccountId = 'default',
  }) {
    final pinKey = quotaPinKey(provider, claudeAccountId: claudeAccountId);
    return !unpinnedQuotaKeys.any(
      (key) => canonicalAgentQuotaProviderId(key) == pinKey,
    );
  }

  factory fromJson(Map<String, Object?> json) =>
      AgentQuotaHostSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}

@MappableClass()
class const AgentQuotaSettings({
  this.hosts = const <String, AgentQuotaHostSettings>{},
}) with AgentQuotaSettingsMappable {
  final Map<String, AgentQuotaHostSettings> hosts;

  static const AgentQuotaSettings defaults = AgentQuotaSettings();

  AgentQuotaHostSettings forHost(String hostId) =>
      hosts[hostId] ?? AgentQuotaHostSettings.defaults;

  AgentQuotaSettings withHost(String hostId, AgentQuotaHostSettings settings) {
    return copyWith(
      hosts: <String, AgentQuotaHostSettings>{...hosts, hostId: settings},
    );
  }

  factory fromJson(Map<String, Object?> json) =>
      AgentQuotaSettingsMapper.fromMap(Map<String, dynamic>.from(json));
}
