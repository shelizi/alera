import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/agent_context_recovery.dart';
import 'package:alera/src/features/workbench/presentation/workspace_agent_context_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final now = DateTime.utc(2026, 11, 24);
  final live = AgentStatusEntry(
    terminalSessionId: 'terminal',
    workspaceId: 'workspace',
    tabId: 'tab',
    agentType: AgentType.codex,
    state: AgentStatusState.done,
    prompt: 'Fix reports',
    updatedAt: now,
    stateStartedAt: now,
    lastAssistantMessage: 'Native raw report without AI summary',
    contextOverview: AgentContextOverview.observe(
      previous: const AgentContextOverview(),
      previousPrompt: null,
      previousAssistant: null,
      prompt: 'Fix reports',
      assistant: 'Native raw report without AI summary',
      at: now,
    ).toJson(),
  );
  Future<void> pump(WidgetTester tester, AgentContextReportView view) async {
    await tester.pumpWidget(MaterialApp(home: Scaffold(body: view)));
    await tester.pumpAndSettle();
  }

  testWidgets(
    'right Agent Context shows original report and settings reminder without summary',
    (tester) async {
      await pump(
        tester,
        AgentContextReportView(sessionId: 'terminal', live: live),
      );
      expect(find.text('最新工作回報（原始回覆）'), findsOneWidget);
      expect(
        find.text('Native raw report without AI summary'),
        findsNWidgets(2),
      );
      expect(find.text('Agent 工作回報（原始回覆）'), findsOneWidget);
      expect(
        find.textContaining('Auto-Summarize Agent Work Reports'),
        findsOneWidget,
      );
    },
  );
  testWidgets(
    'restored tab with no live presence shows saved original report',
    (tester) async {
      final saved = AgentContextRecovery(
        AgentContextOverview.fromRaw(live.contextOverview),
      );
      await pump(
        tester,
        AgentContextReportView(
          sessionId: 'terminal',
          live: null,
          recovery: saved,
        ),
      );
      expect(find.text('最新工作回報（原始回覆）'), findsOneWidget);
      expect(
        find.text('Native raw report without AI summary'),
        findsNWidgets(2),
      );
      expect(find.text('Fix reports'), findsNWidgets(2));
    },
  );
  testWidgets(
    'verified history recovers the visible report, live report wins',
    (tester) async {
      final restored = contextFromNativeMessages([
        {
          'role': 'user',
          'text': 'Earlier goal',
          'timestamp': now.toIso8601String(),
        },
        {
          'role': 'assistant',
          'text': 'Earlier native response',
          'timestamp': now.toIso8601String(),
        },
        {'role': 'tool', 'text': 'DO NOT SHOW TOOL CONTENT'},
      ]);
      final recovered = AgentContextRecovery(restored, fromHistory: true);
      await pump(
        tester,
        AgentContextReportView(
          sessionId: 'terminal',
          live: null,
          recovery: recovered,
        ),
      );
      expect(find.text('最新工作回報（原始回覆）'), findsOneWidget);
      expect(find.text('Earlier native response'), findsNWidgets(2));
      expect(find.text('DO NOT SHOW TOOL CONTENT'), findsNothing);
      await pump(
        tester,
        AgentContextReportView(
          sessionId: 'terminal',
          live: live,
          recovery: recovered,
        ),
      );
      expect(find.text('最新工作回報（原始回覆）'), findsOneWidget);
      expect(
        find.text('Native raw report without AI summary'),
        findsNWidgets(2),
      );
    },
  );
  testWidgets('missing raw reply is distinguished from selecting a non-Agent', (
    tester,
  ) async {
    await pump(
      tester,
      const AgentContextReportView(sessionId: 'terminal', live: null),
    );
    expect(find.textContaining('尚未取得工作回報'), findsOneWidget);
  });
  testWidgets(
    'selected Agent Tab reads exact saved report and refreshes after SessionEnd',
    (tester) async {
      final workspace = Workspace(
        id: 'workspace',
        projectId: 'project',
        name: 'Project',
        path: 'C:/fixture',
        createdAt: now,
        updatedAt: now,
        kind: WorkspaceKind.main,
        status: WorkspaceStatus.active,
      );
      final a = WorkspaceTabRecord(
        id: 'tab',
        workspaceId: workspace.id,
        title: 'Codex A',
        createdAt: now,
        updatedAt: now,
        payload: {
          'agentType': 'codex',
          'terminalSessionId': 'terminal',
          'agentTitleConversationId': 'conversation-a',
        },
      );
      final b = WorkspaceTabRecord(
        id: 'tab-b',
        workspaceId: workspace.id,
        title: 'Claude B',
        createdAt: now,
        updatedAt: now,
        payload: {
          'agentType': 'claude',
          'terminalSessionId': 'terminal-b',
          'agentTitleConversationId': 'conversation-b',
        },
      );
      final workbench = _SelectedTabController(
        WorkbenchState(
          workspacesByProject: {
            'project': [workspace],
          },
          tabsByWorkspace: {
            'workspace': [a, b],
          },
          activeProjectId: 'project',
          activeWorkspaceId: workspace.id,
          activeTabIdByWorkspace: {'workspace': a.id},
        ),
      );
      final status = _PresenceController();
      final host = _ContextHost({
        'tab': AgentContextOverview.fromRaw({
          'goal': 'Task A',
          'progress': 'Raw checkpoint A',
          'reportSummary': 'Saved A',
        }),
        'tab-b': AgentContextOverview.fromRaw({
          'goal': 'Task B',
          'progress': 'Raw checkpoint B',
          'reportSummary': 'Saved B',
        }),
      });
      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            workbenchControllerProvider.overrideWith(() => workbench),
            agentStatusControllerProvider.overrideWith(() => status),
            runtimeHostClientProvider.overrideWithValue(host),
          ],
          child: MaterialApp(
            home: Scaffold(
              body: WorkspaceAgentContextPanel(workspace: workspace),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Saved A'), findsOneWidget);
      expect(find.text('Saved B'), findsNothing);
      expect(host.requests.first.$2['conversationId'], 'conversation-a');
      expect(host.requests.first.$2['sessionId'], 'terminal');
      workbench.select(b.id);
      await tester.pumpAndSettle();
      expect(find.text('Saved B'), findsOneWidget);
      expect(find.text('Saved A'), findsNothing);
      workbench.select(a.id);
      await tester.pumpAndSettle();
      status.replace({'terminal': live});
      await tester.pumpAndSettle();
      expect(
        find.text('Native raw report without AI summary'),
        findsNWidgets(2),
      );
      // Simulate fresh Host JSON for each update, not just the same Map
      // reference. No visible Context field changed, so the complete report
      // view and its scrollable retain their original widgets.
      final reportFinder = find.byType(AgentContextReportView);
      final listFinder = find.byKey(
        const ValueKey<String>('agent-context-terminal'),
      );
      final originalReport = tester.widget(reportFinder);
      final originalList = tester.widget(listFinder);
      final requestCount = host.requests.length;
      for (var i = 0; i < 100; i++) {
        status.replace({
          'terminal': live.copyWith(
            state: AgentStatusState.working,
            toolName: 'Tool $i',
            toolInput: 'Private input $i',
            updatedAt: now.add(Duration(seconds: i + 1)),
            lastAssistantMessage: 'Intermediate output $i',
            contextOverview: AgentContextOverview.fromRaw(live.contextOverview)
                .toJson(),
          ),
        });
        await tester.pump();
        expect(tester.widget(reportFinder), same(originalReport));
        expect(tester.widget(listFinder), same(originalList));
      }
      expect(host.requests.length, requestCount);
      // A new real report must replace the old projection.
      status.replace({
        'terminal': live.copyWith(
          contextOverview: AgentContextOverview.fromRaw({
            'goal': 'Fix reports',
            'progress': 'New real report',
            'reportSummary': 'New summary',
            'timeline': [
              {
                'kind': 'report',
                'text': 'New real report',
                'at': now.toIso8601String(),
              },
            ],
          }).toJson(),
          lastAssistantMessage: 'New real report',
        ),
      });
      await tester.pumpAndSettle();
      expect(find.text('New summary'), findsNWidgets(2));
      expect(find.text('New real report'), findsNothing);
      expect(tester.widget(reportFinder), isNot(same(originalReport)));
      expect(find.text('Native raw report without AI summary'), findsNothing);

      host.saved[a.id] = AgentContextOverview.fromRaw({
        'goal': 'Task A',
        'progress': 'Raw latest persisted A',
        'reportSummary': 'Latest persisted A',
      });
      status.replace({});
      await tester.pumpAndSettle();
      expect(find.text('Latest persisted A'), findsOneWidget);
      expect(find.text('Saved A'), findsNothing);
      expect(find.text('Saved B'), findsNothing);

      // Same Tab/PTY, different native conversation activation. The
      // recovery key must include the activation, and a lingering live
      // report from A must not leak into C.
      final oldLive = live.copyWith(
        contextOverview: {
          ...live.contextOverview!,
          'conversationId': 'conversation-a',
        },
      );
      status.replace({'terminal': oldLive});
      await tester.pumpAndSettle();
      expect(
        find.text('Native raw report without AI summary'),
        findsNWidgets(2),
      );
      host.saved[a.id] = AgentContextOverview.fromRaw({
        'goal': 'Task A second conversation',
        'progress': 'Raw report for conversation C',
        'reportSummary': 'Report for conversation C',
      });
      workbench.switchConversation(a.id, 'conversation-c');
      await tester.pumpAndSettle();
      expect(find.text('Report for conversation C'), findsOneWidget);
      expect(find.text('Latest persisted A'), findsNothing);
      expect(find.text('Native raw report without AI summary'), findsNothing);
      expect(host.requests.last.$2['conversationId'], 'conversation-c');
      expect(host.requests.last.$2['sessionId'], 'terminal');
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'overall and historical summary actions select exact timestamps',
    (tester) async {
      const latest = '2026-11-24T12:01:00.987654321+00:00';
      const earlier = '2026-11-24T12:00:00.123456789+00:00';
      final requested = <String?>[];
      final entry = live.copyWith(
        contextOverview: {
          'goal': 'Original goal',
          'progress': 'Latest raw reply',
          'reportSummary': 'Latest AI summary',
          'overallSummary': 'Cumulative work',
          'timeline': [
            {
              'kind': 'report',
              'text': 'Latest raw reply',
              'summary': 'Latest AI summary',
              'at': latest,
            },
            {'kind': 'report', 'text': 'Earlier raw reply', 'at': earlier},
          ],
        },
      );
      await pump(
        tester,
        AgentContextReportView(
          sessionId: 'terminal',
          live: entry,
          summarize: (at) async {
            requested.add(at);
          },
        ),
      );
      expect(find.text('整體進度摘要'), findsOneWidget);
      expect(find.text('Cumulative work'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('overall-summary-action')));
      await tester.pumpAndSettle();
      final latestButton = find.byKey(
        const ValueKey(('latest-summary-action', latest)),
      );
      await tester.ensureVisible(latestButton);
      await tester.tap(latestButton);
      await tester.pumpAndSettle();
      final historyButton = find.byKey(
        const ValueKey(('history-summary-action', earlier)),
      );
      await tester.scrollUntilVisible(historyButton, 120);
      await tester.tap(historyButton);
      await tester.pumpAndSettle();
      expect(requested, [null, latest, earlier]);
    },
  );
  testWidgets('long original reply has its own expand and collapse labels', (
    tester,
  ) async {
    final reply = List.filled(19, 'Original work reply. ').join().trim();
    final entry = live.copyWith(
      contextOverview: {'goal': 'Fix reports', 'progress': reply},
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(
            child: SizedBox(
              width: 280,
              child: AgentContextReportView(sessionId: 'terminal', live: entry),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('最新工作回報（原始回覆）'), findsOneWidget);
    expect(find.text('展開摘要'), findsNothing);
    expect(find.text('展開回覆'), findsOneWidget);
    expect(
      tester.getBottomLeft(find.text(reply)).dy,
      lessThanOrEqualTo(tester.getTopLeft(find.text('展開回覆')).dy),
    );
    await tester.tap(find.text('展開回覆'));
    await tester.pumpAndSettle();
    expect(
      find.byWidgetPredicate((w) => w is SelectableText && w.data == reply),
      findsOneWidget,
    );
    await tester.ensureVisible(find.text('收合回覆'));
    await tester.pumpAndSettle();
    expect(
      tester
          .getBottomLeft(
            find.byWidgetPredicate(
              (w) => w is SelectableText && w.data == reply,
            ),
          )
          .dy,
      lessThanOrEqualTo(tester.getTopLeft(find.text('收合回覆')).dy),
    );
    await tester.tap(find.text('收合回覆'));
    await tester.pumpAndSettle();
    expect(find.text('展開回覆'), findsOneWidget);
  });
  testWidgets('long objective and cumulative summary expand independently', (
    tester,
  ) async {
    final goal = List.filled(20, 'Retain the full objective. ').join().trim();
    final overall = List.filled(20, 'Cumulative verified work. ').join().trim();
    final entry = live.copyWith(
      contextOverview: {'goal': goal, 'overallSummary': overall},
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(
            child: SizedBox(
              width: 320,
              child: AgentContextReportView(sessionId: 'terminal', live: entry),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final fullGoal = find.byWidgetPredicate(
      (w) => w is SelectableText && w.data == goal,
    );
    final fullOverall = find.byWidgetPredicate(
      (w) => w is SelectableText && w.data == overall,
    );
    expect(find.text('展開目標'), findsOneWidget);
    expect(find.text('展開總摘要'), findsOneWidget);
    expect(
      tester.getBottomLeft(find.text(goal)).dy,
      lessThanOrEqualTo(tester.getTopLeft(find.text('展開目標')).dy),
    );
    expect(
      tester.getBottomLeft(find.text(overall)).dy,
      lessThanOrEqualTo(tester.getTopLeft(find.text('展開總摘要')).dy),
    );
    await tester.tap(find.text('展開目標'));
    await tester.pumpAndSettle();
    expect(fullGoal, findsOneWidget);
    await tester.ensureVisible(find.text('收合目標'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('收合目標'));
    await tester.pumpAndSettle();
    expect(fullGoal, findsNothing);
    await tester.ensureVisible(find.text('展開總摘要'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('展開總摘要'));
    await tester.pumpAndSettle();
    expect(fullOverall, findsOneWidget);
    await tester.ensureVisible(find.text('收合總摘要'));
    await tester.pumpAndSettle();
    await tester.tap(find.text('收合總摘要'));
    await tester.pumpAndSettle();
    expect(fullOverall, findsNothing);
  });
  testWidgets('manual summary errors remain visible on the action', (
    tester,
  ) async {
    await pump(
      tester,
      AgentContextReportView(
        sessionId: 'terminal',
        live: live,
        summarize: (_) async =>
            throw StateError('Summary model is unavailable'),
      ),
    );
    await tester.tap(find.byKey(const ValueKey('overall-summary-action')));
    await tester.pumpAndSettle();
    expect(find.text('摘要失敗，重試'), findsOneWidget);
    expect(find.textContaining('Summary model is unavailable'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
  testWidgets(
    'long summary expands, collapses and resets on selection change',
    (tester) async {
      final summary = List.filled(20, 'Fixed UI and tests. ').join().trim();
      final entry = live.copyWith(
        contextOverview: {
          'goal': 'Fix reports',
          'progress': 'RAW PRIVATE REPLY',
          'reportSummary': summary,
        },
      );
      Future<void> show(String session) async {
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: Center(
                child: SizedBox(
                  width: 280,
                  child: AgentContextReportView(
                    key: ValueKey(session),
                    sessionId: session,
                    live: entry,
                  ),
                ),
              ),
            ),
          ),
        );
        await tester.pumpAndSettle();
      }

      final fullSummary = find.byWidgetPredicate(
        (widget) => widget is SelectableText && widget.data == summary,
      );
      await show('terminal');
      expect(find.text('RAW PRIVATE REPLY'), findsNothing);
      expect(find.text('最新工作回報（AI 摘要）'), findsOneWidget);
      expect(
        find.textContaining('Auto-Summarize Agent Work Reports'),
        findsNothing,
      );
      expect(find.text('展開摘要'), findsOneWidget);
      expect(fullSummary, findsNothing);
      await tester.tap(find.text('展開摘要'));
      await tester.pumpAndSettle();
      await tester.ensureVisible(find.text('收合摘要'));
      expect(fullSummary, findsOneWidget);
      await tester.tap(find.text('收合摘要'));
      await tester.pumpAndSettle();
      expect(fullSummary, findsNothing);
      await tester.tap(find.text('展開摘要'));
      await tester.pumpAndSettle();
      await show('other-terminal');
      expect(find.text('展開摘要'), findsOneWidget);
      expect(fullSummary, findsNothing);
    },
  );
}

class _SelectedTabController extends WorkbenchController {
  _SelectedTabController(this.initial);
  final WorkbenchState initial;
  @override
  WorkbenchState build() => initial;
  void select(String id) {
    state = state.copyWith(activeTabIdByWorkspace: {'workspace': id});
  }

  void switchConversation(String id, String conversationId) {
    state = state.copyWith(
      tabsByWorkspace: {
        ...state.tabsByWorkspace,
        'workspace': [
          for (final tab in state.tabsFor('workspace'))
            if (tab.id == id)
              tab.copyWith(
                payload: {
                  ...tab.payload,
                  'agentTitleConversationId': conversationId,
                },
              )
            else
              tab,
        ],
      },
    );
  }
}

class _PresenceController extends AgentStatusController {
  @override
  Map<String, AgentStatusEntry> build() => {};
  void replace(Map<String, AgentStatusEntry> entries) => state = entries;
}

class _ContextHost implements RuntimeHostClient {
  _ContextHost(this.saved);
  final Map<String, AgentContextOverview> saved;
  final requests = <(String, Map<String, Object?>)>[];
  @override
  Stream<RuntimeHostEvent> get runtimeEvents => const Stream.empty();
  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const {},
    Duration? timeout,
  ]) async {
    requests.add((type, payload));
    if (type != 'agentContext.get') throw StateError('Unexpected request');
    final id = payload['tabId'];
    return {'readOnly': true, 'contextOverview': saved[id]?.toJson()};
  }
}
