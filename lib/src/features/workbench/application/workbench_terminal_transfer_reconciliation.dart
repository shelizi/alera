import 'package:alera/src/features/workbench/application/workbench_terminal_tab_transfer_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';

/// Re-hydrates both workspaces after the host confirms a transfer. Never
/// synthesize a local tab.move to hide a failed or ambiguous Runtime write.
///
/// This intentionally operates on read-only authoritative snapshots and
/// applies them without persistence. Re-persisting old client layouts after
/// the host's atomic CAS would reintroduce the source/destination race.
final class WorkbenchTerminalTransferReconciliation {
  const WorkbenchTerminalTransferReconciliation({
    required WorkbenchTerminalTabTransferPort port,
    required void Function(String workspaceId, List<WorkspaceTabRecord> tabs)
    applyTabs,
    required Future<void> Function(WorkbenchLayout layout)
    applyLayoutWithoutPersistence,
  }) : _port = port,
       _applyTabs = applyTabs,
       _applyLayoutWithoutPersistence = applyLayoutWithoutPersistence;

  final WorkbenchTerminalTabTransferPort _port;
  final void Function(String, List<WorkspaceTabRecord>) _applyTabs;
  final Future<void> Function(WorkbenchLayout) _applyLayoutWithoutPersistence;

  Future<void> reconcile({
    required String tabId,
    required String sourceWorkspaceId,
    required String targetWorkspaceId,
  }) async {
    if (sourceWorkspaceId == targetWorkspaceId) {
      throw StateError(
        'Cross-workspace transfer requires distinct workspaces.',
      );
    }
    final sourceTabs = await _port.listTabs(sourceWorkspaceId);
    final targetTabs = await _port.listTabs(targetWorkspaceId);
    if (sourceTabs.any((tab) => tab.id == tabId) ||
        targetTabs.where((tab) => tab.id == tabId).length != 1) {
      throw StateError(
        'Runtime transfer is not reflected in both workspaces; refresh required.',
      );
    }
    final sourceLayout = await _port.findLayout(sourceWorkspaceId);
    final targetLayout = await _port.findLayout(targetWorkspaceId);
    if (sourceLayout == null ||
        targetLayout == null ||
        sourceLayout.groupIdForTab(tabId) != null ||
        targetLayout.groupIdForTab(tabId) == null) {
      throw StateError(
        'Runtime transfer layout is incomplete; refresh required.',
      );
    }
    _applyTabs(sourceWorkspaceId, sourceTabs);
    _applyTabs(targetWorkspaceId, targetTabs);
    await _applyLayoutWithoutPersistence(sourceLayout);
    await _applyLayoutWithoutPersistence(targetLayout);
  }
}
