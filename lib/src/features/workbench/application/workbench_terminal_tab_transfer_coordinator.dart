import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:uuid/uuid.dart';

/// The Host might have committed the SQLite transfer before a client timeout
/// or disconnect. Do not report success, undo, or run a fresh mutation until
/// the original receipt and live PTY owner can be confirmed.
final class TerminalTransferOutcomeUnknownException implements Exception {
  const TerminalTransferOutcomeUnknownException({
    required this.tabId,
    required this.clientMutationId,
  });

  final String tabId;
  final String clientMutationId;

  @override
  String toString() =>
      'Terminal move status is unknown (tab $tabId, operation '
      '$clientMutationId). Refresh both workspaces before trying again; '
      'the running command may already be in the destination.';
}

/// Narrow Runtime capability used exclusively for cross-workspace terminal
/// transfer. A regular tab.upsert/layout.upsert is *not* a substitute: it
/// cannot atomically move a running PTY's owner.
abstract interface class WorkbenchTerminalTabTransferPort {
  Future<List<WorkspaceTabRecord>> listTabs(String workspaceId);
  Future<WorkbenchLayout?> findLayout(String workspaceId);
  Future<WorkspaceTabRecord> commit(WorkbenchTerminalTabTransferPlan plan);
}

/// Optional authoritative receipt lookup for clients whose transfer reply
/// was lost. The operation ID must never be replaced or blindly replayed.
abstract interface class WorkbenchTerminalTabTransferRecoveryPort {
  Future<WorkspaceTabRecord?> confirmCommitted(
    WorkbenchTerminalTabTransferPlan original,
  );
}

/// A crash-safe record of the exact Host operation and its two workspace
/// snapshots. Must be durable before sending tab.transfer; clear only after
/// confirming the Host receipt and reconciling both layouts.
abstract interface class WorkbenchTerminalTransferJournal {
  Future<Map<String, WorkbenchTerminalTabTransferPlan>> loadPending();
  Future<void> remember(WorkbenchTerminalTabTransferPlan plan);
  Future<void> forget(WorkbenchTerminalTabTransferPlan plan);
}

/// Source and destination layouts are prepared from the same snapshot and
/// sent to the Runtime as an optimistic, atomic compare-and-swap request.
/// The host is responsible for validating/committing both layouts and the
/// live PTY/session owner as one logical operation.
class const WorkbenchTerminalTabTransferPlan({
  required final String tabId,
  required final String sourceWorkspaceId,
  required final String targetWorkspaceId,
  required final String sessionId,
  required final String clientMutationId,
  required final WorkbenchLayout? expectedSourceLayout,
  required final WorkbenchLayout? expectedTargetLayout,
  required final WorkbenchLayout sourceLayout,
  required final WorkbenchLayout targetLayout,
}) {
  Map<String, Object?> toRuntimePayload() => <String, Object?>{
    'tabId': tabId,
    'sourceWorkspaceId': sourceWorkspaceId,
    'targetWorkspaceId': targetWorkspaceId,
    'sessionId': sessionId,
    'clientMutationId': clientMutationId,
    'expectedSourceLayout': expectedSourceLayout?.toMap(),
    'expectedTargetLayout': expectedTargetLayout?.toMap(),
    'sourceLayout': sourceLayout.toMap(),
    'targetLayout': targetLayout.toMap(),
  };
}

/// No UI state is changed before the Runtime acknowledges a successful move.
/// Failing a transfer preserves the original tab, session, and layouts; an
/// ambiguous timeout is *not* retried with a new mutation id.
final class WorkbenchTerminalTabTransferCoordinator {
  WorkbenchTerminalTabTransferCoordinator(
    this._port, {
    Uuid? uuid,
    WorkbenchTerminalTransferJournal? journal,
  }) : _uuid = uuid ?? const Uuid(),
       _journal = journal;

  final WorkbenchTerminalTabTransferPort _port;
  final Uuid _uuid;
  final WorkbenchTerminalTransferJournal? _journal;
  Future<void>? _restoringJournal;
  final Set<String> _transferringTabIds = <String>{};
  final Map<String, WorkbenchTerminalTabTransferPlan> _unknownTransfers =
      <String, WorkbenchTerminalTabTransferPlan>{};

  /// Crash recovery is fail-closed: corrupt or unreadable journal data must
  /// block another move instead of being silently treated as an empty set.
  Future<void> restoreUnknownTransfers() =>
      _restoringJournal ??= _loadJournal();

  Future<void> _loadJournal() async {
    final journal = _journal;
    if (journal == null) return;
    final saved = await journal.loadPending();
    for (final entry in saved.entries) {
      _unknownTransfers.putIfAbsent(entry.key, () => entry.value);
    }
  }

  bool isTransferring(String tabId) => _transferringTabIds.contains(tabId);

  /// Keep the original receipt for an inconclusive timeout. A second drag
  /// cannot silently mint a new mutation ID for this same live PTY.
  bool hasUnknownOutcome(String tabId) => _unknownTransfers.containsKey(tabId);

  /// The exact original attempt, never a synthesized replacement mutation.
  /// Its source/destination are needed to re-hydrate both authoritative
  /// layouts once the Host proves that the PTY and its receipt agree.
  WorkbenchTerminalTabTransferPlan? unknownPlan(String tabId) =>
      _unknownTransfers[tabId];

