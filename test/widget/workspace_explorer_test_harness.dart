part of 'workspace_explorer_test.dart';

Future<void> _pumpExplorer(
  WidgetTester tester,
  _FakeWorkspaceFileService service, {
  Workspace? workspace,
  ValueChanged<String>? onOpenFile,
  ValueChanged<String>? onOpenFilePermanently,
  ValueChanged<String>? onOpenFileInAlera,
  EditorSessionRegistry? registry,
  WorkspaceFolderOpener? folderOpener,
  ExternalEditorLauncher? externalEditorLauncher,
  GitBackend? gitBackend,
  String? focusedSourceControlRoot,
  Future<bool> Function(String relativePath)? onFocusSourceControlFolder,
  VoidCallback? onClearSourceControlRoot,
}) async {
  await tester.pumpWidget(
    _withWorkspaceFiles(
      service,
      registry: registry,
      folderOpener: folderOpener,
      externalEditorLauncher: externalEditorLauncher,
      gitBackend: gitBackend,
      child: MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 320,
            height: 480,
            child: WorkspaceExplorer(
              workspace: workspace ?? _workspace(),
              mode: .hideIgnored,
              onModeChanged: (_) {},
              showHiddenFiles: false,
              onShowHiddenFilesChanged: (_) {},
              onOpenFile: onOpenFile ?? (_) {},
              onOpenFilePermanently: onOpenFilePermanently,
              onOpenFileInAlera: onOpenFileInAlera,
              focusedSourceControlRoot: focusedSourceControlRoot,
              onFocusSourceControlFolder: onFocusSourceControlFolder,
              onClearSourceControlRoot: onClearSourceControlRoot,
              onPathMoved: (_, _) async {},
            ),
          ),
        ),
      ),
    ),
  );
  await tester.pumpAndSettle();
}

class const _WorkspaceExplorerModeHarness() extends StatefulWidget {
  @override
  State<_WorkspaceExplorerModeHarness> createState() =>
      _WorkspaceExplorerModeHarnessState();
}

class _WorkspaceExplorerModeHarnessState
    extends State<_WorkspaceExplorerModeHarness> {
  WorkspaceExplorerMode _mode = .hideIgnored;
  bool _showHiddenFiles = false;

  @override
  Widget build(BuildContext context) {
    return WorkspaceExplorer(
      workspace: _workspace(),
      mode: _mode,
      onModeChanged: (mode) => setState(() => _mode = mode),
      showHiddenFiles: _showHiddenFiles,
      onShowHiddenFilesChanged: (show) =>
          setState(() => _showHiddenFiles = show),
      onOpenFile: (_) {},
      onPathMoved: (_, _) async {},
    );
  }
}

Widget _withWorkspaceFiles(
  _FakeWorkspaceFileService service, {
  required Widget child,
  EditorSessionRegistry? registry,
  WorkspaceFolderOpener? folderOpener,
  ExternalEditorLauncher? externalEditorLauncher,
  GitBackend? gitBackend,
}) {
  return ProviderScope(
    overrides: [
      workspaceFileServiceProvider.overrideWithValue(service),
      gitBackendProvider.overrideWithValue(gitBackend ?? FakeGitBackend()),
      if (folderOpener != null)
        workspaceFolderOpenerProvider.overrideWithValue(folderOpener),
      installedExternalEditorsProvider.overrideWith(
        (ref) async => externalEditorLauncher == null
            ? const <ExternalEditorSpec>[]
            : <ExternalEditorSpec>[
                externalEditorSpecs[ExternalEditorKind.zed]!,
              ],
      ),
      resolvedExternalEditorProvider.overrideWith(
        (ref) async => externalEditorLauncher == null
            ? null
            : externalEditorSpecs[ExternalEditorKind.zed],
      ),
      if (externalEditorLauncher != null) ...[
        externalEditorLauncherProvider.overrideWithValue(
          externalEditorLauncher,
        ),
        externalEditorLauncherForProvider.overrideWith(
          (ref, kind) => externalEditorLauncher,
        ),
      ],
      if (registry != null)
        editorSessionRegistryProvider.overrideWithValue(registry),
    ],
    child: child,
  );
}

