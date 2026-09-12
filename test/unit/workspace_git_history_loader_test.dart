import 'dart:async';

import 'package:alera/src/features/workbench/application/workspace_git_history_loader.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:flutter_test/flutter_test.dart';

import 'fake_git_backend.dart';

void main() {
  test('coalesces overlapping loads and returns one backend future', () async {
    final pending = Completer<GitHistoryResult>();
    final backend = FakeGitBackend()..gitHistoryResultQueue.add(pending.future);
    var changes = 0;
    final loader = WorkspaceGitHistoryLoader(
      backend: backend,
      scopePath: '/tmp/project',
      onChanged: () => changes += 1,
    );

    final first = loader.load();
    final second = loader.load();

    expect(identical(first, second), isTrue);
    expect(
      backend.calls.where((call) => call.method == 'history'),
      hasLength(1),
    );
    expect(loader.loading, isTrue);
    expect(loader.refreshing, isFalse);
    expect(loader.needsLoad, isFalse);

    final result = _history('first', 'First Commit');
    pending.complete(result);

    expect(await first, same(result));
    expect(await second, same(result));
    expect(loader.result, same(result));
    expect(loader.dirty, isFalse);
    expect(loader.loading, isFalse);
    expect(loader.needsLoad, isFalse);
    expect(changes, greaterThanOrEqualTo(2));
  });

  test(
    'markStale defers a reload and supersedes an in-flight result',
    () async {
      final first = Completer<GitHistoryResult>();
      final backend = FakeGitBackend()
        ..gitHistoryResultQueue.add(first.future)
        ..gitHistoryResult = _history('second', 'Second Commit');
      final loader = WorkspaceGitHistoryLoader(
        backend: backend,
        scopePath: '/tmp/project',
        onChanged: () {},
      );

      final firstLoad = loader.load();
      loader.markStale();

      expect(loader.dirty, isTrue);
      expect(loader.needsLoad, isTrue);
      final secondLoad = loader.load();
      final secondResult = await secondLoad;
      expect(secondResult.items.single.subject, 'Second Commit');

      first.complete(_history('first', 'First Commit'));
      await firstLoad;

      expect(loader.result, same(secondResult));
      expect(loader.result?.items.single.subject, 'Second Commit');
      expect(
        backend.calls.where((call) => call.method == 'history'),
        hasLength(2),
      );
    },
  );

  test(
    'rebind resets state and drops a late result from the old scope',
    () async {
      final first = Completer<GitHistoryResult>();
      final backend = FakeGitBackend()
        ..gitHistoryResultQueue.add(first.future)
        ..gitHistoryResult = _history('second', 'Second Commit');
      var changes = 0;
      final loader = WorkspaceGitHistoryLoader(
        backend: backend,
        scopePath: '/tmp/project',
        onChanged: () => changes += 1,
      );

      final firstLoad = loader.load();
      loader.rebind('/tmp/project/packages/app');

      expect(loader.scopePath, '/tmp/project/packages/app');
      expect(loader.result, isNull);
      expect(loader.error, isNull);
      expect(loader.loading, isFalse);
      expect(loader.needsLoad, isTrue);

      final secondResult = await loader.load();
      first.complete(_history('first', 'First Commit'));
      await firstLoad;

      expect(loader.result, same(secondResult));
      expect(loader.result?.items.single.subject, isNot('First Commit'));
      expect(
        backend.calls
            .where((call) => call.method == 'history')
            .map((call) => call.args['path']),
        <Object?>['/tmp/project', '/tmp/project/packages/app'],
      );
      expect(changes, greaterThanOrEqualTo(3));
    },
  );

  test(
    'detach prevents late completions from notifying the presentation',
    () async {
      final pending = Completer<GitHistoryResult>();
      final backend = FakeGitBackend()
        ..gitHistoryResultQueue.add(pending.future);
      var changes = 0;
      final loader = WorkspaceGitHistoryLoader(
        backend: backend,
        scopePath: '/tmp/project',
        onChanged: () => changes += 1,
      );

      final load = loader.load();
      final changesBeforeDetach = changes;
      loader.detach();
      pending.complete(_history('late', 'Late Commit'));
      await load;

      expect(changes, changesBeforeDetach);
      loader.dispose();
    },
  );
}

GitHistoryResult _history(String id, String subject) => GitHistoryResult(
  currentRef: GitHistoryItemRef(
    id: 'refs/heads/main',
    name: 'main',
    revision: id,
  ),
  hasIncomingChanges: false,
  hasOutgoingChanges: false,
  hasMore: false,
  limit: 50,
  items: <GitHistoryItem>[
    GitHistoryItem(
      id: id,
      parentIds: const <String>[],
      subject: subject,
      message: subject,
    ),
  ],
);
