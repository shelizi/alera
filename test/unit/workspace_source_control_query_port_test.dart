import 'package:alera/src/features/workbench/application/workspace_source_control_snapshot_loader.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_source_control_query_port.dart';
import 'package:flutter_test/flutter_test.dart';

final class _SourceControlReader implements GitSourceControlQueryPort {
  final List<String> calls = <String>[];
  final GitStatusResult result = const GitStatusResult(entries: []);
  final GitRepositoryState repository = const GitRepositoryState(
    branch: 'feature',
    upstream: 'origin/feature',
    ahead: 2,
  );
  final List<GitStashEntry> stashRows = const <GitStashEntry>[
    GitStashEntry(index: 0, reference: 'stash@{0}', message: 'WIP', oid: 'abc'),
  ];

  @override
  Future<GitStatusResult> status(String path) async {
    calls.add('status:$path');
    return result;
  }

  @override
  Future<GitRepositoryState> repositoryState(String path) async {
    calls.add('repository:$path');
    return repository;
  }

  @override
  Future<List<GitStashEntry>> listStashes(String path) async {
    calls.add('stashes:$path');
    return stashRows;
  }
}

void main() {
  test('source-control snapshot needs only three read operations', () async {
    final reader = _SourceControlReader();
    final snapshot = await WorkspaceSourceControlSnapshotLoader(reader)
        .load('/workspace');
    expect(snapshot.status, same(reader.result));
    expect(snapshot.repositoryState, same(reader.repository));
    expect(snapshot.stashes, same(reader.stashRows));
    expect(reader.calls, <String>[
      'status:/workspace',
      'repository:/workspace',
      'stashes:/workspace',
    ]);
  });
}
