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
