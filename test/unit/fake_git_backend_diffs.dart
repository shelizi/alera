part of 'fake_git_backend.dart';

mixin _FakeGitBackendDiffs {
  List<GitBackendCall> get calls;

  GitDiffResult gitDiffResult = const GitDiffResult(files: []);
  GitDiffResult gitDiffAllResult = const GitDiffResult(files: []);
  GitDiffWhitespaceMode lastDiffWhitespaceMode = GitDiffWhitespaceMode.normal;
  Uint8List readingDiffPatchResult = Uint8List(0);
  GitException? readingDiffPatchError;

  /// Bytes served by [diffBlobBytes], keyed by file path and requested side.
  final Map<({String filePath, bool oldSide}), Uint8List> diffBlobBytesBySide =
      <({String filePath, bool oldSide}), Uint8List>{};

  Future<GitDiffResult> diff({
    required String path,
    required String filePath,
    required GitChangeArea area,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  }) async {
    lastDiffWhitespaceMode = whitespaceMode;
    calls.add(
      GitBackendCall('diff', <String, Object?>{
        'path': path,
        'filePath': filePath,
        'area': area,
        'whitespaceMode': whitespaceMode,
      }),
    );
    return gitDiffResult;
  }

  Future<GitDiffResult> diffAll({
    required String path,
    String? filePath,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  }) async {
    lastDiffWhitespaceMode = whitespaceMode;
    calls.add(
      GitBackendCall('diffAll', <String, Object?>{
        'path': path,
        'filePath': filePath,
        'whitespaceMode': whitespaceMode,
      }),
    );
    return gitDiffAllResult;
  }

  GitDiffPage gitDiffAllPageResult = const GitDiffPage(files: []);

  Future<GitDiffPage> diffAllPage({
    required String path,
    required List<String> filePaths,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  }) async {
    lastDiffWhitespaceMode = whitespaceMode;
    calls.add(
      GitBackendCall('diffAllPage', <String, Object?>{
        'path': path,
        'filePaths': filePaths,
        'whitespaceMode': whitespaceMode,
      }),
    );
    return gitDiffAllPageResult;
  }

  Future<Uint8List> readingDiffPatch({
    required String path,
    String? filePath,
    String? oldPath,
    GitChangeArea? area,
    String? commitOid,
    String? parentOid,
    String? baseRef,
  }) async {
    calls.add(
      GitBackendCall('readingDiffPatch', <String, Object?>{
        'path': path,
        'filePath': filePath,
        'oldPath': oldPath,
        'area': area,
        'commitOid': commitOid,
        'parentOid': parentOid,
        'baseRef': baseRef,
      }),
    );
    final error = readingDiffPatchError;
    if (error != null) {
      throw error;
    }
    return readingDiffPatchResult;
  }

  Future<Uint8List?> diffBlobBytes({
    required String path,
    required String filePath,
    String? oldPath,
    GitChangeArea? area,
    String? commitOid,
    String? parentOid,
    required bool oldSide,
  }) async {
    calls.add(
      GitBackendCall('diffBlobBytes', <String, Object?>{
        'path': path,
        'filePath': filePath,
        'oldPath': oldPath,
        'area': area,
        'commitOid': commitOid,
        'parentOid': parentOid,
        'oldSide': oldSide,
      }),
    );
    return diffBlobBytesBySide[(filePath: filePath, oldSide: oldSide)];
  }
}
