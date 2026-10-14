part of 'workspace_tab_service_test.dart';

void _registerWorkspaceTabBranchCompareTests() {
  test(
    'branch comparison persists merge-base snapshot and reuses its tab',
    () async {
      final repository = _FakeWorkbenchRepository();
      final service = WorkspaceTabService(
        repository: repository,
        now: () => DateTime.utc(2026, 10, 14),
      );

      final first = await service.openOrCreateGitBranchCompareTab(
        workspaceId: 'workspace-1',
        baseRef: 'main',
        headRef: 'feature/mobile',
        headOid: 'head-1',
        mergeBaseOid: 'common-1',
      );

      expect(first.kind, WorkspaceTabKind.gitDiff);
      expect(first.gitDiffSource, WorkspaceGitDiffSource.branchCompare);
      expect(first.gitDiffScope, WorkspaceGitDiffScope.all);
      expect(first.gitBranchCompareBaseRef, 'main');
      expect(first.gitBranchCompareHeadRef, 'feature/mobile');
      expect(first.gitBranchCompareMode, 'mergeBase');
      expect(first.gitDiffCommitOid, 'head-1');
      expect(first.gitDiffParentOid, 'common-1');
      expect(first.isPreview, isFalse);

      final reopened = await service.openOrCreateGitBranchCompareTab(
        workspaceId: 'workspace-1',
        baseRef: 'main',
        headRef: 'feature/mobile',
        headOid: 'head-2',
        mergeBaseOid: 'common-2',
      );

      expect(reopened.id, first.id);
      expect(
        reopened.gitDiffCommitOid,
        'head-1',
        reason: 'Reopening must not silently move a pinned comparison',
      );

      final updated = await service.updateGitBranchCompareSnapshot(
        tabId: first.id,
        headOid: 'head-2',
        mergeBaseOid: 'common-2',
      );
      expect(updated.id, first.id);
      expect(updated.gitDiffCommitOid, 'head-2');
      expect(updated.gitDiffParentOid, 'common-2');

      final reversed = await service.openOrCreateGitBranchCompareTab(
        workspaceId: 'workspace-1',
        baseRef: 'feature/mobile',
        headRef: 'main',
        headOid: 'head-main',
        mergeBaseOid: 'common-2',
      );
      expect(reversed.id, isNot(first.id));
      expect(repository.tabs, hasLength(2));
    },
  );

  test('branch comparison rejects identical refs', () async {
    final service = WorkspaceTabService(repository: _FakeWorkbenchRepository());
    await expectLater(
      service.openOrCreateGitBranchCompareTab(
        workspaceId: 'workspace-1',
        baseRef: 'main',
        headRef: 'main',
        headOid: 'oid',
        mergeBaseOid: 'oid',
      ),
      throwsStateError,
    );
  });
}
