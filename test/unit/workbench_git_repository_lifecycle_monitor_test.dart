import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_git_repository_lifecycle_monitor.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  Project project(String id, String path) => Project(
    id: id,
    name: id,
    repoPath: path,
    kind: ProjectKind.folder,
    createdAt: DateTime.utc(2026),
    updatedAt: DateTime.utc(2026),
  );

  test('only top-level .git directory or pointer-file events count', () {
    const root = '/example/my-project';
    expect(isGitMetadataEntryEvent(root, p.join(root, '.git')), isTrue);
    expect(isGitMetadataEntryEvent(root, p.join(root, 'src', '.git')), isFalse);
    expect(
      isGitMetadataEntryEvent(root, p.join(root, 'src', 'main.dart')),
      isFalse,
    );
  });

  test(
    'initial selection, .git events, and resume trigger re-detection',
    () async {
      final events = StreamController<String>.broadcast(sync: true);
      addTearDown(events.close);
      final refreshed = <String>[];
      final monitor = WorkbenchGitRepositoryLifecycleMonitor(
        refreshProject: (id) async => refreshed.add(id),
        watchRoot: (_) => events.stream,
        debounce: const Duration(milliseconds: 25),
      );
      addTearDown(monitor.dispose);

      monitor.selectProject(project('project-a', '/projects/a'));
      await Future<void>.delayed(Duration.zero);
      expect(refreshed, ['project-a']);

      events.add('/projects/a/src/index.js');
      await Future<void>.delayed(const Duration(milliseconds: 45));
      expect(refreshed, ['project-a']);

      events.add('/projects/a/.git');
      events.add('/projects/a/.git'); // burst is debounced
      await Future<void>.delayed(const Duration(milliseconds: 70));
      expect(refreshed, ['project-a', 'project-a']);

      monitor.onAppResumed();
      await Future<void>.delayed(Duration.zero);
      expect(refreshed, ['project-a', 'project-a', 'project-a']);
    },
  );

  test(
    'switching projects does not leave active watchers on old roots',
    () async {
      final events = StreamController<String>.broadcast(sync: true);
      addTearDown(events.close);
      final refreshed = <String>[];
      final monitor = WorkbenchGitRepositoryLifecycleMonitor(
        refreshProject: (id) async => refreshed.add(id),
        watchRoot: (_) => events.stream,
        debounce: const Duration(milliseconds: 20),
      );
      addTearDown(monitor.dispose);

      monitor.selectProject(project('a', '/projects/a'));
      await Future<void>.delayed(Duration.zero);
      monitor.selectProject(project('b', '/projects/b'));
      await Future<void>.delayed(Duration.zero);
      events.add('/projects/a/.git');
      events.add('/projects/b/.git');
      await Future<void>.delayed(const Duration(milliseconds: 60));
      expect(refreshed, ['a', 'b', 'b']);

      monitor.dispose();
      events.add('/projects/b/.git');
      await Future<void>.delayed(const Duration(milliseconds: 50));
      expect(refreshed, ['a', 'b', 'b']);
    },
  );
}
