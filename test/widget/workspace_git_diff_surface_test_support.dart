part of 'workspace_git_diff_surface_test.dart';

Future<void> _pumpDiffSurface(
  WidgetTester tester, {
  required FakeGitBackend backend,
  WorkbenchController? controller,
  WorkspaceTabRecord? tab,
  ReadingDiffService? readingDiffService,
  SettingsController? settingsController,
  WorkspaceFileService? workspaceFileService,
}) {
  return tester.pumpWidget(
    ProviderScope(
      overrides: [
        gitBackendProvider.overrideWithValue(backend),
        workspaceFileServiceProvider.overrideWithValue(
          workspaceFileService ?? _DiffEncodingFileService(),
        ),
        if (settingsController == null)
          settingsControllerProvider.overrideWithValue(.defaults)
        else
          settingsControllerProvider.overrideWith(() => settingsController),
        if (readingDiffService != null)
          readingDiffServiceProvider.overrideWithValue(readingDiffService),
        if (controller != null)
          workbenchControllerProvider.overrideWith(() => controller),
      ],
      child: MaterialApp(
        theme: ThemeData(splashFactory: InkRipple.splashFactory),
        home: Scaffold(
          body: SizedBox(
            width: 900,
            height: 600,
            child: WorkspaceGitDiffSurface(
              workspace: _workspace(),
              tab: tab ?? _diffTab(),
            ),
          ),
        ),
      ),
    ),
  );
}

typedef _DiffDecodeCallback = FutureOr<native.WorkspaceDecodedText> Function(
  List<int> bytes,
  native.WorkspaceTextEncoding? encoding,
);

class _DiffEncodingFileService extends WorkspaceFileService {
  _DiffEncodingFileService({this._decode});

  final _DiffDecodeCallback? _decode;
  final List<({List<int> bytes, native.WorkspaceTextEncoding? encoding})>
  decodeCalls = <({List<int> bytes, native.WorkspaceTextEncoding? encoding})>[];

  @override
  Future<native.WorkspaceDecodedText> decodeTextBytes({
    required List<int> bytes,
    native.WorkspaceTextEncoding? encoding,
  }) async {
    decodeCalls.add((bytes: List<int>.from(bytes), encoding: encoding));
    final decode = _decode;
    if (decode != null) return await decode(bytes, encoding);
    return native.WorkspaceDecodedText(
      content: utf8.decode(bytes),
      encoding: encoding ?? native.WorkspaceTextEncoding.utf8,
    );
  }
}

class _EditableDiffFileService extends _DiffEncodingFileService {
  _EditableDiffFileService({required String content})
    : current = native.WorkspaceEditorTextFile(
        rawContent: content,
        displayContent: content,
        contentToken: 'token-1',
        modifiedMillis: 1,
        size: BigInt.from(content.length),
        encoding: native.WorkspaceTextEncoding.utf8,
      );

  native.WorkspaceEditorTextFile current;
  int readCount = 0;
  final List<
    ({
      String currentDisplayContent,
      String? expectedContentToken,
      bool overwriteIfChanged,
      native.WorkspaceTextEncoding encoding,
    })
  >
  writes = [];

  @override
  Future<native.WorkspaceEditorTextFile> readEditorTextFile({
    required String workspacePath,
    required String relativePath,
    required int tabSize,
    native.WorkspaceTextEncoding? encoding,
  }) async {
    readCount += 1;
    return current;
  }

  @override
  Future<native.WorkspaceEditorTextFile> writeEditorTextFile({
    required String workspacePath,
    required String relativePath,
    required String currentDisplayContent,
    required String? originalRawContent,
    required String? originalDisplayContent,
    required String? expectedContentToken,
    required bool overwriteIfChanged,
    required int tabSize,
    required native.WorkspaceTextEncoding encoding,
  }) async {
    writes.add((
      currentDisplayContent: currentDisplayContent,
      expectedContentToken: expectedContentToken,
      overwriteIfChanged: overwriteIfChanged,
      encoding: encoding,
    ));
    current = native.WorkspaceEditorTextFile(
      rawContent: currentDisplayContent,
      displayContent: currentDisplayContent,
      contentToken: 'token-${writes.length + 1}',
      modifiedMillis: writes.length + 1,
      size: BigInt.from(currentDisplayContent.length),
      encoding: encoding,
    );
    return current;
  }
}

class _MutableSettingsController(final AleraSettings _settings)
    extends SettingsController {
  @override
  AleraSettings build() => _settings;

  void setAiAssistEnabled(bool enabled) {
    state = state.copyWith(aiAssist: state.aiAssist.copyWith(enabled: enabled));
  }
}

IconButton _openFileButton(WidgetTester tester) {
  final finder = find.ancestor(
    of: find.byIcon(AleraIcons.external),
    matching: find.byType(IconButton),
  );
  return tester.widget<IconButton>(finder);
}

