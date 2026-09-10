import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workbench_workspace_tag_update_plan.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('normalizes requested tags and returns only assignment diffs', () {
    final now = DateTime.utc(2026, 9, 10);
    final workspace = _workspace(
      'workspace',
      now,
      tagIds: const <String>['old', 'keep'],
    );

    final plan = planWorkbenchWorkspaceTagUpdate(
      state: const WorkbenchState(),
      workspace: workspace,
      requestedTagIds: const <String>{' keep ', 'new', ' ', 'new '},
    );

    expect(plan.tagIdsToRemove, <String>{'old'});
    expect(plan.tagIdsToAdd, <String>{'new'});
  });

  test('freshest workspace state wins over a stale caller snapshot', () {
    final now = DateTime.utc(2026, 9, 10);
    final stale = _workspace('workspace', now, tagIds: const <String>['stale']);
    final current = stale.copyWith(tagIds: const <String>['current']);
    final state = WorkbenchState(
      workspacesByProject: <String, List<Workspace>>{
        stale.projectId: <Workspace>[current],
      },
    );

    final plan = planWorkbenchWorkspaceTagUpdate(
      state: state,
      workspace: stale,
      requestedTagIds: const <String>{'new'},
    );

    expect(plan.tagIdsToRemove, <String>{'current'});
    expect(plan.tagIdsToAdd, <String>{'new'});
    expect(plan.tagIdsToRemove, isNot(contains('stale')));
  });

  test(
    'falls back to caller workspace when state has no matching workspace',
    () {
      final now = DateTime.utc(2026, 9, 10);
      final workspace = _workspace(
        'workspace',
        now,
        tagIds: const <String>['old'],
      );

      final plan = planWorkbenchWorkspaceTagUpdate(
        state: const WorkbenchState(),
        workspace: workspace,
        requestedTagIds: const <String>{'new'},
      );

      expect(plan.tagIdsToRemove, <String>{'old'});
      expect(plan.tagIdsToAdd, <String>{'new'});
    },
  );

  test('returns empty diffs when normalized membership is unchanged', () {
    final now = DateTime.utc(2026, 9, 10);
    final workspace = _workspace(
      'workspace',
      now,
      tagIds: const <String>['keep', 'other'],
    );

    final plan = planWorkbenchWorkspaceTagUpdate(
      state: const WorkbenchState(),
      workspace: workspace,
      requestedTagIds: const <String>{' other ', 'keep'},
    );

    expect(plan.tagIdsToRemove, isEmpty);
    expect(plan.tagIdsToAdd, isEmpty);
  });
}

Workspace _workspace(String id, DateTime now, {required List<String> tagIds}) =>
    Workspace(
      id: id,
      projectId: 'project',
      name: id,
      path: 'C:/$id',
      createdAt: now,
      updatedAt: now,
      kind: WorkspaceKind.linked,
      status: WorkspaceStatus.active,
      tagIds: tagIds,
    );