Widget _workspaceContextSidebar(Workspace workspace) {
  return WorkspaceContextSidebar(
    workspace: workspace,
    prefs: .defaults,
    onToggleVisible: () {},
    onResize: (_) {},
    onSetContextPanelTab: (_) {},
    onSetExplorerMode: (_) {},
    onSetShowHiddenFiles: (_) {},
    onSetGitDiffViewMode: (_) {},
    onSetGitDiffGroupMode: (_) {},
    onOpenFile: (_) {},
    onOpenGitDiff: ({
      relativePath,
      area,
      gitDiffRoot,
      required scope,
      bool preview = false,
    }) async {},
    onOpenGitCommitDiff: ({
      relativePath,
      oldPath,
      required scope,
      gitDiffRoot,
      required commitOid,
      parentOid,
      required compareRef,
      subject,
      message,
      bool preview = false,
    }) async {},
    onOpenSearchMatch: (_) {},
    onPathMoved: (_, _) async {},
  );
}

class _FakeWorkspaceFolderOpener() extends WorkspaceFolderOpener {
  this : super(processRunner: _NoopProcessRunner(), platform: .macos);

  final List<String> revealedPaths = <String>[];
  final List<String> defaultOpenedPaths = <String>[];

  @override
  Future<WorkspaceFolderOpenResult> reveal(String path) async {
    revealedPaths.add(path);
    return const WorkspaceFolderOpenResult.success();
  }

  @override
  Future<WorkspaceFolderOpenResult> openWithDefaultApplication(
    String path,
  ) async {
    defaultOpenedPaths.add(path);
    return const WorkspaceFolderOpenResult.success();
  }
}

class _FakeExternalEditorLauncher implements ExternalEditorLauncher {
  final List<String> workspacePaths = <String>[];
  final List<ExternalEditorOpenRequest> fileRequests =
      <ExternalEditorOpenRequest>[];

  @override
  Future<ExternalEditorAvailability> checkAvailability() async =>
      const ExternalEditorAvailability(available: true);

  @override
  Future<bool> isInstalled() async => true;

  @override
  Future<ExternalEditorLaunchResult> openFile(
    ExternalEditorOpenRequest request,
  ) async {
    fileRequests.add(request);
    return ExternalEditorLaunchResultFactories.opened;
  }

  @override
  Future<ExternalEditorLaunchResult> openFiles(
    ExternalEditorOpenFilesRequest request,
  ) async => ExternalEditorLaunchResultFactories.opened;

  @override
  Future<ExternalEditorLaunchResult> openWorkspace(String workspacePath) async {
    workspacePaths.add(workspacePath);
    return ExternalEditorLaunchResultFactories.opened;
  }
}

class _NoopProcessRunner implements ProcessRunner {
  @override
  Future<ProcessRunOutput> run(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
  }) async {
    return const ProcessRunOutput(exitCode: 0, stdout: '', stderr: '');
  }

  @override
  Future<StartedProcess> start(
    String executable,
    List<String> arguments, {
    String? workingDirectory,
    Map<String, String>? environment,
    bool includeParentEnvironment = true,
  }) {
    throw UnimplementedError();
  }
}

Workspace _workspace({
  String id = 'workspace-1',
  String name = 'alera',
  String path = '/repo/alera',
}) {
  final now = DateTime.utc(2026);
  return Workspace(
    id: id,
    projectId: 'project-1',
    name: name,
    path: path,
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
}

native.WorkspaceFileEntry _file(
  String relativePath, {
  native.WorkspaceFileGitStatus? gitStatus,
  bool isHidden = false,
}) {
  return _entry(
    relativePath: relativePath,
    kind: native.WorkspaceFileKind.file,
    hasChildrenHint: false,
    gitStatus: gitStatus,
    isHidden: isHidden,
  );
}

native.WorkspaceFileEntry _directory(
  String relativePath, {
  required bool hasChildrenHint,
  native.WorkspaceFileGitStatus? gitStatus,
  bool isHidden = false,
}) {
  return _entry(
    relativePath: relativePath,
    kind: native.WorkspaceFileKind.directory,
    hasChildrenHint: hasChildrenHint,
    gitStatus: gitStatus,
    isHidden: isHidden,
  );
}

native.WorkspaceFileEntry _entry({
  required String relativePath,
  required native.WorkspaceFileKind kind,
  required bool hasChildrenHint,
  native.WorkspaceFileGitStatus? gitStatus,
  bool isHidden = false,
}) {
  return native.WorkspaceFileEntry(
    relativePath: relativePath,
    name: relativePath.split('/').last,
    kind: kind,
    size: .zero,
    modifiedMillis: 0,
    contentToken: '$relativePath-token',
    isIgnored: false,
    isHidden: isHidden,
    isSymlink: false,
    isProtected: false,
    hasChildrenHint: hasChildrenHint,
    gitStatus: gitStatus,
  );
}
