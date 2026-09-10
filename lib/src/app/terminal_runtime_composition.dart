import 'package:alera/src/features/workbench/application/terminal_runtime_bindings.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';

/// Binds application-facing terminal capabilities to the one presentation
/// runtime instance owned by the app composition root.
terminalRuntimeBindingOverride() {
  return terminalRuntimeBindingsProvider.overrideWith(
    (ref) => ref.watch(terminalRuntimeProvider),
  );
}
