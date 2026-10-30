import 'dart:convert';

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
    // The moved Tab alone is not sufficient evidence: another pane could
    // have disappeared or acquired a duplicate Tab while Host notifications
    // were delivered in different orders.
    _requireConsistentWorkspace(sourceWorkspaceId, sourceTabs, sourceLayout);
    _requireConsistentWorkspace(targetWorkspaceId, targetTabs, targetLayout);
    // Refresh the snapshots a second time before replacing local state.
    // A concurrent Host layout write must not be hidden by an older pair of
    // individually valid responses. Failure leaves the local state untouched.
    final latestSourceTabs = await _port.listTabs(sourceWorkspaceId);
    final latestTargetTabs = await _port.listTabs(targetWorkspaceId);
    final latestSourceLayout = await _port.findLayout(sourceWorkspaceId);
    final latestTargetLayout = await _port.findLayout(targetWorkspaceId);
    if (_tabSignature(latestSourceTabs) != _tabSignature(sourceTabs) ||
        _tabSignature(latestTargetTabs) != _tabSignature(targetTabs) ||
        _layoutSignature(latestSourceLayout) !=
            _layoutSignature(sourceLayout) ||
        _layoutSignature(latestTargetLayout) !=
            _layoutSignature(targetLayout)) {
      throw StateError(
        'Terminal transfer changed during reconciliation; refresh both workspaces.',
      );
    }
    _applyTabs(sourceWorkspaceId, sourceTabs);
    _applyTabs(targetWorkspaceId, targetTabs);
    await _applyLayoutWithoutPersistence(sourceLayout);
    await _applyLayoutWithoutPersistence(targetLayout);
  }
}

void _requireConsistentWorkspace(
  String workspaceId,
  List<WorkspaceTabRecord> tabs,
  WorkbenchLayout layout,
) {
  if (layout.workspaceId != workspaceId ||
      !layout.groups.containsKey(layout.activeGroupId)) {
    throw StateError('Host returned a layout for the wrong workspace.');
  }
  final knownTabs = <String>{};
  for (final tab in tabs) {
    if (tab.workspaceId != workspaceId || !knownTabs.add(tab.id)) {
      throw StateError('Host returned duplicate or misplaced Workspace tabs.');
    }
  }
  final paneIds = layout.root.leafGroupIds();
  if (paneIds.toSet().length != paneIds.length ||
      paneIds.toSet().length != layout.groups.length ||
      !paneIds.toSet().containsAll(layout.groups.keys)) {
    throw StateError('Host transfer layout has an inconsistent split tree.');
  }
  final layoutTabs = <String>{};
  for (final entry in layout.groups.entries) {
    final group = entry.value;
    if (group.id != entry.key ||
        (group.tabIds.isEmpty
            ? group.activeTabId != null
            : !group.tabIds.contains(group.activeTabId))) {
      throw StateError('Host transfer layout has an invalid pane selection.');
    }
    for (final id in group.tabIds) {
      if (!layoutTabs.add(id)) {
        throw StateError('Host transfer layout contains a duplicate tab.');
      }
    }
  }
  if (knownTabs.length != layoutTabs.length ||
      !knownTabs.containsAll(layoutTabs)) {
    throw StateError(
      'Host transfer layout does not match its authoritative workspace tabs.',
    );
  }
}

String _tabSignature(List<WorkspaceTabRecord> tabs) {
  final records = <Map<String, dynamic>>[for (final tab in tabs) tab.toMap()]
    ..sort((a, b) => (a['id'] as String).compareTo(b['id'] as String));
  return jsonEncode(records);
}

String _layoutSignature(WorkbenchLayout? layout) =>
    layout == null ? '<none>' : jsonEncode(layout.toMap());
