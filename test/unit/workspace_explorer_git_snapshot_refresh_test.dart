import 'dart:async';

import 'package:alera/src/features/workbench/application/workspace_explorer_git_snapshot_refresh.dart';
import 'package:alera/src/shared/infra/git/git_explorer_status.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'synchronous requests share one scan but later idle requests are fresh',
    () async {
      final fixture = _Fixture();
      addTearDown(fixture.dispose);
      final requests = List.generate(
        100,
        (_) => fixture.refresh.refresh('/repo'),
      );
      await _flush();
      expect(fixture.calls, ['/repo']);
      expect(
        requests.every((future) => identical(future, requests.first)),
        isTrue,
      );
      fixture.finish(0, 'first.txt');
      await Future.wait(requests);
      expect(
        fixture.published.single.statusFor('first.txt'),
        GitExplorerStatus.modified,
      );

      final next = fixture.refresh.refresh('/repo');
      await _flush();
      expect(fixture.calls, ['/repo', '/repo']);
      fixture.finish(1, 'second.txt');
      await next;
      expect(
        fixture.published.last.statusFor('second.txt'),
        GitExplorerStatus.modified,
      );
    },
  );

  test(
    '1000 trailing requests share one fresh scan after blocked read',
    () async {
      final fixture = _Fixture();
      addTearDown(fixture.dispose);
      final first = fixture.refresh.refresh('/repo');
      await _flush();
      expect(fixture.calls, ['/repo']);
      final trailing = List.generate(
        1000,
        (_) => fixture.refresh.refresh('/repo'),
      );
      var trailingDone = false;
      unawaited(trailing.first.then((_) => trailingDone = true));
      await _flush();
      expect(fixture.calls, ['/repo']);
      expect(
        trailing.every((future) => identical(future, trailing.first)),
        isTrue,
      );
      expect(identical(trailing.first, first), isFalse);

      fixture.finish(0, 'old.txt');
      await first;
      await _flush();
      expect(fixture.calls, ['/repo', '/repo']);
      expect(
        trailingDone,
        isFalse,
        reason: 'must not reuse the pre-request scan',
      );
      fixture.finish(1, 'fresh.txt');
      await Future.wait(trailing);
      expect(trailingDone, isTrue);
      expect(
        fixture.published.map((snapshot) => snapshot.statuses.keys.single),
        ['old.txt', 'fresh.txt'],
      );
      expect(fixture.peak, 1);
    },
  );

  test(
    'failed scan publishes empty fallback and trailing request recovers',
    () async {
      final fixture = _Fixture();
      addTearDown(fixture.dispose);
      final first = fixture.refresh.refresh('/repo');
      await _flush();
      final next = fixture.refresh.refresh('/repo');
      fixture.gates.first.completeError(StateError('fixture failure'));
      await first;
      await _flush();
      expect(fixture.published.single.statuses, isEmpty);
      expect(fixture.calls, ['/repo', '/repo']);
      fixture.finish(1, 'recovered.txt');
      await next;
      expect(
        fixture.published.last.statusFor('recovered.txt'),
        GitExplorerStatus.modified,
      );
    },
  );

  test(
    'same-path generation reset retires old success and pending request',
    () async {
      final fixture = _Fixture();
      addTearDown(fixture.dispose);
      final old = fixture.refresh.refresh('/repo');
      await _flush();
      final oldPending = fixture.refresh.refresh('/repo');
      fixture.refresh.clear();
      await Future.wait([old, oldPending]);
      final latest = fixture.refresh.refresh('/repo');
      await _flush();
      expect(fixture.calls, ['/repo', '/repo']);
      fixture.finish(1, 'latest.txt');
      await latest;
      fixture.finish(0, 'stale.txt');
      await _flush();
      expect(
        fixture.published.single.statusFor('latest.txt'),
        GitExplorerStatus.modified,
      );
      expect(fixture.published.single.statusFor('stale.txt'), isNull);
      expect(fixture.calls, hasLength(2));
    },
  );

  test(
    'rapid generation changes bound scans and cancel obsolete queued scopes',
    () async {
      final fixture = _Fixture();
      addTearDown(fixture.dispose);
      final requests = <Future<void>>[];
      final paths = List.generate(7, (index) => '/repo/$index');
      for (final path in paths) {
        requests.add(fixture.refresh.refresh(path));
        await _flush();
      }
      expect(fixture.calls, paths.take(4).toList());
      expect(fixture.peak, 4);
      fixture.finish(0, 'stale.txt');
      await _flush();
      expect(fixture.calls, [...paths.take(4), paths.last]);
      expect(fixture.peak, 4);
      expect(fixture.published, isEmpty);

      fixture.refresh.dispose();
      await Future.wait(requests);
      for (var i = 1; i < fixture.gates.length; i++) {
        fixture.finish(i, 'retired-$i.txt');
      }
      await _flush();
      expect(fixture.published, isEmpty);
      expect(fixture.active, 0);
      await fixture.refresh.refresh('/repo/ignored');
      expect(fixture.calls, hasLength(5));
    },
  );
}

Future<void> _flush() async {
  await Future<void>.delayed(Duration.zero);
  await Future<void>.delayed(Duration.zero);
}

final class _Fixture {
  _Fixture() {
    refresh = WorkspaceExplorerGitSnapshotRefresh(
      load: (path) async {
        calls.add(path);
        final gate = Completer<GitExplorerStatusSnapshot>();
        gates.add(gate);
        active++;
        if (active > peak) peak = active;
        try {
          return await gate.future;
        } finally {
          active--;
        }
      },
      publish: published.add,
    );
  }

  late final WorkspaceExplorerGitSnapshotRefresh refresh;
  final calls = <String>[];
  final gates = <Completer<GitExplorerStatusSnapshot>>[];
  final published = <GitExplorerStatusSnapshot>[];
  int active = 0;
  int peak = 0;

  void finish(int index, String path) {
    gates[index].complete(
      GitExplorerStatusSnapshot({path: GitExplorerStatus.modified}),
    );
  }

  void dispose() {
    refresh.dispose();
    for (final gate in gates) {
      if (!gate.isCompleted) {
        gate.complete(const GitExplorerStatusSnapshot.empty());
      }
    }
  }
}
