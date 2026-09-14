import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/workbench/domain/external_terminal_launcher.dart';
import 'package:alera/src/features/workbench/infra/native_external_terminal_launcher.dart';
import 'package:alera/src/shared/infra/runtime/alera_cli_sidecar.dart';
import 'package:alera/src/shared/infra/process/process_providers.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'external_terminal_providers.g.dart';

@Riverpod(keepAlive: true)
ExternalTerminalLauncher externalTerminalLauncher(Ref ref) {
  return NativeExternalTerminalLauncher(
    processRunner: ref.watch(processRunnerProvider),
    cliResolver: DefaultAleraCliResolver(),
    gitBashExecutablePath: ref
        .watch(settingsControllerProvider)
        .agents
        .gitBashExecutablePath,
  );
}
