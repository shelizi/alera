import 'package:alera/src/features/workbench/application/workspace_git_diff_page_reader.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_working_tree_diff_query_port.dart';
import 'package:flutter_test/flutter_test.dart';

final class _DiffSnapshotFake implements GitWorkingTreeDiffQueryPort {
  GitStatusResult snapshot = const GitStatusResult(entries: []);
  GitDiffResult combined = const GitDiffResult(files: []);
  GitDiffPage page = const GitDiffPage(files: []);
  bool failPage = false;
  final calls = <String>[];

  @override
  Future<GitStatusResult> status(String path) async {
    calls.add('status:$path');
    return snapshot;
  }

  @override
  Future<GitDiffResult> diff({
    required String path,
    required String filePath,
    required GitChangeArea area,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  }) async {
    calls.add('diff:$path:$filePath');
    return combined;
  }

  @override
  Future<GitDiffResult> diffAll({
    required String path,
    String? filePath,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  }) async {
    calls.add('all:$path:${whitespaceMode.name}');
    return combined;
  }

  @override
  Future<GitDiffPage> diffAllPage({
    required String path,
    required List<String> filePaths,
    GitDiffWhitespaceMode whitespaceMode = GitDiffWhitespaceMode.normal,
  }) async {
    calls.add('page:$path:${filePaths.join(',')}:${whitespaceMode.name}');
    if (failPage) throw StateError('stale git snapshot');
    return page;
  }
}

const _file = GitDiffFile(path: 'a.dart', area: .unstaged, status: .modified);

void main() {
  test('preview limit accounting preserves line text lengths and newlines', () {
    const result = GitDiffResult(
      files: [
        GitDiffFile(
          path: 'a.dart',
          area: .unstaged,
          status: .modified,
          lines: [
            GitDiffLine.addition('a'),
            GitDiffLine.context('你好'),
            GitDiffLine.deletion('😀'),
          ],
        ),
      ],
    );
    // Dart String.length counts UTF-16 code units (the legacy preview limit),
    // not UTF-8 encoded bytes. Keep this precise parity until benchmarked.
    expect(gitDiffPreviewBytes(result), 8);
    expect(gitDiffPreviewBytes(const GitDiffResult(files: [])), 0);
  });

  test(
    'stable status snapshot deduplicates paths before querying the first page',
    () async {
      final fake = _DiffSnapshotFake()
        ..snapshot = const GitStatusResult(
          entries: [
            GitChangeEntry(path: 'a.dart', area: .staged, status: .modified),
            GitChangeEntry(path: 'a.dart', area: .unstaged, status: .modified),
            GitChangeEntry(path: 'b.dart', area: .unstaged, status: .added),
          ],
        )
        ..page = const GitDiffPage(files: [_file], truncated: true);
      final reader = WorkspaceGitDiffPageReader(fake);
      final paths = await reader.changedPaths('/repo');
      expect(paths, <String>['a.dart', 'b.dart']);
      final first = await reader.firstPage(
        path: '/repo',
        pathsFuture: Future.value(paths),
        whitespaceMode: GitDiffWhitespaceMode.ignoreAll,
      );
      expect(first.result.files, [_file]);
      expect(first.result.truncated, isTrue);
      expect(first.nextIndex, 1);
      expect(fake.calls, ['status:/repo', 'page:/repo:a.dart:ignoreAll']);
    },
  );

  test(
    'nested submodule worktree changes fall back to native combined diff',
    () async {
      final fake = _DiffSnapshotFake()
        ..snapshot = const GitStatusResult(
          entries: [
            GitChangeEntry(
              path: 'submodule',
              area: .unstaged,
              status: .modified,
              submodule: GitSubmoduleStatus(
                commitChanged: false,
                trackedChanges: true,
                untrackedChanges: false,
                inspectable: true,
              ),
            ),
          ],
        )
        ..combined = const GitDiffResult(files: [_file]);
      final reader = WorkspaceGitDiffPageReader(fake);
      final paths = await reader.changedPaths('/repo');
      expect(paths, isNull);
      final first = await reader.firstPage(
        path: '/repo',
        pathsFuture: Future.value(paths),
        whitespaceMode: GitDiffWhitespaceMode.normal,
      );
      expect(first.result.files, [_file]);
      expect(first.nextIndex, isNull);
      expect(fake.calls, ['status:/repo', 'all:/repo:normal']);
    },
  );

  test(
    'empty status snapshot neither pages nor requests a combined diff',
    () async {
      final fake = _DiffSnapshotFake();
      final first = await WorkspaceGitDiffPageReader(fake).firstPage(
        path: '/repo',
        pathsFuture: Future.value(<String>[]),
        whitespaceMode: GitDiffWhitespaceMode.normal,
      );
      expect(first.result.files, isEmpty);
      expect(first.nextIndex, isNull);
      expect(fake.calls, isEmpty);
    },
  );

  test(
    'a stale later page is best-effort and preserves previously rendered files',
    () async {
      final fake = _DiffSnapshotFake()..failPage = true;
      final reader = WorkspaceGitDiffPageReader(fake);
      expect(
        await reader.nextPage(
          path: '/repo',
          filePaths: ['b.dart'],
          whitespaceMode: .ignoreEol,
        ),
        isNull,
      );
      expect(fake.calls, ['page:/repo:b.dart:ignoreEol']);
    },
  );
}
