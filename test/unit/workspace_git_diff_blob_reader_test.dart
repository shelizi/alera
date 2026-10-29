import 'dart:typed_data';

import 'package:alera/src/features/workbench/application/workspace_git_branch_compare_cache.dart';
import 'package:alera/src/features/workbench/application/workspace_git_diff_blob_reader.dart';
import 'package:alera/src/shared/infra/git/git_diff_blob_query_port.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter_test/flutter_test.dart';

final class _TinyBlobFake implements GitDiffBlobQueryPort {
  final calls = <String>[];
  bool fail = false;
  final bytes = Uint8List.fromList([1, 2, 3]);

  @override
  Future<Uint8List?> fileRevisionBlobBytes({
    required String path,
    required String filePath,
    required String revisionOid,
    required bool oldSide,
  }) async {
    calls.add('revision:$path:$filePath:$revisionOid:$oldSide');
    if (fail) throw StateError('revision missing');
    return bytes;
  }

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
    calls.add(
      'diff:$path:$filePath:$oldPath:${area?.name}:$commitOid:$parentOid:$oldSide',
    );
    if (fail) throw StateError('blob unavailable');
    return bytes;
  }
}

const _renamed = GitDiffFile(
  path: 'new.dart',
  oldPath: 'old.dart',
  area: .staged,
  status: .renamed,
);

void main() {
  test(
    'file revision uses an exact revision read and never requests a diff blob',
    () async {
      final fake = _TinyBlobFake();
      final reader = WorkspaceGitDiffBlobReader(fake);
      final bytes = await reader.loadSide(
        path: '/repo',
        file: _renamed,
        oldSide: true,
        isFileRevisionDiff: true,
        revisionOid: 'rev-1',
        isCommitBackedDiff: false,
      );
      expect(bytes, same(fake.bytes));
      expect(fake.calls, ['revision:/repo:new.dart:rev-1:true']);
    },
  );

  test('missing revision and failed native blob read return null', () async {
    final fake = _TinyBlobFake()..fail = true;
    final reader = WorkspaceGitDiffBlobReader(fake);
    expect(
      await reader.loadSide(
        path: '/repo',
        file: _renamed,
        oldSide: true,
        isFileRevisionDiff: true,
        revisionOid: null,
        isCommitBackedDiff: false,
      ),
      isNull,
    );
    expect(fake.calls, isEmpty);
    expect(
      await reader.loadSide(
        path: '/repo',
        file: _renamed,
        oldSide: true,
        isFileRevisionDiff: true,
        revisionOid: 'rev',
        isCommitBackedDiff: false,
      ),
      isNull,
    );
  });

  test(
    'staged working tree diff preserves change area and original rename path',
    () async {
      final fake = _TinyBlobFake();
      await WorkspaceGitDiffBlobReader(fake).loadSide(
        path: '/repo',
        file: _renamed,
        oldSide: false,
        isFileRevisionDiff: false,
        isCommitBackedDiff: false,
      );
      expect(fake.calls, [
        'diff:/repo:new.dart:old.dart:staged:null:null:false',
      ]);
    },
  );

  test(
    'commit-backed diff excludes worktree area and caches immutable blob',
    () async {
      final fake = _TinyBlobFake();
      final cache = WorkspaceGitBranchCompareCache();
      final reader = WorkspaceGitDiffBlobReader(fake);
      for (var n = 0; n < 2; n++) {
        final result = await reader.loadSide(
          path: '/repo',
          file: _renamed,
          oldSide: true,
          isFileRevisionDiff: false,
          isCommitBackedDiff: true,
          revisionOid: 'head',
          parentOid: 'base',
          branchCompareCache: cache,
        );
        expect(result, same(fake.bytes));
      }
      expect(fake.calls, ['diff:/repo:new.dart:old.dart:null:head:base:true']);
    },
  );
}
