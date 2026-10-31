import 'package:alera/src/features/workbench/application/workbench_terminal_tab_transfer_coordinator.dart';
import 'package:alera/src/features/workbench/application/workbench_terminal_transfer_reconciliation.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workbench_tab_drag_data.dart';
import 'package:flutter_test/flutter_test.dart';

final class _MemoryTransferHost
    implements
        WorkbenchTerminalTabTransferPort,
        WorkbenchTerminalTabTransferRecoveryPort {
  _MemoryTransferHost(WorkspaceTabRecord original)
    : tabs = <String, List<WorkspaceTabRecord>>{
        'source': <WorkspaceTabRecord>[original],
        'target': <WorkspaceTabRecord>[],
      },
      layouts = <String, WorkbenchLayout>{
        'source': WorkbenchLayout.single(
          workspaceId: 'source',
          tabIds: <String>[original.id],
        ),
        'target': WorkbenchLayout.single(
          workspaceId: 'target',
          tabIds: <String>[],
        ),
      };

  final Map<String, List<WorkspaceTabRecord>> tabs;
  final Map<String, WorkbenchLayout> layouts;
  WorkbenchTerminalTabTransferPlan? lastCommitted;
  bool loseNextReply = false;
  int commits = 0;

  @override
  Future<List<WorkspaceTabRecord>> listTabs(String id) async =>
      List.unmodifiable(tabs[id]!);

  @override
  Future<WorkbenchLayout?> findLayout(String id) async => layouts[id];

  @override
  Future<WorkspaceTabRecord> commit(
    WorkbenchTerminalTabTransferPlan plan,
  ) async {
    final source = tabs[plan.sourceWorkspaceId]!;
    final target = tabs[plan.targetWorkspaceId]!;
    final original = source.singleWhere((tab) => tab.id == plan.tabId);
    final moved = original.copyWith(
      workspaceId: plan.targetWorkspaceId,
      payload: <String, Object?>{
        ...original.payload,
        'lastTerminalTabTransfer': <String, Object?>{
          'id': plan.clientMutationId,
          'from': plan.sourceWorkspaceId,
          'to': plan.targetWorkspaceId,
        },
      },
    );
    source.remove(original);
    target.add(moved);
    layouts[plan.sourceWorkspaceId] = plan.sourceLayout;
    layouts[plan.targetWorkspaceId] = plan.targetLayout;
    lastCommitted = plan;
    commits += 1;
    if (loseNextReply) {
      loseNextReply = false;
      throw TerminalTransferOutcomeUnknownException(
        tabId: plan.tabId,
        clientMutationId: plan.clientMutationId,
      );
    }
    return moved;
  }

  @override
  Future<WorkspaceTabRecord?> confirmCommitted(
    WorkbenchTerminalTabTransferPlan plan,
  ) async => lastCommitted?.clientMutationId == plan.clientMutationId
      ? tabs[plan.targetWorkspaceId]!.single
      : null;
}

final class _MemoryTransferJournal implements WorkbenchTerminalTransferJournal {
  final pending = <String, WorkbenchTerminalTabTransferPlan>{};

  @override
  Future<Map<String, WorkbenchTerminalTabTransferPlan>> loadPending() async =>
      Map.of(pending);

  @override
  Future<void> remember(WorkbenchTerminalTabTransferPlan plan) async {
    if (pending.containsKey(plan.tabId)) {
      throw StateError('Original transfer is still unresolved.');
    }
    pending[plan.tabId] = plan;
  }

  @override
  Future<void> forget(WorkbenchTerminalTabTransferPlan plan) async {
    if (pending[plan.tabId]?.clientMutationId != plan.clientMutationId) {
      throw StateError('Cannot clear an unrelated transfer.');
    }
    pending.remove(plan.tabId);
  }
}

Workspace _workspace(String id) => Workspace(
  id: id,
  projectId: id,
  name: id,
  path: 'C:/repo/$id',
  kind: WorkspaceKind.main,
  status: WorkspaceStatus.active,
  createdAt: DateTime.utc(2026, 10, 31),
  updatedAt: DateTime.utc(2026, 10, 31),
);

WorkspaceTabRecord _originalTab() => WorkspaceTabRecord(
  id: 'original-tab',
  workspaceId: 'source',
  kind: WorkspaceTabKind.terminal,
  title: 'Running Claude',
  createdAt: DateTime.utc(2026, 10, 31),
  updatedAt: DateTime.utc(2026, 10, 31),
  payload: const <String, Object?>{
    'terminalSessionId': 'original-pty-session',
    'terminalExecutionWorkspaceId': 'source',
    'terminalExecutionWorkingDirectory': 'C:/repo/source',
  },
);

