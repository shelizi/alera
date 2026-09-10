import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_bootstrap_orchestrator.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('runs bootstrap effects in the established order', () async {
    final events = <String>[];
    final projects = <Project>[_project('first'), _project('second')];
    const prefs = WorkbenchViewPrefs.defaults;

    await const WorkbenchBootstrapOrchestrator().run(
      loadViewPrefs: () async {
        events.add('load-prefs');
        return prefs;
      },
      applyViewPrefs: (_) => events.add('apply-prefs'),
      watchViewPrefs: () => events.add('watch-prefs'),
      startSections: () => events.add('start-sections'),
      watchProjects: () => events.add('watch-projects'),
      listProjects: () async {
        events.add('list-projects');
        return projects;
      },
      applyProjects: (_) => events.add('apply-projects'),
      ensureMainWorkspace: (project) async {
        events.add('ensure-${project.id}');
      },
    );

    expect(events, <String>[
      'load-prefs',
      'apply-prefs',
      'watch-prefs',
      'start-sections',
      'watch-projects',
      'list-projects',
      'apply-projects',
      'ensure-first',
      'ensure-second',
    ]);
  });

  test(
    'skips preference apply and watch when no repository is available',
    () async {
      final events = <String>[];

      await const WorkbenchBootstrapOrchestrator().run(
        loadViewPrefs: () async {
          events.add('load-prefs');
          return null;
        },
        applyViewPrefs: (_) => events.add('apply-prefs'),
        watchViewPrefs: () => events.add('watch-prefs'),
        startSections: () => events.add('start-sections'),
        watchProjects: () => events.add('watch-projects'),
        listProjects: () async {
          events.add('list-projects');
          return const <Project>[];
        },
        applyProjects: (_) => events.add('apply-projects'),
        ensureMainWorkspace: (_) async {},
      );

      expect(events, <String>[
        'load-prefs',
        'start-sections',
        'watch-projects',
        'list-projects',
        'apply-projects',
      ]);
    },
  );

  test('preference phase failures do not block project bootstrap', () async {
    final events = <String>[];

    await const WorkbenchBootstrapOrchestrator().run(
      loadViewPrefs: () async {
        events.add('load-prefs');
        throw StateError('bad prefs');
      },
      applyViewPrefs: (_) => events.add('apply-prefs'),
      watchViewPrefs: () => events.add('watch-prefs'),
      startSections: () => events.add('start-sections'),
      watchProjects: () => events.add('watch-projects'),
      listProjects: () async {
        events.add('list-projects');
        return const <Project>[];
      },
      applyProjects: (_) => events.add('apply-projects'),
      ensureMainWorkspace: (_) async {},
    );

    expect(events, <String>[
      'load-prefs',
      'start-sections',
      'watch-projects',
      'list-projects',
      'apply-projects',
    ]);
  });

  test(
    'project bootstrap failures propagate to the controller boundary',
    () async {
      final events = <String>[];

      await expectLater(
        const WorkbenchBootstrapOrchestrator().run(
          loadViewPrefs: () async => null,
          applyViewPrefs: (_) {},
          watchViewPrefs: () {},
          startSections: () => events.add('start-sections'),
          watchProjects: () => events.add('watch-projects'),
          listProjects: () async {
            events.add('list-projects');
            throw StateError('projects unavailable');
          },
          applyProjects: (_) => events.add('apply-projects'),
          ensureMainWorkspace: (_) async {},
        ),
        throwsStateError,
      );

      expect(events, <String>[
        'start-sections',
        'watch-projects',
        'list-projects',
      ]);
    },
  );
}

Project _project(String id) {
  final now = DateTime.utc(2026, 9, 10);
  return Project(
    id: id,
    name: id,
    repoPath: 'C:/$id',
    createdAt: now,
    updatedAt: now,
  );
}
