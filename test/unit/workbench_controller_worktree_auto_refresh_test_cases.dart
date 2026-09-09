part of 'workbench_controller_test.dart';

void _registerWorkbenchControllerWorktreeAutoRefreshTests() {
  test(
    'external git worktree metadata changes refresh workspace state',
    () async {
      final worktreesMetadata = Directory(
        p.join(_harness.project.repoPath, '.git', 'worktrees'),
      )..createSync(recursive: true);

      await _controller.bootstrap();
      await _flushUntil(
        () => _controller.state.workspacesFor(_harness.project.id).length == 1,
      );

      final externalPath = p.join(
        _harness.tempDir.path,
        'external-live-worktree',
      );
      Directory(externalPath).createSync(recursive: true);
      _harness.gitBackend.liveBranchByPath = <String, String>{
        _harness.project.repoPath: 'main',
        externalPath: 'feature/external-live',
      };

      final externalMetadata = Directory(
        p.join(worktreesMetadata.path, 'external-live'),
      )..createSync(recursive: true);
      File(p.join(externalMetadata.path, 'HEAD'))
          .writeAsStringSync('ref: refs/heads/feature/external-live\n');

      await _waitForWorkbenchCondition(
        () => _controller.state.workspacesFor(_harness.project.id).length == 2,
      );
      final imported = _controller.state
          .workspacesFor(_harness.project.id)
          .firstWhere((workspace) => !workspace.isMain);
      expect(imported.path, externalPath);
      expect(imported.branch, 'feature/external-live');

      _harness.gitBackend.liveBranchByPath = <String, String>{
        _harness.project.repoPath: 'main',
      };
      externalMetadata.deleteSync(recursive: true);

      await _waitForWorkbenchCondition(
        () => _controller.state.workspacesFor(_harness.project.id).length == 1,
      );
      expect(
        _controller.state.workspacesFor(_harness.project.id).single.isMain,
        isTrue,
      );
    },
  );
}

Future<void> _waitForWorkbenchCondition(
  bool Function() condition, {
  Duration timeout = const Duration(seconds: 3),
}) async {
  final deadline = DateTime.now().add(timeout);
  while (!condition()) {
    if (DateTime.now().isAfter(deadline)) {
      throw StateError('condition was not met before timeout');
    }
    await Future<void>.delayed(const Duration(milliseconds: 25));
  }
}
