import 'package:alera/src/features/workbench/application/terminal_runtime_bindings.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';

/// Binds application-facing terminal capabilities to the one presentation
/// runtime instance owned by the app composition root.
// Riverpod returns its opaque provider override type; infer it at this
// composition edge rather than widening to dynamic or a generic Object.
// ignore: strict_top_level_inference
terminalRuntimeBindingOverride() {
  return terminalRuntimeBindingsProvider.overrideWith(
    (ref) => ref.watch(terminalRuntimeProvider),
  );
}

// ignore: strict_top_level_inference
terminalSessionTitleSourceOverride() {
  return terminalSessionTitleSourceProvider.overrideWith(
    (ref) => _TerminalSessionTitleSource(ref.watch(terminalRuntimeProvider)),
  );
}

final class _TerminalSessionTitleSource(final TerminalRuntime _runtime)
    implements TerminalSessionTitleSource {
  @override
  TerminalSessionTitleSnapshot? peekTitleForTab(String tabId) {
    final session = _runtime.peekSession(tabId);
    if (session == null) {
      return null;
    }
    return TerminalSessionTitleSnapshot(
      workspaceId: session.workspaceId,
      terminalSessionId: session.terminalSessionId,
      displayTitle: session.displayTitle,
      titleUpdates: session.titleListenable,
    );
  }
}
