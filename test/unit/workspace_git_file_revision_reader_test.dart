import 'package:alera/src/features/workbench/application/workspace_git_file_revision_reader.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_file_revision_query_port.dart';
import 'package:flutter_test/flutter_test.dart';

final class _TinyRevisionFake implements GitFileRevisionQueryPort {
  final calls = <(String, String, String)>[];
  GitFileRevision? result;
  Object? error;

  @override
  Future<GitFileRevision?> latestFileRevision({
    required String path,
    required String filePath,
    required String gitRef,
  }) async {
    calls.add((path, filePath, gitRef));
    if (error case final error?) throw error;
    return result;
  }
}

const _nested = WorkspaceSourceControlScope(
  workspaceId: 'one',
  workspacePath: '/workspace',
  path: '/workspace/packages/app',
  relativeRoot: 'packages/app',
);

void main() {
  test('nested repository revision request stays within its Git scope and keeps selected ref', () async {
    final fake = _TinyRevisionFake()
      ..result = const GitFileRevision(
        oid: 'long-oid',
        shortOid: 'short',
        subject: 'test subject',
      );
    final revision = await WorkspaceGitFileRevisionReader(fake)
        .latestForScopedFile(
          scope: _nested,
          workspaceRelativePath: 'packages\\app/lib/../lib/main.dart',
          gitRef: 'feature/custom',
        );
    expect(revision, same(fake.result));
    expect(fake.calls, [
      ('/workspace/packages/app', 'lib/main.dart', 'feature/custom'),
    ]);
  });

  test(
    'other submodule paths, root-only scopes and traversal never query Git',
    () async {
      final fake = _TinyRevisionFake();
      final reader = WorkspaceGitFileRevisionReader(fake);
      for (final path in [
        'packages/elsewhere/main.dart',
        'packages/app',
        '../secrets',
      ]) {
        expect(
          await reader.latestForScopedFile(
            scope: _nested,
            workspaceRelativePath: path,
            gitRef: 'main',
          ),
          isNull,
        );
      }
      expect(fake.calls, isEmpty);
    },
  );

  test(
    'unborn branch missing revision stays null, native errors propagate',
    () async {
      final fake = _TinyRevisionFake();
      const root = WorkspaceSourceControlScope(
        workspaceId: 'one',
        workspacePath: '/root',
        path: '/root',
      );
      final reader = WorkspaceGitFileRevisionReader(fake);
      expect(
        await reader.latestForScopedFile(
          scope: root,
          workspaceRelativePath: 'src/hello.dart',
          gitRef: 'HEAD',
        ),
        isNull,
      );
      fake.error = StateError('git backend unavailable');
      await expectLater(
        reader.latestForScopedFile(
          scope: root,
          workspaceRelativePath: 'src/hello.dart',
          gitRef: 'HEAD',
        ),
        throwsStateError,
      );
    },
  );
}
