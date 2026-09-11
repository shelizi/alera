import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tree_pin_coordinator.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'pins the root and descendants needing changes in policy order',
    () async {
      final root = _workspace('root', isPinned: false);
      final child = _workspace(
        'child',
        parentWorkspaceId: root.id,
        isPinned: false,
      );
      final alreadyPinned = _workspace(
        'already-pinned',
        parentWorkspaceId: child.id,
        isPinned: true,
      );
      final events = <String>[];
      final coordinator = WorkbenchWorkspaceTreePinCoordinator(
        setPinned: ({required workspaceId, required isPinned}) async {
          events.add('$workspaceId:$isPinned');
        },
      );

      await coordinator.run(
        state: _state(<Workspace>[root, child, alreadyPinned]),
        workspaceId: root.id,
        isPinned: true,
      );

      expect(events, <String>['root:true', 'child:true']);
    },
  );

  test('failure preserves earlier successes and stops later targets', () async {
    final root = _workspace('root', isPinned: false);
    final child = _workspace(
      'child',
      parentWorkspaceId: root.id,
      isPinned: false,
    );
    final grandchild = _workspace(
      'grandchild',
      parentWorkspaceId: child.id,
      isPinned: false,
    );
    final events = <String>[];
    final coordinator = WorkbenchWorkspaceTreePinCoordinator(
      setPinned: ({required workspaceId, required isPinned}) async {
        events.add(workspaceId);
        if (workspaceId == child.id) {
          throw StateError('pin failed');
        }
      },
    );

    await expectLater(
      coordinator.run(
        state: _state(<Workspace>[root, child, grandchild]),
        workspaceId: root.id,
        isPinned: true,
      ),
      throwsA(isA<StateError>()),
    );

    expect(events, <String>[root.id, child.id]);
  });

  test(
    'does not invoke persistence when the whole tree already matches',
    () async {
      final root = _workspace('root', isPinned: true);
      final child = _workspace(
        'child',
        parentWorkspaceId: root.id,
        isPinned: true,
      );
      var calls = 0;
      final coordinator = WorkbenchWorkspaceTreePinCoordinator(
        setPinned: ({required workspaceId, required isPinned}) async {
          calls++;
        },
      );

      await coordinator.run(
        state: _state(<Workspace>[root, child]),
        workspaceId: root.id,
        isPinned: true,
      );

      expect(calls, 0);
    },
  );
}

WorkbenchState _state(List<Workspace> workspaces) => WorkbenchState(
  workspacesByProject: <String, List<Workspace>>{'project': workspaces},
);

Workspace _workspace(
  String id, {
  String? parentWorkspaceId,
  required bool isPinned,
}) => Workspace(
  id: id,
  projectId: 'project',
  name: id,
  path: 'C:/$id',
  createdAt: DateTime.utc(2026, 9, 11),
  updatedAt: DateTime.utc(2026, 9, 11),
  kind: WorkspaceKind.linked,
  status: WorkspaceStatus.active,
  parentWorkspaceId: parentWorkspaceId,
  isPinned: isPinned,
);
