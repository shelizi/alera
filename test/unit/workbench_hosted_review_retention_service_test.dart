import 'package:alera/src/features/workbench/application/workbench_hosted_review_retention_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'release retries the same review root from the fallback workspace',
    () async {
      final backend = _RecordingGitBackend();
      final service = WorkbenchHostedReviewRetentionService(
        gitBackend: backend,
      );
      final workspace = _workspace(path: 'C:/work/workspace');
      final primaryPath = sourceControlRootAbsolutePath(
        workspacePath: workspace.path,
        relativeRoot: 'packages/app',
      );
      final fallbackPath = sourceControlRootAbsolutePath(
        workspacePath: 'C:/work/repo',
        relativeRoot: 'packages/app',
      );
      backend.failingReleasePaths.add(primaryPath);

      await service.release(
        workspace: workspace,
        relativeRoot: 'packages/app',
        retentionId: 'retention-1',
        fallbackWorkspacePath: 'C:/work/repo',
      );

      expect(backend.releaseCalls, <_HostedReviewCall>[
        _HostedReviewCall(path: primaryPath, retentionId: 'retention-1'),
        _HostedReviewCall(path: fallbackPath, retentionId: 'retention-1'),
      ]);
    },
  );

  test('persist resolves a relative source-control root', () async {
    final backend = _RecordingGitBackend();
    final service = WorkbenchHostedReviewRetentionService(gitBackend: backend);
    final workspace = _workspace(path: 'C:/work/workspace');
    final expectedPath = sourceControlRootAbsolutePath(
      workspacePath: workspace.path,
      relativeRoot: 'packages/app',
    );

    await service.persist(
      workspace: workspace,
      relativeRoot: 'packages/app',
      retentionId: 'retention-2',
    );

    expect(backend.persistCalls, <_HostedReviewCall>[
      _HostedReviewCall(path: expectedPath, retentionId: 'retention-2'),
    ]);
  });

  test('releaseTab only releases retained pull-request tabs', () async {
    final backend = _RecordingGitBackend();
    final service = WorkbenchHostedReviewRetentionService(gitBackend: backend);
    final workspace = _workspace(path: 'C:/work/workspace');

    await service.releaseTab(
      workspace,
      _tab(
        id: 'working-tree',
        payload: <String, Object?>{
          workspaceTabGitDiffHostedReviewRetentionIdPayloadKey: 'ignored',
        },
      ),
    );
    await service.releaseTab(
      workspace,
      _tab(
        id: 'pull-request',
        payload: <String, Object?>{
          workspaceTabGitDiffSourcePayloadKey:
              WorkspaceGitDiffSource.pullRequest.key,
          workspaceTabGitDiffHostedReviewRetentionIdPayloadKey: 'retention-3',
        },
      ),
    );

    expect(backend.releaseCalls, <_HostedReviewCall>[
      const _HostedReviewCall(
        path: 'C:/work/workspace',
        retentionId: 'retention-3',
      ),
    ]);
  });
}

Workspace _workspace({required String path}) {
  final now = DateTime.utc(2026, 9, 10);
  return Workspace(
    id: 'workspace',
    projectId: 'project',
    name: 'Workspace',
    path: path,
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.main,
    status: WorkspaceStatus.active,
  );
}

WorkspaceTabRecord _tab({
  required String id,
  Map<String, Object?> payload = const <String, Object?>{},
}) {
  final now = DateTime.utc(2026, 9, 10);
  return WorkspaceTabRecord(
    id: id,
    workspaceId: 'workspace',
    title: id,
    createdAt: now,
    updatedAt: now,
    payload: payload,
  );
}

final class _HostedReviewCall {
  const _HostedReviewCall({required this.path, required this.retentionId});

  final String path;
  final String retentionId;

  @override
  bool operator ==(Object other) =>
      other is _HostedReviewCall &&
      other.path == path &&
      other.retentionId == retentionId;

  @override
  int get hashCode => Object.hash(path, retentionId);
}

final class _RecordingGitBackend implements GitBackend {
  final List<_HostedReviewCall> releaseCalls = <_HostedReviewCall>[];
  final List<_HostedReviewCall> persistCalls = <_HostedReviewCall>[];
  final Set<String> failingReleasePaths = <String>{};

  @override
  Future<void> releaseHostedReviewRange({
    required String path,
    required String retentionId,
  }) async {
    releaseCalls.add(_HostedReviewCall(path: path, retentionId: retentionId));
    if (failingReleasePaths.contains(path)) {
      throw StateError('release failed');
    }
  }

  @override
  Future<void> persistHostedReviewRange({
    required String path,
    required String retentionId,
  }) async {
    persistCalls.add(_HostedReviewCall(path: path, retentionId: retentionId));
  }

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}
