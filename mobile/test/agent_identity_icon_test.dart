import 'package:alera_mobile/src/features/workbench/presentation/agent_identity_icon.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('normalizes agent ids and aliases', () {
    expect(canonicalAgentId('antigravity'), 'agy');
    expect(canonicalAgentId('  CODEX  '), 'codex');
    expect(canonicalAgentId('unknown'), 'unknown');
  });

  testWidgets('renders local icon assets for every supported agent', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(
        home: Row(
          children: <Widget>[
            AgentIdentityIcon(agentType: 'codex'),
            AgentIdentityIcon(agentType: 'claude'),
            AgentIdentityIcon(agentType: 'copilot'),
            AgentIdentityIcon(agentType: 'cursor'),
            AgentIdentityIcon(agentType: 'agy'),
            AgentIdentityIcon(agentType: 'opencode'),
            AgentIdentityIcon(agentType: 'pi'),
            AgentIdentityIcon(agentType: 'amp'),
            AgentIdentityIcon(agentType: 'grok'),
            AgentIdentityIcon(agentType: 'fx'),
          ],
        ),
      ),
    );
    await tester.pump();

    expect(find.byType(SvgPicture), findsNWidgets(6));
    expect(find.byType(Image), findsNWidgets(4));
    expect(find.byTooltip('fx'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('resolves the Antigravity alias to its identity icon', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(home: AgentIdentityIcon(agentType: 'antigravity')),
    );
    await tester.pump();

    expect(find.byTooltip('Antigravity'), findsOneWidget);
    expect(find.byType(Image), findsOneWidget);
    expect(
      (tester.widget<Image>(find.byType(Image)).image as AssetImage).assetName,
      'assets/agents/agy.png',
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('uses the generic icon when Devin has no mobile asset', (
    tester,
  ) async {
    await tester.pumpWidget(
      const MaterialApp(home: AgentIdentityIcon(agentType: 'devin')),
    );
    await tester.pump();

    expect(agentDisplayName('devin'), 'Devin');
    expect(find.byTooltip('Devin'), findsOneWidget);
    expect(find.byIcon(Icons.smart_toy_outlined), findsOneWidget);
    expect(find.byType(Image), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets('uses the generic fallback for an unknown agent', (tester) async {
    await tester.pumpWidget(
      const MaterialApp(home: AgentIdentityIcon(agentType: 'unknown')),
    );
    await tester.pump();

    expect(agentDisplayName('unknown'), 'Agent');
    expect(find.byTooltip('Agent'), findsOneWidget);
    expect(find.byIcon(Icons.smart_toy_outlined), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
