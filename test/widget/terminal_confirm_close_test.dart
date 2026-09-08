import 'dart:typed_data';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
import 'package:alera/src/features/workbench/presentation/workbench_dialog_launchers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import '../unit/terminal_host_test_fakes.dart';

class _TestSettingsController extends SettingsController {
  _TestSettingsController(this._seed);
  final AleraSettings _seed;

  @override
  AleraSettings build() => _seed;
}

class _TestAgentStatusController extends AgentStatusController {
  _TestAgentStatusController(this._entries);
  final Map<String, AgentStatusEntry> _entries;

  @override
  Map<String, AgentStatusEntry> build() => _entries;
}

Future<void> _pumpHarness(
  WidgetTester tester, {
  required FakeTerminalHostClient client,
  required Future<void> Function(BuildContext context, WidgetRef ref) onAction,
  AleraSettings settings = AleraSettings.defaults,
  Map<String, AgentStatusEntry> agentStatuses =
      const <String, AgentStatusEntry>{},
}) async {
  await tester.pumpWidget(
    ProviderScope(
      overrides: [
        terminalHostClientProvider.overrideWithValue(client),
        settingsControllerProvider.overrideWith(
          () => _TestSettingsController(settings),
        ),
        agentStatusControllerProvider.overrideWith(
          () => _TestAgentStatusController(agentStatuses),
        ),
      ],
      child: MaterialApp(
        home: Scaffold(
          body: Center(
            child: Consumer(
              builder: (context, ref, _) {
                return FilledButton(
                  onPressed: () => onAction(context, ref),
                  child: const Text('Trigger Close'),
                );
              },
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pump();
}

void main() {
  group('terminal confirm close', () {
    late FakeTerminalHostClient client;
    final now = DateTime.utc(2026, 5, 25);
    final tab1 = WorkspaceTabRecord(
      id: 'session-1',
      workspaceId: 'ws-1',
      title: 'Terminal 1',
      createdAt: now,
      updatedAt: now,
    );
    final tab2 = WorkspaceTabRecord(
      id: 'session-2',
      workspaceId: 'ws-1',
      title: 'Terminal 2',
      createdAt: now,
      updatedAt: now,
    );

    setUp(() {
      client = FakeTerminalHostClient(
        attachment: TerminalHostAttachment(
          sessionId: 'session-1',
          created: true,
          running: true,
          snapshot: Uint8List(0),
        ),
      );
    });

    testWidgets('allows closing idle terminal without confirmation dialog', (
      tester,
    ) async {
      bool? result;
      await _pumpHarness(
        tester,
        client: client,
        onAction: (context, ref) async {
          result = await confirmCloseWorkspaceTabs(
            context,
            ref,
            <WorkspaceTabRecord>[tab1],
            <String>[tab1.id],
          );
        },
      );

      await tester.tap(find.text('Trigger Close'));
      await tester.pumpAndSettle();

      expect(result, isTrue);
      expect(find.text('Stop Running Command?'), findsNothing);
      expect(find.text('Stop Running Agent?'), findsNothing);
    });

    testWidgets(
      'confirms before closing terminal with active OS child process',
      (tester) async {
        client.runningProcesses['session-1'] = <String>['python.exe'];
        bool? result;
        await _pumpHarness(
          tester,
          client: client,
          onAction: (context, ref) async {
            result = await confirmCloseWorkspaceTabs(
              context,
              ref,
              <WorkspaceTabRecord>[tab1],
              <String>[tab1.id],
            );
          },
        );

        await tester.tap(find.text('Trigger Close'));
        await tester.pumpAndSettle();

        expect(find.text('Stop Running Command?'), findsOneWidget);
        expect(
          find.text(
            'The process "python.exe" is still running in "Terminal 1". Closing will terminate it.',
          ),
          findsOneWidget,
        );

        // Cancel
        await tester.tap(find.text('Cancel'));
        await tester.pumpAndSettle();
        expect(result, isFalse);

        // Trigger again and confirm
        await tester.tap(find.text('Trigger Close'));
        await tester.pumpAndSettle();
        await tester.tap(find.text('Stop And Close'));
        await tester.pumpAndSettle();
        expect(result, isTrue);
      },
    );

    testWidgets('confirms before closing terminal with active working agent', (
      tester,
    ) async {
      final agentStatus = AgentStatusEntry(
        terminalSessionId: 'session-1',
        workspaceId: 'ws-1',
        tabId: 'session-1',
        agentType: AgentType.codex,
        state: AgentStatusState.working,
        prompt: 'Building something',
        updatedAt: now,
        stateStartedAt: now,
      );

      bool? result;
      await _pumpHarness(
        tester,
        client: client,
        agentStatuses: {'session-1': agentStatus},
        onAction: (context, ref) async {
          result = await confirmCloseWorkspaceTabs(
            context,
            ref,
            <WorkspaceTabRecord>[tab1],
            <String>[tab1.id],
          );
        },
      );

      await tester.tap(find.text('Trigger Close'));
      await tester.pumpAndSettle();

      expect(find.text('Stop Running Agent?'), findsOneWidget);
      expect(
        find.text(
          'An agent is actively working in "Terminal 1". Closing will terminate the session.',
        ),
        findsOneWidget,
      );

      await tester.tap(find.text('Stop And Close'));
      await tester.pumpAndSettle();
      expect(result, isTrue);
    });

    testWidgets(
      'allows closing immediately when confirmCloseRunningProcesses is false',
      (tester) async {
        client.runningProcesses['session-1'] = <String>['node.exe'];
        bool? result;
        await _pumpHarness(
          tester,
          client: client,
          settings: AleraSettings.defaults.copyWith(
            terminal: AleraSettings.defaults.terminal.copyWith(
              confirmCloseRunningProcesses: false,
            ),
          ),
          onAction: (context, ref) async {
            result = await confirmCloseWorkspaceTabs(
              context,
              ref,
              <WorkspaceTabRecord>[tab1],
              <String>[tab1.id],
            );
          },
        );

        await tester.tap(find.text('Trigger Close'));
        await tester.pumpAndSettle();

        expect(result, isTrue);
        expect(find.text('Stop Running Command?'), findsNothing);
      },
    );

    testWidgets(
      'shows plural confirmation dialog when closing multiple busy terminals',
      (tester) async {
        client.runningProcesses['session-1'] = <String>['make'];
        client.runningProcesses['session-2'] = <String>['cargo'];
        bool? result;
        await _pumpHarness(
          tester,
          client: client,
          onAction: (context, ref) async {
            result = await confirmCloseWorkspaceTabs(
              context,
              ref,
              <WorkspaceTabRecord>[tab1, tab2],
              <String>[tab1.id, tab2.id],
            );
          },
        );

        await tester.tap(find.text('Trigger Close'));
        await tester.pumpAndSettle();

        expect(find.text('Close Busy Terminals?'), findsOneWidget);
        expect(
          find.text(
            '2 terminal tabs have running processes or active agents. Closing will terminate them.',
          ),
          findsOneWidget,
        );

        await tester.tap(find.text('Stop And Close'));
        await tester.pumpAndSettle();
        expect(result, isTrue);
      },
    );
  });
}
