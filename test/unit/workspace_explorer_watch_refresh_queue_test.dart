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
  test('continuous changes never starve Explorer refresh', () {
    fakeAsync((clock) {
      final batches = <List<String>>[];
      final queue = WorkspaceExplorerWatchRefreshQueue(
        refresh: (paths) async => batches.add(paths),
        debounce: const Duration(milliseconds: 120),
        maxWait: const Duration(milliseconds: 350),
      );

      for (var index = 0; index < 40; index++) {
        queue.enqueue(const ['lib', 'lib', 'src']);
        clock.elapse(const Duration(milliseconds: 40));
      }

      // No 120-ms idle gap occurred, but the 350-ms maximum wait forces
      // several snapshots instead of hiding all updates until the build ends.
      expect(batches.length, greaterThanOrEqualTo(4));
      expect(batches.length, lessThan(9));
      expect(batches.every((batch) => batch.length == 2), isTrue);
      queue.dispose();
    });
  });

  test('a due trailing batch runs when an in-flight scan completes', () {
    fakeAsync((clock) {
      final firstRead = Completer<void>();
      final batches = <List<String>>[];
      final queue = WorkspaceExplorerWatchRefreshQueue(
        refresh: (paths) {
          batches.add(paths);
          return batches.length == 1 ? firstRead.future : Future<void>.value();
        },
        debounce: const Duration(milliseconds: 120),
        maxWait: const Duration(milliseconds: 250),
      );
      queue.enqueue(const ['src'], immediate: true);
      for (var index = 0; index < 20; index++) {
        queue.enqueue(const ['test']);
        clock.elapse(const Duration(milliseconds: 30));
      }
      expect(batches, [
        <String>['src'],
      ]);
      firstRead.complete();
      clock.flushMicrotasks();
      expect(batches, [
        <String>['src'],
        <String>['test'],
      ]);
      queue.dispose();
    });
  });
}
