part of 'workbench_catalog_owner.dart';

/// Workspace sections: the sidebar grouping stored next to workspaces.
extension WorkbenchCatalogOwnerSections on WorkbenchCatalogOwner {
  WorkspaceSectionRepository get _sectionRepository =>
      _host.workbenchRepository as WorkspaceSectionRepository;

  void _startSections() {
    final repository = _host.workbenchRepository;
    if (repository is! WorkspaceSectionRepository) return;
    // Keep watching unsupported hosts: an in-app update can add this capability.
    _host.rootSubscriptions.watchSections(
      (repository as WorkspaceSectionRepository).watchSections(),
      onData: (snapshot) {
        if (_host.isDisposed) return;
        _host.emitState(
          applyWorkbenchSectionSnapshotState(
            state: _host.readState(),
            snapshot: snapshot,
          ),
        );
      },
      onError: (Object error) {
        if (!_host.isDisposed) {
          _host.emitState(
            _host.readState().copyWith(
              error: 'Could not load sections: $error',
            ),
          );
        }
      },
    );
  }

  Future<List<WorkspaceSection>> listWorkspaceSections() =>
      _sectionRepository.listSections();

  Future<void> saveWorkspaceSection(
    String workspaceId, {
    String? sectionId,
    String? newName,
  }) async {
    if (newName != null) {
      await _sectionRepository.createSection(newName, workspaceId);
    } else {
      await _sectionRepository.setSection(workspaceId, sectionId);
    }
  }

  Future<void> deleteWorkspaceSection(String sectionId) =>
      _sectionRepository.removeSection(sectionId);

  void setSectionSort(WorkbenchSortBy sort) {
    _host.emitState(
      _host.readState().copyWith(
        viewPrefs: _host.readState().viewPrefs.copyWith(sectionSort: sort),
      ),
    );
    _host.persistViewPrefs();
  }

  void toggleSectionCollapsed(String? sectionId) {
    _host.emitState(
      toggleWorkbenchSectionCollapsedState(
        state: _host.readState(),
        sectionId: sectionId,
      ),
    );
    _host.persistViewPrefs();
  }
}
