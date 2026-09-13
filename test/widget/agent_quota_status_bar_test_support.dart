part of 'agent_quota_status_bar_test.dart';

Widget _wrap({
  required List<AgentQuotaSnapshot> snapshots,
  bool loading = false,
  AgentQuotaHostSettings settings = const AgentQuotaHostSettings(
    enabledProviders: <AgentQuotaProviderId>[
      AgentQuotaProviderId.claude,
      AgentQuotaProviderId.agy,
    ],
  ),
  AgentQuotaPinToggle? onTogglePinned,
}) {
  return MaterialApp(
    theme: buildAleraDarkTheme(),
    home: Scaffold(
      body: Align(
        alignment: Alignment.bottomCenter,
        child: AgentQuotaStatusBarView(
          hostId: 'local',
          snapshots: snapshots,
          settings: settings,
          loading: loading,
          onRefresh: () {},
          onTogglePinned: onTogglePinned ?? (_, _) {},
        ),
      ),
    ),
  );
}

AgentQuotaSnapshot _snapshot({
  required AgentQuotaProviderId provider,
  String accountId = 'default',
  String displayName = 'Default',
  AgentQuotaStatus status = AgentQuotaStatus.ok,
  String? error,
  List<AgentQuotaWindow> windows = const <AgentQuotaWindow>[],
  List<AgentQuotaBucket> buckets = const <AgentQuotaBucket>[],
}) {
  return AgentQuotaSnapshot(
    provider: provider,
    accountId: accountId,
    displayName: displayName,
    status: status,
    updatedAt: .utc(2026),
    error: error,
    windows: windows,
    buckets: buckets,
  );
}

AgentQuotaWindow _window(String label, double usedPercent) {
  return AgentQuotaWindow(
    label: label,
    usedPercent: usedPercent,
    windowMinutes: null,
    resetsAt: null,
    resetDescription: null,
  );
}

AgentQuotaBucket _bucket(
  String name,
  double usedPercent, {
  String? resetDescription,
}) {
  return AgentQuotaBucket(
    name: name,
    usedPercent: usedPercent,
    windowMinutes: null,
    resetsAt: null,
    resetDescription: resetDescription,
  );
}
