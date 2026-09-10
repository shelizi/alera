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
}
