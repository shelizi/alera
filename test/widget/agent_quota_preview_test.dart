import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/agent_quota/domain/agent_quota.dart';
import 'package:alera/src/features/agent_quota/presentation/agent_quota_provider_icon.dart';
import 'package:alera/src/features/agent_quota/presentation/agent_quota_status_bar_content.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('Claude native cost and usage show screenshot-style progress', (tester) async {
    await tester.pumpWidget(MaterialApp(
      theme: buildAleraDarkTheme(),
      home: Scaffold(body: AgentQuotaStatusBarContent(
        hostId: 'local',
        settings: const AgentQuotaHostSettings(
          enabledProviders: [AgentQuotaProviderId.claude],
        ),
        snapshots: [
          AgentQuotaSnapshot(
            provider: .claude,
            accountId: 'default',
            displayName: 'Default',
            status: .ok,
            updatedAt: DateTime.utc(2026),
            error: null,
            windows: const [
              AgentQuotaWindow(label: '5 Hour', usedPercent: 1,
                windowMinutes: 300, resetsAt: null, resetDescription: 'Resets in 4h 35m'),
            ],
            buckets: const [],
            amounts: const [
              AgentQuotaAmount(
                label: 'Usage credits', currency: 'USD',
                spentAmount: 2.83, remainingAmount: null, limitAmount: 50.0,
                resetsAt: null, resetDescription: null,
              ),
            ],
          ),
        ],
        onRefresh: () {},
        onTogglePinned: (_, _) {},
      )),
    ));
    final mouse = await tester.createGesture(kind: .mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: .zero);
    await mouse.moveTo(tester.getCenter(
      find.byWidgetPredicate((widget) => widget is AgentQuotaProviderIcon &&
        widget.provider == AgentQuotaProviderId.claude),
    ));
    await tester.pump(AleraTokens.durationMid);
    await tester.pumpAndSettle();
    expect(find.text('1% used'), findsOneWidget);
    expect(find.text('Resets in 4h 35m'), findsOneWidget);
    expect(find.text(r'US$2.83 / US$50.00'), findsOneWidget);
    expect(find.text('6% used'), findsOneWidget);
  });

  testWidgets('quota previews open overlays without native providers', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: buildAleraDarkTheme(),
        home: Scaffold(
          body: AgentQuotaStatusBarContent(
            hostId: 'local',
            settings: const AgentQuotaHostSettings(
              enabledProviders: [
                AgentQuotaProviderId.claude,
                AgentQuotaProviderId.codex,
              ],
            ),
            snapshots: [
              for (final provider in [
                AgentQuotaProviderId.claude,
                AgentQuotaProviderId.codex,
              ])
                AgentQuotaSnapshot(
                  provider: provider,
                  accountId: 'default',
                  displayName: 'Default',
                  status: .unavailable,
                  updatedAt: .utc(2026),
                  error: 'Preview account is offline',
                  windows: const [],
                  buckets: const [],
                  rateLimitResetCredits: const CodexResetCredits(
                    availableCount: 2,
                    totalEarnedCount: 2,
                    nextExpiresAt: null,
                    offerRevision: 'preview',
                    canConsume: false,
                  ),
                ),
            ],
            onRefresh: () {},
            onTogglePinned: (_, _) {},
          ),
        ),
      ),
    );

    final mouse = await tester.createGesture(kind: .mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: .zero);
    expect(find.text('Default'), findsNothing);
    await mouse.moveTo(
      tester.getCenter(
        find.byWidgetPredicate(
          (widget) =>
              widget is AgentQuotaProviderIcon &&
              widget.provider == AgentQuotaProviderId.claude,
        ),
      ),
    );
    await tester.pump(AleraTokens.durationMid);
    await tester.pumpAndSettle();
    expect(find.text('Try With TUI'), findsNothing);
    expect(tester.takeException(), isNull);

    await mouse.moveTo(const Offset(0, 600));
    await tester.pumpAndSettle();
    await tester.tap(find.byIcon(AleraIcons.quota));
    await tester.pumpAndSettle();
    expect(find.text('Codex'), findsOneWidget);
    expect(find.text('Use Reset'), findsNothing);
    expect(tester.takeException(), isNull);
  });
}
