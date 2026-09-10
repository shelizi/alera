import 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('terminal lifecycle provider exposes the narrow contract', () {
    expect(terminalRuntimeLifecycleProvider, isNotNull);
    expect(TerminalRuntimeLifecycle, isNotNull);
  });
}
