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

  test(
    'reconcile collapses duplicate metadata for the project root workspace',
    () async {
      gitBackend.headBranch = 'main';
      final canonical = Workspace(
        id: 'workspace-main',
        projectId: project.id,
        name: 'main',
        branch: 'main',
        path: project.repoPath,
        createdAt: .utc(2026, 5, 19),
        updatedAt: .utc(2026, 5, 19),
        kind: .main,
        status: .active,
      );
      final duplicate = Workspace(
        id: 'workspace-default',
        projectId: project.id,
        name: 'Default',
        branch: 'main',
        path: project.repoPath,
        createdAt: .utc(2026, 5, 20),
        updatedAt: .utc(2026, 5, 20),
        kind: .linked,
        status: .active,
        reusesExistingBranch: true,
      );
      await repository.upsertWorkspace(canonical);
      await repository.upsertWorkspace(duplicate);
      gitBackend.liveBranchByPath = <String, String>{project.repoPath: 'main'};

      final workspaces = await service.reconcile(project);

      expect(workspaces, hasLength(1));
      expect(workspaces.single.id, canonical.id);
      expect(workspaces.single.kind, WorkspaceKind.main);
      expect(workspaces.single.path, project.repoPath);
    },
  );

  test('reconcile promotes legacy root metadata instead of creating a duplicate main workspace', () async {
    gitBackend.headBranch = 'main';
    final legacy = Workspace(
      id: 'workspace-legacy-default',
      projectId: project.id,
      name: 'Default',
      branch: 'main',
      path: project.repoPath,
      createdAt: .utc(2026, 5, 19),
      updatedAt: .utc(2026, 5, 19),
      kind: .linked,
      status: .active,
      reusesExistingBranch: true,
    );
    await repository.upsertWorkspace(legacy);
    gitBackend.liveBranchByPath = <String, String>{project.repoPath: 'main'};

    final workspaces = await service.reconcile(project);

    expect(workspaces, hasLength(1));
    expect(workspaces.single.id, legacy.id);
    expect(workspaces.single.kind, WorkspaceKind.main);
    expect(workspaces.single.name, 'main');
    expect(workspaces.single.reusesExistingBranch, isFalse);
  });

  test(
    'reconcile demotes an extra main record for a distinct live worktree',
    () async {
      gitBackend.headBranch = 'main';
      final canonical = Workspace(
        id: 'workspace-main',
        projectId: project.id,
        name: 'main',
        branch: 'main',
        path: project.repoPath,
        createdAt: .utc(2026, 5, 19),
        updatedAt: .utc(2026, 5, 19),
        kind: .main,
        status: .active,
      );
      final linkedPath = p.join(tempDir.path, 'legacy-linked-worktree');
      Directory(linkedPath).createSync(recursive: true);
      final duplicateMain = Workspace(
        id: 'workspace-extra-main',
        projectId: project.id,
        name: 'feature/legacy',
        branch: 'feature/legacy',
        path: linkedPath,
        createdAt: .utc(2026, 5, 20),
        updatedAt: .utc(2026, 5, 20),
        kind: .main,
        status: .active,
      );
      await repository.upsertWorkspace(canonical);
      await repository.upsertWorkspace(duplicateMain);
      gitBackend.liveBranchByPath = <String, String>{
        project.repoPath: 'main',
        linkedPath: 'feature/legacy',
      };

      final workspaces = await service.reconcile(project);

      expect(workspaces.where((workspace) => workspace.isMain), hasLength(1));
      expect(
        workspaces
            .singleWhere((workspace) => workspace.id == canonical.id)
            .isMain,
        isTrue,
      );
      final linked = workspaces.singleWhere(
        (workspace) => workspace.id == duplicateMain.id,
      );
      expect(linked.kind, WorkspaceKind.linked);
      expect(linked.reusesExistingBranch, isTrue);
    },
  );

  test('reconcile does not adopt the main worktree when the project path uses a verbatim prefix', () async {
    // libgit2 reports plain `E:\...` workdir paths while a stored project
    // path may carry the verbatim `\\?\` prefix; both must compare equal.
    final verbatimProject = project.copyWith(
      repoPath: '\\\\?\\${project.repoPath}',
    );
    gitBackend.headBranch = 'main';
    final mainWorkspace = await service.ensureMainWorkspace(verbatimProject);
    gitBackend.liveBranchByPath = <String, String>{project.repoPath: 'main'};

    final workspaces = await service.reconcile(verbatimProject);

    expect(workspaces, hasLength(1));
    expect(workspaces.single.id, mainWorkspace.id);
    expect(workspaces.single.isMain, isTrue);
  }, skip: !Platform.isWindows);

  test('reconcile removes a phantom linked record that points at the main checkout via a plain path', () async {
    final verbatimProject = project.copyWith(
      repoPath: '\\\\?\\${project.repoPath}',
    );
    gitBackend.headBranch = 'main';
    final mainWorkspace = await service.ensureMainWorkspace(verbatimProject);
    final phantom = Workspace(
      id: 'workspace-phantom-main',
      projectId: verbatimProject.id,
      name: 'main',
      branch: 'main',
      path: project.repoPath,
      createdAt: .utc(2026, 5, 20),
      updatedAt: .utc(2026, 5, 20),
      kind: .linked,
      status: .active,
      reusesExistingBranch: true,
    );
    await repository.upsertWorkspace(phantom);
    gitBackend.liveBranchByPath = <String, String>{project.repoPath: 'main'};

    final workspaces = await service.reconcile(verbatimProject);

    expect(workspaces, hasLength(1));
    expect(workspaces.single.id, mainWorkspace.id);
  }, skip: !Platform.isWindows);

  test('reconcile moves tabs from a same-path duplicate record onto the main workspace', () async {
    final verbatimProject = project.copyWith(
      repoPath: '\\\\?\\${project.repoPath}',
    );
    gitBackend.headBranch = 'main';
    final mainWorkspace = await service.ensureMainWorkspace(verbatimProject);
    final phantom = Workspace(
      id: 'workspace-phantom-tabs',
      projectId: verbatimProject.id,
      name: 'main',
      branch: 'main',
      path: project.repoPath,
      createdAt: .utc(2026, 5, 20),
      updatedAt: .utc(2026, 5, 20),
      kind: .linked,
      status: .active,
      reusesExistingBranch: true,
    );
    await repository.upsertWorkspace(phantom);
    await repository.upsertWorkspaceTab(
      WorkspaceTabRecord(
        id: 'tab-phantom',
        workspaceId: phantom.id,
        title: 'phantom terminal',
        createdAt: .utc(2026, 5, 20),
        updatedAt: .utc(2026, 5, 20),
      ),
    );
    gitBackend.liveBranchByPath = <String, String>{project.repoPath: 'main'};

    await service.reconcile(verbatimProject);

    final tabs = await repository.listWorkspaceTabs(mainWorkspace.id);
    expect(tabs.map((tab) => tab.id), contains('tab-phantom'));
    expect(
      repository.workspaces.any((workspace) => workspace.id == phantom.id),
      isFalse,
    );
  }, skip: !Platform.isWindows);
}