Workspace _workspace() {
  final now = DateTime.utc(2026, 6, 6);
  return Workspace(
    id: 'workspace-1',
    projectId: 'project-1',
    name: 'Main',
    path: '/tmp/project',
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
}

WorkspaceTabRecord _diffTab({
  WorkspaceGitDiffSource source = WorkspaceGitDiffSource.workingTree,
  WorkspaceGitDiffScope scope = WorkspaceGitDiffScope.file,
  String? filePath = 'lib/large.dart',
  String title = 'large.dart unstaged',
  GitChangeArea? area = GitChangeArea.unstaged,
  String? gitDiffRoot,
  String? oldPath,
  String? commitOid,
  String? parentOid,
  String? compareRef,
}) {
  final now = DateTime.utc(2026, 6, 6);
  final payload = <String, Object?>{
    workspaceTabGitDiffSourcePayloadKey: source.key,
    workspaceTabGitDiffScopePayloadKey: scope.key,
    workspaceTabFilePathPayloadKey: ?filePath,
  };
  if (area != null) {
    payload[workspaceTabGitDiffAreaPayloadKey] = area.key;
  }
  if (oldPath != null) {
    payload[workspaceTabGitDiffOldPathPayloadKey] = oldPath;
  }
  if (commitOid != null) {
    payload[workspaceTabGitDiffCommitOidPayloadKey] = commitOid;
  }
  if (parentOid != null) {
    payload[workspaceTabGitDiffParentOidPayloadKey] = parentOid;
  }
  if (compareRef != null) {
    payload[workspaceTabGitDiffCompareRefPayloadKey] = compareRef;
  }
  if (gitDiffRoot != null) {
    payload[workspaceTabGitDiffRootPayloadKey] = gitDiffRoot;
  }
  return WorkspaceTabRecord(
    id: 'tab-1',
    workspaceId: 'workspace-1',
    kind: .gitDiff,
    title: title,
    createdAt: now,
    updatedAt: now,
    payload: payload,
  );
}

class _GitDiffSurfaceTestController extends WorkbenchController {
  final List<String> openedRelativePaths = <String>[];

  @override
  WorkbenchState build() => const WorkbenchState();

  @override
  Future<WorkspaceTabRecord> openEditorTab({
    required Workspace workspace,
    required String relativePath,
    String? targetGroupId,
    bool preview = false,
  }) async {
    openedRelativePaths.add(relativePath);
    final now = DateTime.utc(2026, 6, 6);
    return WorkspaceTabRecord(
      id: 'editor-${openedRelativePaths.length}',
      workspaceId: workspace.id,
      kind: .editor,
      title: relativePath.split('/').last,
      createdAt: now,
      updatedAt: now,
      payload: <String, Object?>{workspaceTabFilePathPayloadKey: relativePath},
    );
  }
}

class _BlockingDiffBlobBackend extends FakeGitBackend {
  final Map<({String filePath, bool oldSide}), Completer<void>> gates =
      <({String filePath, bool oldSide}), Completer<void>>{};
  final List<({String filePath, bool oldSide})> started =
      <({String filePath, bool oldSide})>[];

  @override
  Future<Uint8List?> diffBlobBytes({
    required String path,
    required String filePath,
    String? oldPath,
    GitChangeArea? area,
    String? commitOid,
    String? parentOid,
    required bool oldSide,
  }) async {
    final request = (filePath: filePath, oldSide: oldSide);
    started.add(request);
    await gates[request]?.future;
    return super.diffBlobBytes(
      path: path,
      filePath: filePath,
      oldPath: oldPath,
      area: area,
      commitOid: commitOid,
      parentOid: parentOid,
      oldSide: oldSide,
    );
  }
}

class _ProgressiveAllDiffBackend extends FakeGitBackend {
  final Map<String, GitDiffResult> diffByFile = <String, GitDiffResult>{};
  final Map<String, Completer<void>> gates = <String, Completer<void>>{};
  final List<String> requestedFilePaths = <String>[];
  final List<String> completedFilePaths = <String>[];

  @override
  Future<GitDiffPage> diffAllPage({
    required String path,
    required List<String> filePaths,
  }) async {
    final files = <GitDiffFile>[];
    for (final filePath in filePaths) {
      requestedFilePaths.add(filePath);
      await gates[filePath]?.future;
      completedFilePaths.add(filePath);
      files.addAll(diffByFile[filePath]?.files ?? const <GitDiffFile>[]);
    }
    calls.add(
      GitBackendCall('diffAllPage', <String, Object?>{
        'path': path,
        'filePaths': filePaths,
      }),
    );
    return GitDiffPage(files: files);
  }
}
