import 'package:alera/src/features/workbench/application/workbench_source_control_root_prefs.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('focus source control root updates only root and panel preferences', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      sourceControlRootByWorkspaceId: const <String, String>{
        'other-workspace': 'packages/other',
      },
      activeContextPanelTab: WorkbenchContextPanelTab.explorer,
      rightSidebarVisible: false,
      selectedProjectIds: const <String>{'project-a'},
    );

    final next = focusWorkbenchSourceControlRootPrefs(
      prefs: prefs,
      workspaceId: 'workspace-a',
      relativeRoot: 'packages/app',
    );

    expect(next.sourceControlRootByWorkspaceId, <String, String>{
      'other-workspace': 'packages/other',
      'workspace-a': 'packages/app',
    });
    expect(next.activeContextPanelTab, WorkbenchContextPanelTab.gitDiff);
    expect(next.rightSidebarVisible, isTrue);
    expect(next.selectedProjectIds, same(prefs.selectedProjectIds));
  });

  test(
    'clear source control root removes only the target and no-ops if absent',
    () {
      final prefs = WorkbenchViewPrefs.defaults.copyWith(
        sourceControlRootByWorkspaceId: const <String, String>{
          'workspace-a': 'packages/app',
          'workspace-b': 'packages/other',
        },
        activeContextPanelTab: WorkbenchContextPanelTab.gitDiff,
        rightSidebarVisible: true,
      );

      final next = clearWorkbenchSourceControlRootPrefs(
        prefs: prefs,
        workspaceId: 'workspace-a',
      );

      expect(next?.sourceControlRootByWorkspaceId, <String, String>{
        'workspace-b': 'packages/other',
      });
      expect(next?.activeContextPanelTab, prefs.activeContextPanelTab);
      expect(next?.rightSidebarVisible, prefs.rightSidebarVisible);
      expect(
        clearWorkbenchSourceControlRootPrefs(
          prefs: prefs,
          workspaceId: 'missing',
        ),
        isNull,
      );
    },
  );

  test('path move updates exact and nested focused source control roots', () {
    final exact = WorkbenchViewPrefs.defaults.copyWith(
      sourceControlRootByWorkspaceId: const <String, String>{
        'workspace': 'packages/app',
      },
    );
    final exactNext = syncWorkbenchSourceControlRootAfterPathMovePrefs(
      prefs: exact,
      workspaceId: 'workspace',
      oldRelativePath: './packages\\app',
      newRelativePath: 'apps/main',
    );
    expect(exactNext?.sourceControlRootByWorkspaceId['workspace'], 'apps/main');

    final nested = WorkbenchViewPrefs.defaults.copyWith(
      sourceControlRootByWorkspaceId: const <String, String>{
        'workspace': 'packages/app/src',
      },
    );
    final nestedNext = syncWorkbenchSourceControlRootAfterPathMovePrefs(
      prefs: nested,
      workspaceId: 'workspace',
      oldRelativePath: 'packages/app',
      newRelativePath: 'apps/main',
    );
    expect(
      nestedNext?.sourceControlRootByWorkspaceId['workspace'],
      'apps/main/src',
    );
  });

  test('path move returns null when root is absent, unrelated, or invalid', () {
    final prefs = WorkbenchViewPrefs.defaults.copyWith(
      sourceControlRootByWorkspaceId: const <String, String>{
        'workspace': 'packages/app',
      },
    );

    expect(
      syncWorkbenchSourceControlRootAfterPathMovePrefs(
        prefs: prefs,
        workspaceId: 'missing',
        oldRelativePath: 'packages/app',
        newRelativePath: 'apps/main',
      ),
      isNull,
    );
    expect(
      syncWorkbenchSourceControlRootAfterPathMovePrefs(
        prefs: prefs,
        workspaceId: 'workspace',
        oldRelativePath: 'packages/other',
        newRelativePath: 'apps/main',
      ),
      isNull,
    );
    expect(
      syncWorkbenchSourceControlRootAfterPathMovePrefs(
        prefs: prefs,
        workspaceId: 'workspace',
        oldRelativePath: '../packages',
        newRelativePath: 'apps/main',
      ),
      isNull,
    );
  });
}
