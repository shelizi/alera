part of 'workbench_controller_test.dart';

Future<void> _flush() => Future.pause(.zero);
Future<Workspace> _selectMainWorkspace(
  WorkbenchController controller,
  _WorkbenchHarness harness,
) async {
  await _flushUntil(
    () => controller.state.workspacesFor(harness.project.id).isNotEmpty,
  );
  final workspace = controller.state.workspacesFor(harness.project.id).single;
  await controller.selectWorkspace(
    project: harness.project,
    workspace: workspace,
  );
  await _flush();
  return workspace;
}

Future<void> _flushUntil(bool Function() condition, {int attempts = 20}) async {
  for (var i = 0; i < attempts; i += 1) {
    if (condition()) {
      return;
    }
    await _flush();
  }
  throw StateError('condition was not met');
}

class _WorkbenchHarness([ManagedWorkspaceRuntime? runtime]) {
  this {
    tempDir = Directory.systemTemp.createTempSync(
      'alera-workbench-_controller-',
    );
    final repoPath = p.join(tempDir.path, 'repo');
    Directory(repoPath).createSync(recursive: true);
    project = Project(
      id: 'project-1',
      name: 'Alera',
      repoPath: repoPath,
      createdAt: .utc(2026, 5, 22),
      updatedAt: .utc(2026, 5, 22),
    );
    projectRepository = _FakeProjectRepository(<Project>[project]);
    workbenchRepository = _FakeWorkbenchRepository();
    workspaceGraphRepository = _FakeWorkspaceGraphRepository();
    gitBackend = FakeGitBackend()
      ..sourceBranches = <String>['main', 'origin/main']
      ..includeQueriedRepoAsMain = true;
    viewPrefsRepository = _FakeWorkbenchViewPrefsRepository();
    final projectService = ProjectService(gitBackend);
    final projectsService = ProjectsService(
      projectService: projectService,
      projectRepository: projectRepository,
    );
    final workspaceTabService = WorkspaceTabService(
      repository: workbenchRepository,
      now: () => DateTime.utc(2026, 5, 22, 1),
    );
    worktreeSetupRunner = _FakeWorktreeSetupRunner();
    final settings = AleraSettings.defaults.copyWith(
      general: AleraSettings.defaults.general.copyWith(
        workspaceDirectory: p.join(tempDir.path, 'workspaces'),
      ),
    );
    terminalRuntime = _FakeTerminalRuntime();
    container = ProviderContainer(
      overrides: [
        gitBackendProvider.overrideWithValue(gitBackend),
        projectRepositoryProvider.overrideWithValue(projectRepository),
        workbenchRepositoryProvider.overrideWithValue(workbenchRepository),
        workspaceGraphRepositoryProvider.overrideWithValue(
          workspaceGraphRepository,
        ),
        projectServiceProvider.overrideWithValue(projectService),
        projectConfigServiceProvider.overrideWithValue(
          ProjectConfigService(
            repository: FakeProjectConfigRepository(),
            fileStore: FakeProjectConfigFileStore(),
          ),
        ),
        projectsServiceProvider.overrideWithValue(projectsService),
        workspaceTabServiceProvider.overrideWithValue(workspaceTabService),
        worktreeSetupRunnerProvider.overrideWithValue(worktreeSetupRunner),
        managedWorkspaceRuntimeProvider.overrideWithValue(runtime),
        workbenchViewPrefsRepositoryProvider.overrideWithValue(
          viewPrefsRepository,
        ),
        settingsControllerProvider.overrideWithValue(settings),
        terminalRuntimeProvider.overrideWithValue(terminalRuntime),
        terminalRuntimeBindingsProvider.overrideWithValue(terminalRuntime),
        agentHookReceiverProvider.overrideWithValue(hookReceiver),
      ],
    );
    _controller = container.read(workbenchControllerProvider.notifier);
  }
  late final Directory tempDir;
  late final Project project;
  late final _FakeProjectRepository projectRepository;
  late final _FakeWorkbenchRepository workbenchRepository;
  late final _FakeWorkspaceGraphRepository workspaceGraphRepository;
  late final FakeGitBackend gitBackend;
  late final _FakeWorkbenchViewPrefsRepository viewPrefsRepository;
  late final _FakeWorktreeSetupRunner worktreeSetupRunner;
  late final _FakeTerminalRuntime terminalRuntime;
  final hookReceiver = _FakeAgentHookReceiver();
  late final ProviderContainer container;
  late final WorkbenchController _controller;
  Future<Project> addProject(String id, String name) async {
    final repoPath = p.join(tempDir.path, id);
    Directory(repoPath).createSync(recursive: true);
    final newProject = Project(
      id: id,
      name: name,
      repoPath: repoPath,
      createdAt: .utc(2026, 5, 22),
      updatedAt: .utc(2026, 5, 22),
    );
    await projectRepository.add(newProject);
    return newProject;
  }

  Future<void> dispose() async {
    container.dispose();
    await terminalRuntime.dispose();
    await projectRepository.dispose();
    await workbenchRepository.dispose();
    if (tempDir.existsSync()) {
      tempDir.deleteSync(recursive: true);
    }
  }
}

