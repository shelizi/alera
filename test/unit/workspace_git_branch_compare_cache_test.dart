import 'dart:async';
import 'dart:typed_data';

import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workspace_git_branch_compare_cache.dart';
import 'package:alera/src/features/workbench/application/workspace_git_branch_compare_cache_provider.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

const _snapshot = GitCommitCompareResult(
  summary: GitCommitCompareSummary(
    commitOid: 'head',
    parentOid: 'base',
    compareRef: 'head',
    baseRef: 'base',
    changedFiles: 0,
    status: .ready,
  ),
  entries: [],
);
const _fileKey = ('/repo', 'head', 'base', 'file.dart', null);
const _diff = GitDiffResult(files: []);

void main() {
  test('shares pending and completed snapshot queries', () async {
    final cache = WorkspaceGitBranchCompareCache();
    final pending = Completer<GitCommitCompareResult>();
    var calls = 0;
    Future<GitCommitCompareResult> query() => cache.snapshot(
      path: '/repo',
      baseOid: 'base',
      headOid: 'head',
      load: () {
        ++calls;
        return pending.future;
      },
    );
    final first = query();
    final second = query();
    expect(calls, 1);
    pending.complete(_snapshot);
    expect(await first, same(_snapshot));
    expect(await second, same(_snapshot));
    expect(await query(), same(_snapshot));
    expect(calls, 1);
  });

  test('failed snapshots and file queries can retry', () async {
    final cache = WorkspaceGitBranchCompareCache();
    await expectLater(
      cache.snapshot(
        path: '/repo',
        baseOid: 'base',
        headOid: 'head',
        load: () => throw StateError('missing objects'),
      ),
      throwsStateError,
    );
    expect(
      await cache.snapshot(
        path: '/repo',
        baseOid: 'base',
        headOid: 'head',
        load: () async => _snapshot,
      ),
      same(_snapshot),
    );
    await expectLater(
      cache.diff(
        key: _fileKey,
        whitespaceMode: .normal,
        load: () async => throw StateError('failed diff'),
      ),
      throwsStateError,
    );
    expect(
      await cache.diff(
        key: _fileKey,
        whitespaceMode: .normal,
        load: () async => _diff,
      ),
      same(_diff),
    );
  });

  test('deduplicates file diff and blob requests with isolated keys', () async {
    final cache = WorkspaceGitBranchCompareCache();
    var diffCalls = 0;
    var blobCalls = 0;
    Future<GitDiffResult> diff(
      BranchCompareFileKey key, [
      GitDiffWhitespaceMode mode = GitDiffWhitespaceMode.normal,
    ]) => cache.diff(
      key: key,
      whitespaceMode: mode,
      load: () async {
        ++diffCalls;
        return _diff;
      },
    );
    Future<Uint8List?> blob(bool oldSide) => cache.blob(
      key: _fileKey,
      oldSide: oldSide,
      load: () async {
        ++blobCalls;
        return Uint8List.fromList([1, 2]);
      },
    );
    await Future.wait([diff(_fileKey), diff(_fileKey)]);
    await diff(_fileKey);
    expect(diffCalls, 1);
    await diff(('/other', 'head', 'base', 'file.dart', null));
    await diff(('/repo', 'head-2', 'base', 'file.dart', null));
    await diff(('/repo', 'head', 'base', 'renamed.dart', 'file.dart'));
    await diff(_fileKey, .ignoreAll);
    expect(diffCalls, 5);
    await Future.wait([blob(true), blob(true)]);
    await blob(true);
    await blob(false);
    expect(blobCalls, 2);
  });

  test('changing pinned snapshot drops old file queries', () async {
    final cache = WorkspaceGitBranchCompareCache();
    Future<void> bind(String path, String head) async {
      await cache.snapshot(
        path: path,
        baseOid: 'base',
        headOid: head,
        load: () async => _snapshot,
      );
    }

    var calls = 0;
    Future<GitDiffResult> diff() => cache.diff(
      key: _fileKey,
      whitespaceMode: .normal,
      load: () async {
        ++calls;
        return _diff;
      },
    );
    await bind('/repo', 'head');
    await diff();
    await bind('/repo', 'head-2');
    await diff();
    await bind('/other', 'head-2');
    await diff();
    expect(calls, 3);
  });

  test('explicit update seeds pinned result without another query', () async {
    final cache = WorkspaceGitBranchCompareCache();
    cache.acceptSnapshot('/repo', _snapshot);
    expect(
      await cache.snapshot(
        path: '/repo',
        baseOid: 'base',
        headOid: 'head',
        load: () => throw StateError('should use update result'),
      ),
      same(_snapshot),
    );
  });

  test('late failures cannot remove an updated snapshot', () async {
    final cache = WorkspaceGitBranchCompareCache();
    final pending = Completer<GitCommitCompareResult>();
    final first = cache.snapshot(
      path: '/repo',
      baseOid: 'base',
      headOid: 'head',
      load: () => pending.future,
    );
    final failure = expectLater(first, throwsStateError);
    cache.acceptSnapshot('/repo', _snapshot);
    pending.completeError(StateError('old request failed'));
    await failure;
    expect(
      await cache.snapshot(
        path: '/repo',
        baseOid: 'base',
        headOid: 'head',
        load: () => throw StateError('should use update result'),
      ),
      same(_snapshot),
    );
  });

  test('evicts least recently used file queries and oversized blobs', () async {
    final cache = WorkspaceGitBranchCompareCache();
    var calls = 0;
    Future<GitDiffResult> diff(int index) => cache.diff(
      key: ('/repo', 'head', 'base', '$index.dart', null),
      whitespaceMode: .normal,
      load: () async {
        ++calls;
        return _diff;
      },
    );
    for (var i = 0; i < 16; ++i) {
      await diff(i);
    }
    await diff(0);
    await diff(16);
    await diff(0);
    expect(calls, 17);
    await diff(1);
    expect(calls, 18);
    var blobCalls = 0;
    Future<Uint8List?> blob() => cache.blob(
      key: _fileKey,
      oldSide: false,
      load: () async {
        ++blobCalls;
        return Uint8List(8 * 1024 * 1024 + 1);
      },
    );
    await blob();
    await blob();
    expect(blobCalls, 2);
  });

  test(
    'tab retirement releases cache but switching active tab preserves it',
    () async {
      final tab = WorkspaceTabRecord(
        id: 'compare',
        workspaceId: 'workspace',
        kind: .gitDiff,
        title: 'Compare',
        createdAt: DateTime.utc(2026),
        updatedAt: DateTime.utc(2026),
      );
      final controller = _Workbench(
        WorkbenchState(
          tabsByWorkspace: {
            'workspace': [tab],
          },
        ),
      );
      final container = ProviderContainer(
        overrides: [workbenchControllerProvider.overrideWith(() => controller)],
      );
      addTearDown(container.dispose);
      final provider = workspaceGitBranchCompareCacheProvider(
        'workspace',
        'compare',
      );
      final cache = container.read(provider);
      cache.selectedPath = 'file.dart';
      controller.change(
        controller.state.copyWith(
          activeTabIdByWorkspace: {'workspace': 'other-tab'},
        ),
      );
      await container.pump();
      expect(container.read(provider), same(cache));
      controller.change(
        const WorkbenchState(tabsByWorkspace: {'workspace': []}),
      );
      await container.pump();
      expect(container.read(provider), isNot(same(cache)));
    },
  );

  test('workspace retirement releases its branch comparison cache', () async {
    final workspace = Workspace(
      id: 'workspace',
      projectId: 'project',
      name: 'Main',
      path: '/repo',
      kind: .main,
      status: .active,
      createdAt: DateTime.utc(2026),
      updatedAt: DateTime.utc(2026),
    );
    final controller = _Workbench(
      WorkbenchState(
        workspacesByProject: {
          'project': [workspace],
        },
      ),
    );
    final container = ProviderContainer(
      overrides: [workbenchControllerProvider.overrideWith(() => controller)],
    );
    addTearDown(container.dispose);
    final provider = workspaceGitBranchCompareCacheProvider(
      'workspace',
      'compare',
    );
    final cache = container.read(provider);
    controller.change(const WorkbenchState());
    await container.pump();
    expect(container.read(provider), isNot(same(cache)));
  });
}

class _Workbench(final WorkbenchState initial) extends WorkbenchController {
  @override
  WorkbenchState build() => initial;

  void change(WorkbenchState value) => state = value;
}
