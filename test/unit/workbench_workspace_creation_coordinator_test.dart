import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_creation_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_creation_result.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'runs create, reconcile, initialization, and parent attachment in order',
    () async {
      final events = <String>[];
      final result = _result();
      final coordinator = WorkbenchWorkspaceCreationCoordinator(
        createWorkspace: () async {
          events.add('create');
          return result;
        },
        reconcileWorkspace: (project, workspace) {
          events.add('reconcile:${project.id}:${workspace.id}');
        },
        selectWorkspace: (project, workspace) async {
          events.add('select:${project.id}:${workspace.id}');
        },
        openDeferredSetupTab: (creation) async {
          events.add('setup:${creation.workspace.id}');
        },
        attachParent: ({required result, parentWorkspaceId}) async {
          events.add('parent:$parentWorkspaceId:${result.workspace.id}');
          return result;
        },
      );

      final completed = await coordinator.run(
        project: _project(),
        initializeTabs: true,
        parentWorkspaceId: 'parent',
      );

      expect(completed, same(result));
      expect(events, <String>[
        'create',
        'reconcile:project:workspace',
        'select:project:workspace',
        'setup:workspace',
        'parent:parent:workspace',
      ]);
    },
  );

  test('prompt creation skips selection and setup but still reconciles and attaches parent', () async {
    final events = <String>[];
    final result = _result();
    final coordinator = WorkbenchWorkspaceCreationCoordinator(
      createWorkspace: () async {
        events.add('create');
        return result;
      },
      reconcileWorkspace: (_, workspace) =>
          events.add('reconcile:${workspace.id}'),
      selectWorkspace: (_, workspace) async =>
          events.add('select:${workspace.id}'),
      openDeferredSetupTab: (creation) async =>
          events.add('setup:${creation.workspace.id}'),
      attachParent: ({required result, parentWorkspaceId}) async {
        events.add('parent:$parentWorkspaceId');
        return result;
      },
    );

    await coordinator.run(
      project: _project(),
      initializeTabs: false,
      parentWorkspaceId: 'parent',
    );

    expect(events, <String>['create', 'reconcile:workspace', 'parent:parent']);
  });

  test('create failure stops every later step and propagates', () async {
    final events = <String>[];
    final coordinator = WorkbenchWorkspaceCreationCoordinator(
      createWorkspace: () async {
        events.add('create');
        throw StateError('create failed');
      },
      reconcileWorkspace: (_, __) => events.add('reconcile'),
      selectWorkspace: (_, __) async => events.add('select'),
      openDeferredSetupTab: (_) async => events.add('setup'),
      attachParent: ({required result, parentWorkspaceId}) async {
        events.add('parent');
        return result;
      },
    );

    await expectLater(
      coordinator.run(project: _project(), initializeTabs: true),
      throwsA(isA<StateError>()),
    );
    expect(events, <String>['create']);
  });

  test(
    'selection failure stops setup and parent attachment and propagates',
    () async {
      final events = <String>[];
      final coordinator = WorkbenchWorkspaceCreationCoordinator(
        createWorkspace: () async {
          events.add('create');
          return _result();
        },
        reconcileWorkspace: (_, __) => events.add('reconcile'),
        selectWorkspace: (_, __) async {
          events.add('select');
          throw StateError('select failed');
        },
        openDeferredSetupTab: (_) async => events.add('setup'),
        attachParent: ({required result, parentWorkspaceId}) async {
          events.add('parent');
          return result;
        },
      );

      await expectLater(
        coordinator.run(project: _project(), initializeTabs: true),
        throwsA(isA<StateError>()),
      );
      expect(events, <String>['create', 'reconcile', 'select']);
    },
  );

  test('returns the completed result produced by parent attachment', () async {
    final original = _result();
    final completed = WorkspaceCreationResult(
      workspace: original.workspace,
      setupReport: original.setupReport,
      deferredSetupCommand: original.deferredSetupCommand,
      parentLinkError: 'parent warning',
    );
    final coordinator = WorkbenchWorkspaceCreationCoordinator(
      createWorkspace: () async => original,
      reconcileWorkspace: (_, __) {},
      selectWorkspace: (_, __) async {},
      openDeferredSetupTab: (_) async {},
      attachParent: ({required result, parentWorkspaceId}) async => completed,
    );

    final returned = await coordinator.run(
      project: _project(),
      initializeTabs: true,
      parentWorkspaceId: 'parent',
    );

    expect(returned, same(completed));
    expect(returned.parentLinkError, 'parent warning');
  });
}

Project _project() => Project(
  id: 'project',
  name: 'Project',
  repoPath: 'C:/repo',
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
);

WorkspaceCreationResult _result() => WorkspaceCreationResult(
  workspace: Workspace(
    id: 'workspace',
    projectId: 'project',
    name: 'Workspace',
    branch: 'feature/test',
    path: 'C:/repo/workspace',
    createdAt: DateTime.utc(2026, 9, 11),
    updatedAt: DateTime.utc(2026, 9, 11),
    kind: WorkspaceKind.linked,
    status: WorkspaceStatus.active,
  ),
  setupReport: .empty,
  deferredSetupCommand: 'setup',
);
