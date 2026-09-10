import 'dart:async';

import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_source_control_folder_focus_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  test('folder repository focus returns a normalized root after both selection checks', () async {
    final events = <String>[];
    final service = WorkbenchSourceControlFolderFocusService(
      gitRepositoryProbe: _FakeGitRepositoryProbe(events: events),
      hasDirectGitEntry: (path) {
        events.add('direct:$path');
        return true;
      },
    );
    var selected = true;

    final result = await service.resolve(
      project: _project(kind: ProjectKind.folder),
      workspace: _workspace(),
      relativePath: './packages\\app',
      isSelectionActive: () {
        events.add('selected:$selected');
        return selected;
      },
    );

    expect(result, 'packages/app');
    final absolutePath = p.join('workspace-path', 'packages', 'app');
    expect(events, <String>[
      'selected:true',
      'direct:$absolutePath',
      'git:$absolutePath',
      'selected:true',
    ]);
  });

  test(
    'non-folder project is rejected before selection or filesystem work',
    () async {
      final events = <String>[];
      final service = WorkbenchSourceControlFolderFocusService(
        gitRepositoryProbe: _FakeGitRepositoryProbe(events: events),
        hasDirectGitEntry: (path) {
          events.add('direct:$path');
          return true;
        },
      );

      final result = await service.resolve(
        project: _project(kind: ProjectKind.gitRepository),
        workspace: _workspace(),
        relativePath: 'packages/app',
        isSelectionActive: () {
          events.add('selected');
          return true;
        },
      );

      expect(result, isNull);
      expect(events, isEmpty);
    },
  );

  test('missing direct git entry skips the backend repository probe', () async {
    final events = <String>[];
    final service = WorkbenchSourceControlFolderFocusService(
      gitRepositoryProbe: _FakeGitRepositoryProbe(events: events),
      hasDirectGitEntry: (path) {
        events.add('direct:$path');
        return false;
      },
    );

    final result = await service.resolve(
      project: _project(kind: ProjectKind.folder),
      workspace: _workspace(),
      relativePath: 'packages/app',
      isSelectionActive: () {
        events.add('selected');
        return true;
      },
    );

    expect(result, isNull);
    final absolutePath = p.join('workspace-path', 'packages', 'app');
    expect(events, <String>['selected', 'direct:$absolutePath']);
  });

  test('non-repository direct git entry is rejected', () async {
    final events = <String>[];
    final service = WorkbenchSourceControlFolderFocusService(
      gitRepositoryProbe: _FakeGitRepositoryProbe(
        events: events,
        isRepository: false,
      ),
      hasDirectGitEntry: (_) => true,
    );

    final result = await service.resolve(
      project: _project(kind: ProjectKind.folder),
      workspace: _workspace(),
      relativePath: 'packages/app',
      isSelectionActive: () => true,
    );

    expect(result, isNull);
    final absolutePath = p.join('workspace-path', 'packages', 'app');
    expect(events, <String>['git:$absolutePath']);
  });

  test(
    'selection becoming stale while git probe is pending rejects completion',
    () async {
      final events = <String>[];
      final gate = Completer<void>();
      final probe = _FakeGitRepositoryProbe(events: events, gate: gate);
      final service = WorkbenchSourceControlFolderFocusService(
        gitRepositoryProbe: probe,
        hasDirectGitEntry: (_) => true,
      );
      var selected = true;

      final future = service.resolve(
        project: _project(kind: ProjectKind.folder),
        workspace: _workspace(),
        relativePath: 'packages/app',
        isSelectionActive: () => selected,
      );
      await probe.started.future;
      selected = false;
      gate.complete();

      expect(await future, isNull);
    },
  );

  test('invalid relative root is rejected before filesystem work', () async {
    final events = <String>[];
    final service = WorkbenchSourceControlFolderFocusService(
      gitRepositoryProbe: _FakeGitRepositoryProbe(events: events),
      hasDirectGitEntry: (path) {
        events.add('direct:$path');
        return true;
      },
    );

    final result = await service.resolve(
      project: _project(kind: ProjectKind.folder),
      workspace: _workspace(),
      relativePath: '../outside',
      isSelectionActive: () => true,
    );

    expect(result, isNull);
    expect(events, isEmpty);
  });
}

final class _FakeGitRepositoryProbe implements WorkbenchGitRepositoryProbe {
  _FakeGitRepositoryProbe({
    required this.events,
    this.isRepository = true,
    this.gate,
  });

  final List<String> events;
  final bool isRepository;
  final Completer<void>? gate;
  final Completer<void> started = Completer<void>();

  @override
  Future<bool> isGitRepository(String path) async {
    events.add('git:$path');
    if (!started.isCompleted) started.complete();
    if (gate case final gate?) await gate.future;
    return isRepository;
  }
}

Project _project({required ProjectKind kind}) => Project(
  id: 'project',
  name: 'Project',
  repoPath: 'workspace-path',
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
  kind: kind,
);

Workspace _workspace() => Workspace(
  id: 'workspace',
  projectId: 'project',
  name: 'Workspace',
  path: 'workspace-path',
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
  kind: WorkspaceKind.main,
  status: WorkspaceStatus.active,
);
