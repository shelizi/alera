import 'package:alera/src/features/workbench/application/workbench_layout_state.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workbench_layout.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('applying layout updates layout and active tab for its workspace', () {
    const state = WorkbenchState(
      activeTabIdByWorkspace: <String, String>{'other': 'other-tab'},
    );
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: const <String>['tab-a', 'tab-b'],
    );

    final next = applyWorkbenchLayoutState(state: state, layout: layout);

    expect(next.layoutFor('workspace'), layout);
    expect(next.activeTabIdByWorkspace, <String, String>{
      'other': 'other-tab',
      'workspace': layout.activeTabId!,
    });
    expect(state.layoutFor('workspace'), isNull);
    expect(state.activeTabIdByWorkspace, <String, String>{
      'other': 'other-tab',
    });
  });

  test('layout without active tab clears only that workspace active tab', () {
    const state = WorkbenchState(
      activeTabIdByWorkspace: <String, String>{
        'workspace': 'stale-tab',
        'other': 'other-tab',
      },
    );
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: const <String>[],
    );

    final next = applyWorkbenchLayoutState(state: state, layout: layout);

    expect(next.layoutFor('workspace'), layout);
    expect(next.activeTabIdByWorkspace, <String, String>{'other': 'other-tab'});
  });

  test('setting active tab updates only the target workspace active tab', () {
    final layout = WorkbenchLayout.single(
      workspaceId: 'workspace',
      tabIds: const <String>['layout-tab'],
    );
    final state = WorkbenchState(
      layoutByWorkspace: <String, WorkbenchLayout>{'workspace': layout},
      activeTabIdByWorkspace: const <String, String>{
        'workspace': 'old-tab',
        'other': 'other-tab',
      },
      searchQuery: 'keep-search',
    );

    final next = applyWorkbenchActiveTabState(
      state: state,
      workspaceId: 'workspace',
      tabId: 'selected-tab',
    );

    expect(next.activeTabIdByWorkspace, <String, String>{
      'workspace': 'selected-tab',
      'other': 'other-tab',
    });
    expect(next.layoutByWorkspace, same(state.layoutByWorkspace));
    expect(next.searchQuery, 'keep-search');
  });
}
