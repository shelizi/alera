part of 'workspace_service_test.dart';

void _reconciliationTests() {
  test(
    'reconcile keeps the main workspace and removes missing linked ones',
    () async {
      gitBackend.headBranch = 'main';
      gitBackend.sourceBranches = <String>['main'];
      final mainWorkspace = await service.ensureMainWorkspace(project);
      final linkedWorkspace = (await service.createLinkedWorkspace(
        project: project,
        sourceBranch: 'main',
        newBranchName: 'feature/remove-me',
      )).workspace;
      gitBackend.liveBranchByPath = <String, String>{project.repoPath: 'main'};

      final workspaces = await service.reconcile(project);

      expect(workspaces.map((workspace) => workspace.id), <String>[
        mainWorkspace.id,
      ]);
      expect(
        workspaces
            .singleWhere((workspace) => workspace.id == mainWorkspace.id)
            .status,
        WorkspaceStatus.active,
      );
      expect(
        repository.workspaces.any(
          (workspace) => workspace.id == linkedWorkspace.id,
        ),
        isFalse,
      );
    },
  );

  test(
    'reconcile keeps linked workspaces when git worktree list fails',
    () async {
      gitBackend.headBranch = 'main';
      gitBackend.sourceBranches = <String>['main'];
      final mainWorkspace = await service.ensureMainWorkspace(project);
      final linkedWorkspace = (await service.createLinkedWorkspace(
        project: project,
        sourceBranch: 'main',
        newBranchName: 'feature/keep-me',
      )).workspace;
      gitBackend.worktreeListFails = true;

      final workspaces = await service.reconcile(project);

      expect(
        workspaces.map((workspace) => workspace.id),
        containsAll(<String>[mainWorkspace.id, linkedWorkspace.id]),
      );
    },
  );

  test(
    'reconcile updates linked workspace metadata from live worktrees',
    () async {
      gitBackend.headBranch = 'main';
      gitBackend.sourceBranches = <String>['main'];
      await service.ensureMainWorkspace(project);
      final linkedWorkspace = (await service.createLinkedWorkspace(
        project: project,
        sourceBranch: 'main',
        newBranchName: 'feature/live-rename',
      )).workspace;
      gitBackend.liveBranchByPath = <String, String>{
        project.repoPath: 'main',
        linkedWorkspace.path: 'feature/live-updated',
      };

      final workspaces = await service.reconcile(project);

      expect(
        workspaces
            .singleWhere((workspace) => workspace.id == linkedWorkspace.id)
            .branch,
        'feature/live-updated',
      );
    },
  );

  test('reconcile skips pruning when the live list does not include the main workspace', () async {
    gitBackend.headBranch = 'main';
    gitBackend.sourceBranches = <String>['main'];
    await service.ensureMainWorkspace(project);
    final linkedWorkspace = (await service.createLinkedWorkspace(
      project: project,
      sourceBranch: 'main',
      newBranchName: 'feature/cannot-prune',
    )).workspace;
    gitBackend.liveBranchByPath = <String, String>{
      linkedWorkspace.path: 'feature/cannot-prune',
    };

    final workspaces = await service.reconcile(project);

    expect(
      workspaces.map((workspace) => workspace.id),
      contains(linkedWorkspace.id),
    );
  });

  test(
    'reconcile keeps only the primary workspace for folder projects',
    () async {
      final folderProject = project.copyWith(kind: .folder);
      final linkedWorkspace = Workspace(
        id: 'linked-folder-workspace',
        projectId: folderProject.id,
        name: 'Linked',
        branch: 'feature/remove-me',
        path: p.join(tempDir.path, 'linked-folder-workspace'),
        createdAt: .utc(2026, 5, 20),
        updatedAt: .utc(2026, 5, 20),
        kind: .linked,
        status: .active,
      );
      await repository.upsertWorkspace(linkedWorkspace);

      final workspaces = await service.reconcile(folderProject);

      expect(workspaces, hasLength(1));
      expect(workspaces.single.isMain, isTrue);
      expect(workspaces.single.branch, isNull);
      expect(
        repository.workspaces.any(
          (workspace) => workspace.id == linkedWorkspace.id,
        ),
        isFalse,
      );
      expect(gitBackend.calls, isEmpty);
    },
  );

  test('reconcile automatically imports existing live worktrees into linked workspaces', () async {
    gitBackend.headBranch = 'main';
    gitBackend.sourceBranches = <String>['main'];
    final mainWorkspace = await service.ensureMainWorkspace(project);

    final extPath = p.join(tempDir.path, 'ext-worktree');
    Directory(extPath).createSync(recursive: true);

    gitBackend.liveBranchByPath = <String, String>{
      project.repoPath: 'main',
      extPath: 'feature/existing-worktree',
    };

    final workspaces = await service.reconcile(project);

    expect(workspaces, hasLength(2));
    expect(workspaces.map((w) => w.id), contains(mainWorkspace.id));

    final imported = workspaces.firstWhere((w) => w.id != mainWorkspace.id);
    expect(imported.name, 'feature/existing-worktree');
    expect(imported.branch, 'feature/existing-worktree');
    expect(imported.path, extPath);
    expect(imported.kind, WorkspaceKind.linked);
    expect(imported.status, WorkspaceStatus.active);
    expect(imported.reusesExistingBranch, isTrue);
    expect(imported.sourceBranch, isNull);

    expect(repository.workspaces.any((w) => w.id == imported.id), isTrue);
  });

  test(
    'reconcile names detached HEAD worktrees using directory basename',
    () async {
      gitBackend.headBranch = 'main';
      gitBackend.sourceBranches = <String>['main'];
      await service.ensureMainWorkspace(project);

      final detachedPath = p.join(tempDir.path, 'custom-detached-wt');
      Directory(detachedPath).createSync(recursive: true);

      gitBackend.liveBranchByPath = <String, String>{
        project.repoPath: 'main',
        detachedPath: 'HEAD',
      };

      final workspaces = await service.reconcile(project);
      final imported = workspaces.firstWhere((w) => !w.isMain);

      expect(imported.name, 'custom-detached-wt');
      expect(imported.branch, 'HEAD');
      expect(imported.path, detachedPath);
      expect(imported.reusesExistingBranch, isTrue);
    },
  );

  test('reconcile does not duplicate already tracked linked workspaces on subsequent runs', () async {
    gitBackend.headBranch = 'main';
    gitBackend.sourceBranches = <String>['main'];
    await service.ensureMainWorkspace(project);

    final extPath = p.join(tempDir.path, 'ext-worktree-2');
    Directory(extPath).createSync(recursive: true);

    gitBackend.liveBranchByPath = <String, String>{
      project.repoPath: 'main',
      extPath: 'feature/existing-2',
    };

    final firstRun = await service.reconcile(project);
    expect(firstRun, hasLength(2));
    final importedId = firstRun.firstWhere((w) => !w.isMain).id;

    final secondRun = await service.reconcile(project);
    expect(secondRun, hasLength(2));
    expect(secondRun.firstWhere((w) => !w.isMain).id, importedId);
  });
}
