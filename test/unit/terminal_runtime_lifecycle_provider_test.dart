import 'package:alera/src/app/terminal_runtime_composition.dart';
import 'package:alera/src/features/workbench/application/terminal_runtime_bindings.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'app composition binds every narrow runtime capability to one runtime',
    () {
      final runtime = _FakeTerminalRuntime();
      final container = ProviderContainer(
        overrides: [
          terminalRuntimeProvider.overrideWithValue(runtime),
          terminalRuntimeBindingOverride(),
        ],
      );
      addTearDown(container.dispose);

      expect(container.read(terminalRuntimeBindingsProvider), same(runtime));
      expect(container.read(terminalRuntimeLifecycleProvider), same(runtime));
      expect(
        container.read(terminalRuntimeCoordinationProvider),
        same(runtime),
      );
      expect(container.read(terminalRuntimeFocusProvider), same(runtime));
    },
  );
}

final class _FakeTerminalRuntime implements TerminalRuntime {
  @override
  Stream<TerminalRuntimeExitEvent> get exits =>
      const Stream<TerminalRuntimeExitEvent>.empty();

  @override
  void closeTab(String tabId) {}

  @override
  void closeWorkspace(String workspaceId) {}

  @override
  void dispose() {}

  @override
  TerminalSessionHandle? peekSession(String tabId) => null;

  @override
  void releaseTab(String tabId) {}

  @override
  void releaseWorkspace(String workspaceId) {}

  @override
  void requestFocus({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) {}

  @override
  TerminalSessionHandle sessionFor({
    required Workspace workspace,
    required WorkspaceTabRecord tab,
  }) => throw UnimplementedError();

  @override
  void setActiveWorkspace(String? workspaceId) {}
}