class _FakeWorkspaceGraphRepository implements WorkspaceGraphRepository {
  final tags = <WorkspaceTag>[];
  final relations = <WorkspaceRelation>[];
  final assignedTags = <({String workspaceId, String tagId})>[];
  final unassignedTags = <({String workspaceId, String tagId})>[];
  final tagAssignmentsByWorkspace = <String, Set<String>>{};
  final linkedWorkspaces =
      <({String parentWorkspaceId, String childWorkspaceId})>[];
  final unlinkedWorkspaces =
      <({String parentWorkspaceId, String childWorkspaceId})>[];
  final linkErrorsByParent = <String, Object>{};
  Object? linkError;

  @override
  Future<List<WorkspaceTag>> listTags() async => List<WorkspaceTag>.of(tags);

  @override
  Future<WorkspaceTag> upsertTag(WorkspaceTag tag) async {
    final duplicate = tags
        .where(
          (candidate) =>
              candidate.id != tag.id &&
              candidate.name.toLowerCase() == tag.name.toLowerCase(),
        )
        .firstOrNull;
    if (duplicate != null) {
      return duplicate;
    }
    tags.removeWhere((candidate) => candidate.id == tag.id);
    tags.add(tag);
    return tag;
  }

  @override
  Future<void> removeTag(String tagId) async {
    tags.removeWhere((tag) => tag.id == tagId);
  }

  @override
  Future<void> assignTag({
    required String workspaceId,
    required String tagId,
  }) async {
    assignedTags.add((workspaceId: workspaceId, tagId: tagId));
    tagAssignmentsByWorkspace
        .putIfAbsent(workspaceId, () => <String>{})
        .add(tagId);
  }

  @override
  Future<void> unassignTag({
    required String workspaceId,
    required String tagId,
  }) async {
    unassignedTags.add((workspaceId: workspaceId, tagId: tagId));
    tagAssignmentsByWorkspace[workspaceId]?.remove(tagId);
  }

  @override
  Future<List<WorkspaceRelation>> listRelations() async {
    return List<WorkspaceRelation>.of(relations);
  }

  @override
  Future<WorkspaceRelation> linkWorkspaces({
    required String parentWorkspaceId,
    required String childWorkspaceId,
  }) async {
    if (linkErrorsByParent.remove(parentWorkspaceId) case final Object error) {
      throw error;
    }
    if (linkError case final Object error) {
      throw error;
    }
    linkedWorkspaces.add((
      parentWorkspaceId: parentWorkspaceId,
      childWorkspaceId: childWorkspaceId,
    ));
    final relation = WorkspaceRelation(
      id: 'relation-${linkedWorkspaces.length}',
      parentWorkspaceId: parentWorkspaceId,
      parentInstanceId: 'instance-$parentWorkspaceId',
      childWorkspaceId: childWorkspaceId,
      childInstanceId: 'instance-$childWorkspaceId',
      createdAt: .utc(2026, 5, 22),
    );
    relations.add(relation);
    return relation;
  }

  @override
  Future<void> unlinkWorkspaces({
    required String parentWorkspaceId,
    required String childWorkspaceId,
  }) async {
    unlinkedWorkspaces.add((
      parentWorkspaceId: parentWorkspaceId,
      childWorkspaceId: childWorkspaceId,
    ));
    relations.removeWhere(
      (relation) =>
          relation.parentWorkspaceId == parentWorkspaceId &&
          relation.childWorkspaceId == childWorkspaceId,
    );
  }
}

class _FakeWorktreeSetupRunner implements WorktreeSetupRunner {
  WorktreeSetupReport report = .empty;
  final List<({Project project, Workspace workspace, ProjectConfig config})>
  calls = <({Project project, Workspace workspace, ProjectConfig config})>[];

  @override
  Future<WorktreeSetupReport> run({
    required Project project,
    required Workspace workspace,
    required ProjectConfig config,
  }) async {
    calls.add((project: project, workspace: workspace, config: config));
    return report;
  }
}

class _FakeProjectRepository(final List<Project> _projects)
    implements ProjectRepository {
  StreamController<List<Project>> _projectsController =
      StreamController<List<Project>>.broadcast();
  Object? listAllError;
  Object? addError;
  Object? updateError;
  Object? removeError;

  @override
  Future<List<Project>> listAll() async {
    if (listAllError case final Object error) {
      throw error;
    }
    return List<Project>.from(_projects);
  }

  @override
  Stream<List<Project>> watchAll() => _projectsController.stream;

  bool get hasWatcher => _projectsController.hasListener;

  void killWatcher() {
    final previous = _projectsController;
    _projectsController = StreamController<List<Project>>.broadcast();
    previous.addError(StateError('project watcher connection closed'));
    unawaited(previous.close());
  }

  Future<void> dispose() => _projectsController.close();

  @override
  Future<Project> add(Project project) async {
    if (addError case final Object error) {
      throw error;
    }
    _projects.add(project);
    _projectsController.add(List<Project>.from(_projects));
    return project;
  }

  @override
  Future<Project> update(Project project) async {
    if (updateError case final Object error) {
      throw error;
    }
    final index = _projects.indexWhere((entry) => entry.id == project.id);
    if (index == -1) {
      _projects.add(project);
    } else {
      _projects[index] = project;
    }
    _projectsController.add(List<Project>.from(_projects));
    return project;
  }

  @override
  Future<void> remove(String projectId) async {
    if (removeError case final Object error) {
      throw error;
    }
    _projects.removeWhere((project) => project.id == projectId);
    _projectsController.add(List<Project>.from(_projects));
  }
}