void main() {
  test(
    'A to B to A reuses the original tab and Session; both layouts reconcile',
    () async {
      final host = _MemoryTransferHost(_originalTab());
      final journal = _MemoryTransferJournal();
      final coordinator = WorkbenchTerminalTabTransferCoordinator(
        host,
        journal: journal,
      );
      final applied = <String, WorkbenchLayout>{};
      final visible = <String, List<WorkspaceTabRecord>>{};
      final reconciliation = WorkbenchTerminalTransferReconciliation(
        port: host,
        applyTabs: (id, tabs) => visible[id] = tabs,
        applyLayoutWithoutPersistence: (layout) async {
          applied[layout.workspaceId] = layout;
        },
      );
      final first = await coordinator.transfer(
        source: _workspace('source'),
        destination: _workspace('target'),
        tabId: 'original-tab',
        clientMutationId: 'first-move',
      );
      expect(first.id, 'original-tab');
      expect(first.terminalSessionId, 'original-pty-session');
      expect(first.title, 'Running Claude');
      expect(
        first.payload['terminalExecutionWorkingDirectory'],
        'C:/repo/source',
      );
      await reconciliation.reconcile(
        tabId: first.id,
        sourceWorkspaceId: 'source',
        targetWorkspaceId: 'target',
      );
      expect(visible['source'], isEmpty);
      expect(visible['target'], hasLength(1));
      expect(applied['source']!.groupIdForTab(first.id), isNull);
      expect(applied['target']!.groupIdForTab(first.id), isNotNull);
      await coordinator.acknowledgeRecovered(
        tabId: first.id,
        clientMutationId: 'first-move',
      );

      final back = await coordinator.transfer(
        source: _workspace('target'),
        destination: _workspace('source'),
        tabId: first.id,
        clientMutationId: 'undo-move',
      );
      await reconciliation.reconcile(
        tabId: back.id,
        sourceWorkspaceId: 'target',
        targetWorkspaceId: 'source',
      );
      await coordinator.acknowledgeRecovered(
        tabId: back.id,
        clientMutationId: 'undo-move',
      );
      expect(host.commits, 2);
      expect(back.id, first.id);
      expect(back.terminalSessionId, first.terminalSessionId);
      expect(
        back.payload['terminalExecutionWorkingDirectory'],
        'C:/repo/source',
      );
      expect(visible['target'], isEmpty);
      expect(visible['source'], hasLength(1));
      expect(journal.pending, isEmpty);
    },
  );

  test(
    'lost reply keeps the original operation; status checks never remint',
    () async {
      final host = _MemoryTransferHost(_originalTab())..loseNextReply = true;
      final journal = _MemoryTransferJournal();
      final coordinator = WorkbenchTerminalTabTransferCoordinator(
        host,
        journal: journal,
      );
      await expectLater(
        coordinator.transfer(
          source: _workspace('source'),
          destination: _workspace('target'),
          tabId: 'original-tab',
          clientMutationId: 'original-id',
        ),
        throwsA(isA<TerminalTransferOutcomeUnknownException>()),
      );
      expect(coordinator.hasUnknownOutcome('original-tab'), isTrue);
      expect(journal.pending['original-tab']!.clientMutationId, 'original-id');
      final confirmed = await coordinator.confirmUnknownOutcome('original-tab');
      expect(confirmed!.terminalSessionId, 'original-pty-session');
      expect(host.commits, 1);
      await expectLater(
        coordinator.transfer(
          source: _workspace('source'),
          destination: _workspace('target'),
          tabId: 'original-tab',
          clientMutationId: 'illicit-retry',
        ),
        throwsStateError,
      );
      expect(host.commits, 1);
      await coordinator.acknowledgeRecovered(
        tabId: 'original-tab',
        clientMutationId: 'original-id',
      );
      expect(journal.pending, isEmpty);
    },
  );

  test('non-terminal and same-workspace drops are never transfer eligible', () {
    const drag = WorkbenchTabDragData(
      workspaceId: 'source',
      sourceGroupId: 'main',
      tabId: 'original-tab',
      kind: WorkspaceTabKind.terminal,
      sourceHostId: 'local',
    );
    expect(
      drag.canTransferTo(
        targetWorkspaceId: 'target',
        targetHostId: 'local',
        targetArchived: false,
      ),
      isTrue,
    );
    expect(
      drag.canTransferTo(
        targetWorkspaceId: 'source',
        targetHostId: 'local',
        targetArchived: false,
      ),
      isFalse,
    );
    expect(
      drag.canTransferTo(
        targetWorkspaceId: 'target',
        targetHostId: 'different-host',
        targetArchived: false,
      ),
      isFalse,
    );
  });
}