  /// A confirmed Host receipt must not be cleared until the Workbench has
  /// reloaded *both* layouts. Otherwise the same stale client could issue a
  /// second move after an incomplete/failed reconcile.
  Future<void> acknowledgeRecovered({
    required String tabId,
    required String clientMutationId,
  }) async {
    final pending = _unknownTransfers[tabId];
    if (pending != null && pending.clientMutationId == clientMutationId) {
      await _journal?.forget(pending);
      _unknownTransfers.remove(tabId);
    }
  }

  Future<WorkspaceTabRecord?> confirmUnknownOutcome(String tabId) async {
    final original = _unknownTransfers[tabId];
    if (original == null) return null;
    final port = _port;
    if (port is! WorkbenchTerminalTabTransferRecoveryPort) return null;
    try {
      final committed = await port.confirmCommitted(original);
      if (committed == null ||
          committed.id != original.tabId ||
          committed.workspaceId != original.targetWorkspaceId ||
          committed.terminalSessionId != original.sessionId) {
        return null;
      }
      // Leave the unresolved guard in place until both source and
      // destination layouts are authoritatively reconciled by the caller.
      return committed;
    } on Object {
      // Reconnect may still be in flight. Retain the exact unresolved
      // operation rather than risking a second transfer of the same PTY.
      return null;
    }
  }

  Future<WorkspaceTabRecord> transfer({
    required Workspace source,
    required Workspace destination,
    required String tabId,
    String? targetGroupId,
    String? clientMutationId,
  }) async {
    if (!_transferringTabIds.add(tabId)) {
      throw StateError('This terminal tab is already being transferred.');
    }
    try {
      await restoreUnknownTransfers();
      final unresolved = _unknownTransfers[tabId];
      if (unresolved != null) {
        final committed = await confirmUnknownOutcome(tabId);
        if (committed != null) {
          throw StateError(
            'The previous Terminal move was confirmed after reconnect. '
            'Refresh both workspaces before moving it again.',
          );
        }
        throw TerminalTransferOutcomeUnknownException(
          tabId: tabId,
          clientMutationId: unresolved.clientMutationId,
        );
      }
      if (source.id == destination.id ||
          !source.isActive ||
          source.isArchived ||
          !destination.isActive ||
          destination.isArchived ||
          source.hostId != destination.hostId) {
        throw StateError(
          'Terminal transfer requires two active workspaces on the same host.',
        );
      }

      final sourceTabs = await _port.listTabs(source.id);
      final destinationTabs = await _port.listTabs(destination.id);
      final matches = sourceTabs.where((tab) => tab.id == tabId).toList();
      if (matches.length != 1 ||
          matches.single.kind != WorkspaceTabKind.terminal) {
        throw StateError('Only an existing terminal tab can be transferred.');
      }
      if (destinationTabs.any((tab) => tab.id == tabId)) {
        throw StateError('The destination already contains this tab.');
      }
      final tab = matches.single;
      final previousSource = await _port.findLayout(source.id);
      final previousTarget = await _port.findLayout(destination.id);
      final currentSource =
          (previousSource ??
                  WorkbenchLayout.single(
                    workspaceId: source.id,
                    tabIds: <String>[for (final entry in sourceTabs) entry.id],
                  ))
              .sanitize(sourceTabs);
      final currentTarget =
          (previousTarget ??
                  WorkbenchLayout.single(
                    workspaceId: destination.id,
                    tabIds: <String>[
                      for (final entry in destinationTabs) entry.id,
                    ],
                  ))
              .sanitize(destinationTabs);
      final groupId = targetGroupId ?? currentTarget.activeGroupId;
      if (!currentTarget.groups.containsKey(groupId)) {
        throw StateError('The destination pane no longer exists.');
      }

      final remaining = <WorkspaceTabRecord>[
        for (final entry in sourceTabs)
          if (entry.id != tabId) entry,
      ];
      final moved = tab.copyWith(workspaceId: destination.id);
      final nextSource = currentSource.removeTab(tabId).sanitize(remaining);
      final nextTarget = currentTarget
          .addTabToGroup(groupId: groupId, tabId: tabId)
          .sanitize(<WorkspaceTabRecord>[...destinationTabs, moved]);
      final plan = WorkbenchTerminalTabTransferPlan(
        tabId: tabId,
        sourceWorkspaceId: source.id,
        targetWorkspaceId: destination.id,
        sessionId: tab.terminalSessionId,
        clientMutationId: clientMutationId ?? _uuid.v4(),
        expectedSourceLayout: previousSource,
        expectedTargetLayout: previousTarget,
        sourceLayout: nextSource,
        targetLayout: nextTarget,
      );
      // Never send a mutating RPC without its durable recovery receipt.
      // Keep the guard even after a normal Host reply until both workspaces
      // have been authoritatively read back by the controller.
      await _journal?.remember(plan);
      _unknownTransfers[tabId] = plan;
      final WorkspaceTabRecord updated;
      try {
        updated = await _port.commit(plan);
      } on TerminalTransferOutcomeUnknownException {
        _unknownTransfers[tabId] = plan;
        rethrow;
      }
      if (updated.id != tabId ||
          updated.workspaceId != destination.id ||
          updated.terminalSessionId != tab.terminalSessionId) {
        // Do not roll back with a second mutation: after a lost reply the
        // result may already be committed. Block a fresh mutation until the
        // original receipt and two Host layouts have been reconciled.
        _unknownTransfers[tabId] = plan;
        throw TerminalTransferOutcomeUnknownException(
          tabId: tabId,
          clientMutationId: plan.clientMutationId,
        );
      }
      return updated;
    } finally {
      _transferringTabIds.remove(tabId);
    }
  }
}
