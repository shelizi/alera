import 'dart:async';

import 'package:alera/src/features/workbench/application/workspace_explorer_watch_refresh_queue.dart';
import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const debounce = Duration(milliseconds: 120);

  test('coalesces file storms and deduplicates directories', () {
    fakeAsync((clock) {
      final batches = <List<String>>[];
      final queue = WorkspaceExplorerWatchRefreshQueue(
        refresh: (paths) async => batches.add(paths),
        debounce: debounce,
      );

      queue.enqueue(const ['lib', 'lib']);
      clock.elapse(const Duration(milliseconds: 60));
      queue.enqueue(const ['lib', 'test']);
      clock.elapse(const Duration(milliseconds: 119));
      expect(batches, isEmpty);

      clock.elapse(const Duration(milliseconds: 1));
      clock.flushMicrotasks();
      expect(batches, [
        <String>['lib', 'test'],
      ]);
      queue.dispose();
    });
  });

  test('in-flight read coalesces trailing events', () {
    fakeAsync((clock) {
      final firstRead = Completer<void>();
      final batches = <List<String>>[];
      final queue = WorkspaceExplorerWatchRefreshQueue(
        refresh: (paths) {
          batches.add(paths);
          if (batches.length == 1) return firstRead.future;
          return Future<void>.value();
        },
        debounce: debounce,
      );

      queue.enqueue(const ['lib'], immediate: true);
      expect(batches, [
        <String>['lib'],
      ]);
      queue.enqueue(const ['test']);
      queue.enqueue(const ['test', 'docs']);
      clock.elapse(debounce * 3);
      expect(batches, hasLength(1));

      firstRead.complete();
      clock.flushMicrotasks();
      clock.elapse(debounce);
      clock.flushMicrotasks();
      expect(batches, [
        <String>['lib'],
        <String>['test', 'docs'],
      ]);
      queue.dispose();
    });
  });

  test('watcher recovery drains immediately after in-flight read', () {
    fakeAsync((clock) {
      final firstRead = Completer<void>();
      final batches = <List<String>>[];
      final queue = WorkspaceExplorerWatchRefreshQueue(
        refresh: (paths) {
          batches.add(paths);
          if (batches.length == 1) return firstRead.future;
          return Future<void>.value();
        },
      );

      queue.enqueue(const ['src'], immediate: true);
      queue.enqueue(const [''], immediate: true);
      firstRead.complete();
      clock.flushMicrotasks();
      expect(batches, [
        <String>['src'],
        <String>[''],
      ]);
      queue.dispose();
    });
  });

  test('clear and dispose cancel queued events', () {
    fakeAsync((clock) {
      final batches = <List<String>>[];
      final queue = WorkspaceExplorerWatchRefreshQueue(
        refresh: (paths) async => batches.add(paths),
      );

      queue.enqueue(const ['old-workspace']);
      queue.clear();
      clock.elapse(debounce * 2);
      expect(batches, isEmpty);

      queue.enqueue(const ['new-workspace']);
      clock.elapse(debounce);
      clock.flushMicrotasks();
      expect(batches, [
        <String>['new-workspace'],
      ]);

      queue.dispose();
      queue.enqueue(const ['ignored'], immediate: true);
      clock.elapse(debounce * 2);
      expect(batches, hasLength(1));
    });
  });
}
