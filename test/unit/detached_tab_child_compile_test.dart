import 'package:alera/src/features/workbench/presentation/detached_tab_child_app.dart';
import 'package:alera/src/features/workbench/presentation/workspace_workbench_view.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('native child and real workspace tab surface compile together', () {
    expect(DetachedTabChildApp, isNotNull);
    expect(DetachedWorkspaceTabSurface, isNotNull);
  });
}
