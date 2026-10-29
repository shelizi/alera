import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:uuid/uuid.dart';

/// Narrow Runtime capability used exclusively for cross-workspace terminal
/// transfer. A regular tab.upsert/layout.upsert is *not* a substitute: it
/// cannot atomically move a running PTY's owner.
abstract interface class WorkbenchTerminalTabTransferPort {
  Future<List<WorkspaceTabRecord>> listTabs(String workspaceId);
  Future<WorkbenchLayout?> findLayout(String workspaceId);
  Future<WorkspaceTabRecord> commit(WorkbenchTerminalTabTransferPlan plan);
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
  WorkbenchTerminalTabTransferCoordinator(this._port, {Uuid? uuid})
    : _uuid = uuid ?? const Uuid();

  final WorkbenchTerminalTabTransferPort _port;
  final Uuid _uuid;
  final Set<String> _transferringTabIds = <String>{};

  bool isTransferring(String tabId) => _transferringTabIds.contains(tabId);

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
      final updated = await _port.commit(plan);
      if (updated.id != tabId ||
          updated.workspaceId != destination.id ||
          updated.terminalSessionId != tab.terminalSessionId) {
        // Do not roll back with a second mutation: after a lost reply the
        // result may already be committed. The UI should reload both sides.
        throw StateError(
          'The runtime returned a mismatched terminal transfer result.',
        );
      }
      return updated;
    } finally {
      _transferringTabIds.remove(tabId);
    }
  }
}
