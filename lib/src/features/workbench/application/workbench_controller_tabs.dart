part of 'workbench_controller.dart';

mixin _WorkbenchControllerTabs
    on _$WorkbenchController, _WorkbenchControllerInternals {
  /// Reconcile a transfer whose original reply was lost. Do not retry the
  /// mutation or mint another id: the Host must confirm its *same* receipt
  /// and still-running Session, then both Layouts must be read back before
  /// we release the coordinator's unknown-outcome guard.
  Future<bool> confirmUnknownTerminalTransfer(String tabId) async {
    final coordinator = ref.read(workbenchTerminalTransferCoordinatorProvider);
    await coordinator.restoreUnknownTransfers();
    final original = coordinator.unknownPlan(tabId);
    if (original == null) return false;
    if (coordinator.isTransferring(tabId)) {
      throw StateError('Terminal transfer is still in progress.');
    }
    coordinator.beginUnknownConfirmation(
      tabId: tabId,
      clientMutationId: original.clientMutationId,
    );
    var layoutFenceAcquired = false;
    try {
      _tabLayoutOwner.beginTerminalTransfer(
        original.sourceWorkspaceId,
        original.targetWorkspaceId,
      );
      layoutFenceAcquired = true;
      final committed = await coordinator.confirmUnknownOutcome(tabId);
      if (committed == null) {
        throw TerminalTransferOutcomeUnknownException(
          tabId: tabId,
          clientMutationId: original.clientMutationId,
        );
      }
      if (_disposed) {
        throw StateError(
          'Workbench closed; reopen it to confirm Terminal move.',
        );
      }
      final port = ref.read(workbenchTerminalTransferPortProvider);
      await WorkbenchTerminalTransferReconciliation(
        port: port,
        applyTabs: _tabLayoutOwner.applyWorkspaceTabs,
        applyLayoutWithoutPersistence: (layout) =>
            _tabLayoutOwner.applyWorkspaceLayout(layout, persist: false),
      ).reconcile(
        tabId: tabId,
        sourceWorkspaceId: original.sourceWorkspaceId,
        targetWorkspaceId: original.targetWorkspaceId,
      );
      // Only the exact Host-verified mutation may release this guard.
      await coordinator.acknowledgeRecovered(
        tabId: tabId,
        clientMutationId: original.clientMutationId,
      );
      final target = _workspaceById(original.targetWorkspaceId);
      if (target != null && !_disposed) {
        final projectIndex = state.projects.indexWhere(
          (entry) => entry.id == target.projectId,
        );
        if (projectIndex >= 0) {
          await _selectionOwner.selectWorkspace(
            project: state.projects[projectIndex],
            workspace: target,
            ensureInitialTerminal: false,
          );
          if (!_disposed) {
            _tabLayoutOwner.activateWorkspaceTab(
              workspaceId: target.id,
              tabId: tabId,
            );
          }
        }
      }
      return true;
    } finally {
      try {
        if (layoutFenceAcquired) {
          _tabLayoutOwner.finishTerminalTransfer(
            original.sourceWorkspaceId,
            original.targetWorkspaceId,
          );
        }
      } finally {
        coordinator.finishUnknownConfirmation(tabId);
      }
    }
  }

  /// Commits the *existing* PTY to the new workspace. The current sidecar
  /// advertises this as disabled until full transfer/restart reconciliation
  /// is ready, so old builds never simulate a transfer with separate writes.
  Future<String> transferTerminalTab({
    required Workspace source,
    required Workspace destination,
    required String tabId,
    String? targetGroupId,
  }) async {
    final available = await ref.read(
      workbenchTerminalTransferAvailabilityProvider.future,
    );
    if (!available) {
      throw StateError(
        'Cross-project Terminal transfer is not supported by this Runtime.',
      );
    }
    final port = ref.read(workbenchTerminalTransferPortProvider);
    final coordinator = ref.read(workbenchTerminalTransferCoordinatorProvider);
    _tabLayoutOwner.beginTerminalTransfer(source.id, destination.id);
    try {
      final moved = await coordinator.transfer(
        source: source,
        destination: destination,
        tabId: tabId,
        targetGroupId: targetGroupId,
      );
      if (_disposed) {
        throw StateError(
          'Terminal transfer committed while Workbench was closing. Refresh to find the moved tab.',
        );
      }
      await WorkbenchTerminalTransferReconciliation(
        port: port,
        applyTabs: _tabLayoutOwner.applyWorkspaceTabs,
        applyLayoutWithoutPersistence: (layout) =>
            _tabLayoutOwner.applyWorkspaceLayout(layout, persist: false),
      ).reconcile(
        tabId: tabId,
        sourceWorkspaceId: source.id,
        targetWorkspaceId: destination.id,
      );
      final projects = state.projects;
      final projectIndex = projects.indexWhere(
        (project) => project.id == destination.projectId,
      );
      if (projectIndex < 0) {
        throw StateError('Destination project is no longer available.');
      }
      await _selectionOwner.selectWorkspace(
        project: projects[projectIndex],
        workspace: destination,
        ensureInitialTerminal: false,
      );
      if (!_disposed) {
        _tabLayoutOwner.activateWorkspaceTab(
          workspaceId: destination.id,
          tabId: tabId,
        );
      }
      final receipt = moved.payload['lastTerminalTabTransfer'];
      final id = receipt is Map ? receipt['id'] : null;
      if (id is! String || id.isEmpty) {
        throw StateError(
          'Terminal transfer was committed but the Undo receipt is unavailable.',
        );
      }
      // Host acknowledged and both layouts have been reloaded. Only now
      // erase the original pre-RPC journal. A crash before this point must
      // still require an explicit same-ID Host status check after restart.
      await coordinator.acknowledgeRecovered(
        tabId: tabId,
        clientMutationId: id,
      );
      return id;
    } finally {
      _tabLayoutOwner.finishTerminalTransfer(source.id, destination.id);
    }
  }

  /// Undo is a new, guarded reverse transfer of the exact most-recent move,
  /// not an unsafe tab.upsert or a PTY close + recreate. Reject a stale Undo
  /// click if another window subsequently moved the same terminal elsewhere.
  Future<void> undoTerminalTransfer({
    required String tabId,
    required String expectedMutationId,
  }) async {
    final tab = await ref
        .read(workbenchRepositoryProvider)
        .findWorkspaceTabById(tabId);
    final receipt = tab?.payload['lastTerminalTabTransfer'];
    if (tab == null ||
        tab.kind != WorkspaceTabKind.terminal ||
        receipt is! Map ||
        receipt['id'] != expectedMutationId ||
        receipt['to'] != tab.workspaceId) {
      throw StateError(
        'Terminal has changed since the move; Undo cannot safely restore the old workspace.',
      );
    }
    final fromId = receipt['from'];
    if (fromId is! String || fromId.isEmpty) {
      throw StateError('Terminal transfer receipt has no original workspace.');
    }
    final current = _workspaceById(tab.workspaceId);
    final original = _workspaceById(fromId);
    if (current == null ||
        original == null ||
        !original.isActive ||
        original.isArchived) {
      throw StateError(
        'The original workspace is no longer available; Undo cannot move the running process.',
      );
    }
    final previousGroup = receipt['sourceGroupId'];
    final restoredLayout = await ref
        .read(workbenchRepositoryProvider)
        .findWorkbenchLayout(original.id);
    final groupId =
        previousGroup is String &&
            restoredLayout?.groups.containsKey(previousGroup) == true
        ? previousGroup
        : null;
    await transferTerminalTab(
      source: current,
      destination: original,
      tabId: tabId,
      targetGroupId: groupId,
    );
  }

  Future<void> closeWorkspaceTab({
    required Workspace workspace,
    required String tabId,
  }) => _tabLayoutOwner.closeWorkspaceTab(workspace: workspace, tabId: tabId);

  Future<void> closeWorkspaceTabs({
    required Workspace workspace,
    required List<String> tabIds,
  }) =>
      _tabLayoutOwner.closeWorkspaceTabs(workspace: workspace, tabIds: tabIds);

  Future<void> renameWorkspaceTab({
    required String tabId,
    required String title,
  }) => _tabLayoutOwner.renameWorkspaceTab(tabId: tabId, title: title);

  Future<void> setWorkspaceTabPinned({
    required String tabId,
    required bool pinned,
  }) => _tabLayoutOwner.setWorkspaceTabPinned(tabId: tabId, pinned: pinned);

  Future<void> syncFileTabsAfterPathMove({
    required Workspace workspace,
    required String oldRelativePath,
    required String newRelativePath,
  }) => _tabLayoutOwner.syncFileTabsAfterPathMove(
    workspace: workspace,
    oldRelativePath: oldRelativePath,
    newRelativePath: newRelativePath,
  );

  void setActiveTab({required String workspaceId, required String tabId}) =>
      _selectionOwner.setActiveTab(workspaceId: workspaceId, tabId: tabId);

  void setActiveWorkspaceTab({
    required String workspaceId,
    required String groupId,
    required String tabId,
  }) => _selectionOwner.setActiveWorkspaceTab(
    workspaceId: workspaceId,
    groupId: groupId,
    tabId: tabId,
  );

  /// Promotes [groupId] to the workspace's active group when a pane in it
  /// receives real keyboard focus. Idempotent so it can be wired directly to
  /// focus-change events without risking loops or redundant rebuilds.
  ///
  /// Focus is ephemeral session state, so the new layout is applied in memory
  /// only; the next explicit user action (tab click, split, merge, rename)
  /// will persist the latest layout. This avoids a SQLite write on every
  /// pane click.
  void focusWorkbenchGroup({
    required String workspaceId,
    required String groupId,
  }) => _selectionOwner.focusWorkbenchGroup(
    workspaceId: workspaceId,
    groupId: groupId,
  );

  Future<void> moveWorkspaceTab({
    required String workspaceId,
    required String tabId,
    required String targetGroupId,
    required WorkbenchDropZone zone,
    int? index,
  }) => _tabLayoutOwner.moveWorkspaceTab(
    workspaceId: workspaceId,
    tabId: tabId,
    targetGroupId: targetGroupId,
    zone: zone,
    index: index,
  );

  Future<WorkspaceTabRecord> splitWorkbenchGroupWithTerminal({
    required Workspace workspace,
    required String groupId,
    required WorkbenchDropZone zone,
  }) => _tabLayoutOwner.splitWorkbenchGroupWithTerminal(
    workspace: workspace,
    groupId: groupId,
    zone: zone,
  );

  Future<void> mergeWorkbenchGroupIntoSibling({
    required String workspaceId,
    required String groupId,
  }) => _tabLayoutOwner.mergeWorkbenchGroupIntoSibling(
    workspaceId: workspaceId,
    groupId: groupId,
  );

  void updateWorkbenchSplitRatio({
    required String workspaceId,
    required List<int> nodePath,
    required double ratio,
  }) => _tabLayoutOwner.updateWorkbenchSplitRatio(
    workspaceId: workspaceId,
    nodePath: nodePath,
    ratio: ratio,
  );
}
