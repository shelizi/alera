import 'package:alera/src/features/workbench/application/terminal_runtime_focus.dart';
import 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
import 'package:alera/src/features/workbench/application/terminal_session_title_source.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

export 'package:alera/src/features/workbench/application/terminal_runtime_focus.dart';
export 'package:alera/src/features/workbench/application/terminal_runtime_lifecycle.dart';
export 'package:alera/src/features/workbench/application/terminal_session_title_source.dart';
export 'package:alera/src/features/workbench/application/terminal_runtime_user_input.dart';

part 'terminal_runtime_bindings.g.dart';

/// Application-facing aggregate for the terminal runtime capabilities that do
/// not expose presentation session handles or rendering APIs.
abstract interface class TerminalRuntimeBindings
    implements
        TerminalRuntimeLifecycle,
        TerminalRuntimeCoordination,
        TerminalRuntimeFocus {}

@Riverpod(keepAlive: true)
TerminalRuntimeBindings terminalRuntimeBindings(Ref ref) {
  throw StateError(
    'Terminal runtime bindings must be provided by the app composition root.',
  );
}

@Riverpod(keepAlive: true)
TerminalRuntimeLifecycle terminalRuntimeLifecycle(Ref ref) {
  return ref.watch(terminalRuntimeBindingsProvider);
}

@Riverpod(keepAlive: true)
TerminalRuntimeCoordination terminalRuntimeCoordination(Ref ref) {
  return ref.watch(terminalRuntimeBindingsProvider);
}

@Riverpod(keepAlive: true)
TerminalRuntimeFocus terminalRuntimeFocus(Ref ref) {
  return ref.watch(terminalRuntimeBindingsProvider);
}

/// The composition root binds this read-only metadata port to the same runtime
/// as terminalRuntimeBindings. Application consumers never touch view handles.
@Riverpod(keepAlive: true)
TerminalSessionTitleSource terminalSessionTitleSource(Ref ref) {
  throw StateError(
    'Terminal session title source must be provided by the app composition root.',
  );
}
