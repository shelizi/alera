import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/features/workbench/application/workspace_search_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_search_focus_request.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/presentation/workspace_search_panel.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'file search refocus requests restore query focus and selection',
    (tester) async {
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final workspace = _workspace(id: 'workspace-a', path: '/workspace-a');
      await _pumpSearchPanel(
        tester,
        container: container,
        workspace: workspace,
      );
      await tester.pumpAndSettle();

      final input = find.byType(TextField).first;
      final field = tester.widget<TextField>(input);
      expect(field.focusNode?.hasFocus, isTrue);
      await tester.enterText(input, 'sample');
      await tester.pump();
      await tester.tap(find.byTooltip('View as tree'));
      await tester.pump();
      // Clicking the toolbar does not necessarily move keyboard focus.
      Focus.of(tester.element(find.byIcon(AleraIcons.listView))).requestFocus();
      await tester.pump();
      expect(field.focusNode?.hasFocus, isFalse);

      container.read(workspaceSearchFocusRequestProvider.notifier).request();
      await tester.pump();
      expect(field.focusNode?.hasFocus, isTrue);
      expect(field.controller?.text, 'sample');
      expect(
        field.controller?.selection,
        const TextSelection(baseOffset: 0, extentOffset: 6),
      );
      // Cancel the controller's debounced filesystem search before teardown.
      container
          .read(workspaceSearchControllerProvider(workspace.id).notifier)
          .setQuery(workspace.path, '');
      await tester.pump();
    },
  );

  testWidgets('reopening hidden file search restores focus', (tester) async {
    final container = ProviderContainer();
    addTearDown(container.dispose);
    final workspace = _workspace(id: 'workspace-a', path: '/workspace-a');
    await _pumpSearchPanel(
      tester,
      container: container,
      workspace: workspace,
      visible: false,
    );
    await tester.pumpAndSettle();
    final input = find.byType(TextField).first;
    final field = tester.widget<TextField>(input);
    expect(field.focusNode?.hasFocus, isFalse);

    await _pumpSearchPanel(
      tester,
      container: container,
      workspace: workspace,
      visible: true,
    );
    await tester.pumpAndSettle();
    expect(field.focusNode?.hasFocus, isTrue);
  });

  testWidgets('search toolbar keeps refresh rightmost and toggles view icon', (
    tester,
  ) async {
    final container = ProviderContainer();
    addTearDown(container.dispose);

    await _pumpSearchPanel(
      tester,
      container: container,
      workspace: _workspace(id: 'workspace-a', path: '/workspace-a'),
    );

    expect(find.byTooltip('View as tree'), findsOneWidget);
    expect(find.byIcon(AleraIcons.gitGraph), findsOneWidget);
    expect(find.byIcon(AleraIcons.listView), findsNothing);
    expect(
      tester.getCenter(find.byTooltip('Refresh')).dx,
      greaterThan(tester.getCenter(find.byTooltip('Collapse All')).dx),
    );

    await tester.tap(find.byTooltip('View as tree'));
    await tester.pump();

    expect(find.byTooltip('View as list'), findsOneWidget);
    expect(find.byIcon(AleraIcons.gitGraph), findsNothing);
    expect(find.byIcon(AleraIcons.listView), findsOneWidget);
  });

  testWidgets('toggles search ignored files action in toolbar', (tester) async {
    final container = ProviderContainer();
    addTearDown(container.dispose);

    await _pumpSearchPanel(
      tester,
      container: container,
      workspace: _workspace(id: 'workspace-a', path: '/workspace-a'),
    );

    expect(find.byTooltip('Search ignored files'), findsOneWidget);
    expect(find.byTooltip('Ignore ignored files'), findsNothing);

    await tester.tap(find.byIcon(AleraIcons.hidden));
    await tester.pump();

    expect(find.byTooltip('Search ignored files'), findsNothing);
    expect(find.byTooltip('Ignore ignored files'), findsOneWidget);
  });

  testWidgets('shows active replacement and filters after workspace switch', (
    tester,
  ) async {
    final container = ProviderContainer();
    addTearDown(container.dispose);

    final workspaceA = _workspace(id: 'workspace-a', path: '/workspace-a');
    final workspaceB = _workspace(id: 'workspace-b', path: '/workspace-b');
    final workspaceBController = container.read(
      workspaceSearchControllerProvider(workspaceB.id).notifier,
    );
    workspaceBController.setReplacement(workspaceB.path, 'replacement text');
    workspaceBController.setIncludePattern(workspaceB.path, 'lib/**');
    workspaceBController.setExcludePattern(workspaceB.path, 'build/**');

    await _pumpSearchPanel(tester, container: container, workspace: workspaceA);
    await tester.pumpAndSettle();

    expect(find.byType(TextField), findsOneWidget);
    expect(_textFieldWithValue('replacement text'), findsNothing);
    expect(_textFieldWithValue('lib/**'), findsNothing);
    expect(_textFieldWithValue('build/**'), findsNothing);

    await _pumpSearchPanel(tester, container: container, workspace: workspaceB);
    await tester.pumpAndSettle();

    expect(find.byType(TextField), findsNWidgets(4));
    expect(_textFieldWithValue('replacement text'), findsOneWidget);
    expect(_textFieldWithValue('lib/**'), findsOneWidget);
    expect(_textFieldWithValue('build/**'), findsOneWidget);
  });
}

Future<void> _pumpSearchPanel(
  WidgetTester tester, {
  required ProviderContainer container,
  required Workspace workspace,
  bool visible = true,
}) {
  return tester.pumpWidget(
    UncontrolledProviderScope(
      container: container,
      child: MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 320,
            height: 520,
            child: WorkspaceSearchPanel(
              workspace: workspace,
              visible: visible,
              onOpenMatch: (_) {},
            ),
          ),
        ),
      ),
    ),
  );
}

Finder _textFieldWithValue(String value) {
  return find.byWidgetPredicate(
    (widget) => widget is TextField && widget.controller?.text == value,
  );
}

Workspace _workspace({required String id, required String path}) {
  final now = DateTime(2026, 1);
  return Workspace(
    id: id,
    projectId: 'project-1',
    name: id,
    path: path,
    createdAt: now,
    updatedAt: now,
    kind: .main,
    status: .active,
  );
}
