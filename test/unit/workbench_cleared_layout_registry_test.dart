import 'package:alera/src/features/workbench/application/workbench_cleared_layout_registry.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('tracks workspaces whose layouts were explicitly cleared', () {
    final registry = WorkbenchClearedLayoutRegistry();

    expect(registry.contains('workspace'), isFalse);
    registry.mark('workspace');
    expect(registry.contains('workspace'), isTrue);

    registry.forget('workspace');
    expect(registry.contains('workspace'), isFalse);
  });

  test('mark and forget are idempotent', () {
    final registry = WorkbenchClearedLayoutRegistry();

    registry.mark('workspace');
    registry.mark('workspace');
    expect(registry.contains('workspace'), isTrue);

    registry.forget('workspace');
    registry.forget('workspace');
    expect(registry.contains('workspace'), isFalse);
  });

  test('forgets a removed set without disturbing live workspaces', () {
    final registry = WorkbenchClearedLayoutRegistry();
    registry.mark('keep');
    registry.mark('remove-a');
    registry.mark('remove-b');

    registry.forgetAll(const <String>['remove-a', 'remove-b']);

    expect(registry.contains('keep'), isTrue);
    expect(registry.contains('remove-a'), isFalse);
    expect(registry.contains('remove-b'), isFalse);
  });
}
