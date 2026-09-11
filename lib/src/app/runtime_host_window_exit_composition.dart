import 'package:alera/src/features/app_window/application/app_window_providers.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';

/// Keeps the app-window feature transport-agnostic while preserving the Linux
/// early-exit safety net for the shared runtime-host socket.
runtimeHostWindowExitOverride() {
  return appWindowBeforeLinuxExitProvider.overrideWith((ref) {
    return () async {
      ref.read(socketTerminalHostClientProvider).dispose();
    };
  });
}
