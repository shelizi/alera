import 'package:alera/src/features/workbench/application/workspace_source_control_index_mutations.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_index_mutation_port.dart';
import 'package:flutter_test/flutter_test.dart';

final class _IndexFake implements GitIndexMutationPort {
  final calls = <String>[];

  @override
  Future<void> stage({required String path, String? filePath}) async {
    calls.add('stage:$path:$filePath');
  }

  @override
  Future<void> unstage({required String path, String? filePath}) async {
    calls.add('unstage:$path:$filePath');
  }

  @override
  Future<void> discard({required String path, String? filePath}) async {
    calls.add('discard:$path:$filePath');
  }

  @override
  Future<void> stageArea({
    required String path,
    required GitChangeArea area,
    String? filePath,
  }) async {
    calls.add('stageArea:$path:${area.key}:$filePath');
  }

  @override
  Future<void> unstageArea({
    required String path,
    required GitChangeArea area,
    String? filePath,
  }) async {
    calls.add('unstageArea:$path:${area.key}:$filePath');
  }

  @override
  Future<void> discardArea({
    required String path,
    required GitChangeArea area,
    String? filePath,
  }) async {
    calls.add('discardArea:$path:${area.key}:$filePath');
  }
}

void main() {
  const renamedUnstaged = GitChangeEntry(
    path: 'renamed.dart',
    oldPath: 'original.dart',
    area: .unstaged,
    status: .renamed,
  );
  const renamedStaged = GitChangeEntry(
    path: 'renamed.dart',
    oldPath: 'original.dart',
    area: .staged,
    status: .renamed,
  );

  test(
    'rename stages old path before new path and skips forbidden areas',
    () async {
      final backend = _IndexFake();
      final index = WorkspaceSourceControlIndexMutations(backend);
      await index.stageEntry('/repo', renamedUnstaged);
      await index.stageEntry('/repo', renamedStaged);
      expect(backend.calls, <String>[
        'stage:/repo:original.dart',
        'stage:/repo:renamed.dart',
      ]);
    },
  );

  test('rename unstage/discard preserve old-before-new ordering', () async {
    final backend = _IndexFake();
    final index = WorkspaceSourceControlIndexMutations(backend);
    await index.unstageEntry('/repo', renamedStaged);
    await index.discardEntry('/repo', renamedUnstaged);
    expect(backend.calls, <String>[
      'unstage:/repo:original.dart',
      'unstage:/repo:renamed.dart',
      'discard:/repo:original.dart',
      'discard:/repo:renamed.dart',
    ]);
  });

  test(
    'nested submodule and dirty submodule are not mutated from parent',
    () async {
      final backend = _IndexFake();
      final index = WorkspaceSourceControlIndexMutations(backend);
      const nested = GitChangeEntry(
        path: 'child/file.dart',
        submoduleRoot: 'child',
        area: .unstaged,
        status: .modified,
      );
      const dirtySubmodule = GitChangeEntry(
        path: 'child',
        area: .unstaged,
        status: .modified,
        submodule: GitSubmoduleStatus(
          commitChanged: false,
          trackedChanges: true,
          untrackedChanges: false,
          inspectable: true,
        ),
      );
      await index.stageEntry('/repo', nested);
      await index.discardEntry('/repo', nested);
      await index.stageEntry('/repo', dirtySubmodule);
      await index.discardEntry('/repo', dirtySubmodule);
      expect(backend.calls, isEmpty);
    },
  );

  test(
    'area mutations forward exact scope without relying on broad GitBackend',
    () async {
      final backend = _IndexFake();
      final index = WorkspaceSourceControlIndexMutations(backend);
      await index.stageArea('/repo', .unstaged, filePath: 'src');
      await index.unstageArea('/repo', .staged);
      await index.discardArea('/repo', .untracked, filePath: 'tmp');
      expect(backend.calls, <String>[
        'stageArea:/repo:unstaged:src',
        'unstageArea:/repo:staged:null',
        'discardArea:/repo:untracked:tmp',
      ]);
    },
  );
}
