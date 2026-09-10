import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_root_subscription_registry.dart';
import 'package:alera/src/features/workbench/application/workspace_section_repository.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('owns section, project, and view prefs subscriptions', () {
    final registry = WorkbenchRootSubscriptionRegistry();
    final sections = StreamController<WorkspaceSectionSnapshot>();
    final projects = StreamController<List<Project>>();
    final prefs = StreamController<WorkbenchViewPrefs>();
    addTearDown(sections.close);
    addTearDown(projects.close);
    addTearDown(prefs.close);

    registry.watchSections(sections.stream, onData: (_) {});
    registry.watchProjects(projects.stream, onData: (_) {});
    registry.watchViewPrefs(prefs.stream, onData: (_) {});

    expect(registry.hasSections, isTrue);
    expect(registry.hasProjects, isTrue);
    expect(registry.hasViewPrefs, isTrue);
  });

  test('completed project watcher releases its slot', () async {
    final registry = WorkbenchRootSubscriptionRegistry();
    final projects = StreamController<List<Project>>();

    registry.watchProjects(projects.stream, onData: (_) {});
    expect(registry.hasProjects, isTrue);

    await projects.close();
    await _flush();

    expect(registry.hasProjects, isFalse);
  });

  test('recovering project watcher re-subscribes after completion', () async {
    final first = StreamController<List<Project>>();
    final second = StreamController<List<Project>>();
    final registry = WorkbenchRootSubscriptionRegistry();
    addTearDown(() {
      registry.cancelAll();
      return second.close();
    });
    var attempts = 0;
    final snapshots = <List<Project>>[];

    registry.watchProjectsRecovering(
      () => attempts++ == 0 ? first.stream : second.stream,
      onData: snapshots.add,
      restartDelay: Duration.zero,
    );

    await first.close();
    await _flush();
    await _flush();

    expect(attempts, 2);
    expect(second.hasListener, isTrue);
    second.add(const <Project>[]);
    await _flush();
    expect(snapshots, hasLength(1));
  });

  test('cancelAll cancels a pending project watcher restart', () async {
    final first = StreamController<List<Project>>();
    final registry = WorkbenchRootSubscriptionRegistry();
    var attempts = 0;

    registry.watchProjectsRecovering(
      () {
        attempts += 1;
        return first.stream;
      },
      onData: (_) {},
      restartDelay: const Duration(milliseconds: 25),
    );

    await first.close();
    registry.cancelAll();
    await Future<void>.delayed(const Duration(milliseconds: 40));

    expect(attempts, 1);
    expect(registry.hasProjects, isFalse);
  });

  test('old watcher completion cannot clear its replacement', () async {
    final cancellationGate = Completer<void>();
    final first = StreamController<List<Project>>(
      onCancel: () => cancellationGate.future,
    );
    final second = StreamController<List<Project>>();
    addTearDown(first.close);
    addTearDown(second.close);
    final registry = WorkbenchRootSubscriptionRegistry();

    registry.watchProjects(first.stream, onData: (_) {});
    registry.watchProjects(second.stream, onData: (_) {});
    expect(registry.hasProjects, isTrue);

    cancellationGate.complete();
    await _flush();

    expect(registry.hasProjects, isTrue);
  });

  test('cancelAll clears and cancels every root subscription', () async {
    var cancelCalls = 0;
    final sections = StreamController<WorkspaceSectionSnapshot>(
      onCancel: () => cancelCalls += 1,
    );
    final projects = StreamController<List<Project>>(
      onCancel: () => cancelCalls += 1,
    );
    final prefs = StreamController<WorkbenchViewPrefs>(
      onCancel: () => cancelCalls += 1,
    );
    addTearDown(sections.close);
    addTearDown(projects.close);
    addTearDown(prefs.close);
    final registry = WorkbenchRootSubscriptionRegistry();

    registry.watchSections(sections.stream, onData: (_) {});
    registry.watchProjects(projects.stream, onData: (_) {});
    registry.watchViewPrefs(prefs.stream, onData: (_) {});
    registry.cancelAll();
    await _flush();

    expect(registry.hasSections, isFalse);
    expect(registry.hasProjects, isFalse);
    expect(registry.hasViewPrefs, isFalse);
    expect(cancelCalls, 3);
  });
}

Future<void> _flush() => Future<void>.delayed(Duration.zero);
