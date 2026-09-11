import 'package:alera/src/features/resource_manager/application/resource_manager_providers.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';

/// Binds Resource Manager's narrow orphan-session termination port to the
/// shared terminal-host socket at the app composition root.
resourceManagerTerminalBindingOverride() {
  return resourceSessionTerminatorProvider.overrideWith(
    (ref) => ref.watch(socketTerminalHostClientProvider).terminate,
  );
}
