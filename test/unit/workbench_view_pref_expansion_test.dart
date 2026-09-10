import 'package:alera/src/features/workbench/application/workbench_view_pref_expansion.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('project collapse toggle changes only collapsed project ids', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      collapsedProjectIds: const <String>{'project-a'},
      collapsedParentWorkspaceIds: const <String>{'parent-a'},
      expandedWorkspaceIds: const <String>{'workspace-a'},
    );

    final removed = toggleWorkbenchProjectCollapsedPrefs(
      prefs: prefs,
      projectId: 'project-a',
    );
    expect(removed.collapsedProjectIds, isEmpty);
    expect(
      removed.collapsedParentWorkspaceIds,
      same(prefs.collapsedParentWorkspaceIds),
    );
    expect(removed.expandedWorkspaceIds, same(prefs.expandedWorkspaceIds));

    final added = toggleWorkbenchProjectCollapsedPrefs(
      prefs: prefs,
      projectId: 'project-b',
    );
    expect(added.collapsedProjectIds, <String>{'project-a', 'project-b'});
  });

  test(
    'parent collapse toggle changes only collapsed parent workspace ids',
    () {
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        collapsedProjectIds: const <String>{'project-a'},
        collapsedParentWorkspaceIds: const <String>{'parent-a'},
        expandedWorkspaceIds: const <String>{'workspace-a'},
      );

      final removed = toggleWorkbenchParentWorkspaceCollapsedPrefs(
        prefs: prefs,
        workspaceId: 'parent-a',
      );
      expect(removed.collapsedParentWorkspaceIds, isEmpty);
      expect(removed.collapsedProjectIds, same(prefs.collapsedProjectIds));
      expect(removed.expandedWorkspaceIds, same(prefs.expandedWorkspaceIds));

      final added = toggleWorkbenchParentWorkspaceCollapsedPrefs(
        prefs: prefs,
        workspaceId: 'parent-b',
      );
      expect(added.collapsedParentWorkspaceIds, <String>{
        'parent-a',
        'parent-b',
      });
    },
  );

  test('workspace expansion toggle changes only expanded workspace ids', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      collapsedProjectIds: const <String>{'project-a'},
      collapsedParentWorkspaceIds: const <String>{'parent-a'},
      expandedWorkspaceIds: const <String>{'workspace-a'},
    );

    final removed = toggleWorkbenchWorkspaceExpandedPrefs(
      prefs: prefs,
      workspaceId: 'workspace-a',
    );
    expect(removed.expandedWorkspaceIds, isEmpty);
    expect(removed.collapsedProjectIds, same(prefs.collapsedProjectIds));
    expect(
      removed.collapsedParentWorkspaceIds,
      same(prefs.collapsedParentWorkspaceIds),
    );

    final added = toggleWorkbenchWorkspaceExpandedPrefs(
      prefs: prefs,
      workspaceId: 'workspace-b',
    );
    expect(added.expandedWorkspaceIds, <String>{'workspace-a', 'workspace-b'});
  });

  test('explicit workspace expansion returns null for a no-op', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      expandedWorkspaceIds: const <String>{'workspace-a'},
    );

    expect(
      nextWorkbenchWorkspaceExpandedPrefs(
        prefs: prefs,
        workspaceId: 'workspace-a',
        expanded: true,
      ),
      isNull,
    );
    expect(
      nextWorkbenchWorkspaceExpandedPrefs(
        prefs: prefs,
        workspaceId: 'workspace-b',
        expanded: false,
      ),
      isNull,
    );

    final collapsed = nextWorkbenchWorkspaceExpandedPrefs(
      prefs: prefs,
      workspaceId: 'workspace-a',
      expanded: false,
    );
    expect(collapsed?.expandedWorkspaceIds, isEmpty);

    final expanded = nextWorkbenchWorkspaceExpandedPrefs(
      prefs: prefs,
      workspaceId: 'workspace-b',
      expanded: true,
    );
    expect(expanded?.expandedWorkspaceIds, <String>{
      'workspace-a',
      'workspace-b',
    });
  });
}
