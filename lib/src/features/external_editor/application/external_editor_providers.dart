import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/infra/zed_external_editor_launcher.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/shared/infra/process/process_providers.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'external_editor_providers.g.dart';

@Riverpod(keepAlive: true)
ExternalEditorLauncher externalEditorLauncher(Ref ref) {
  final editor = ref.watch(
    settingsControllerProvider.select((settings) => settings.editor),
  );
  return ZedExternalEditorLauncher(
    processRunner: ref.watch(processRunnerProvider),
    commandReader: () => editor.zedExecutableMode == .custom
        ? editor.zedExecutablePath
        : null,
    workspaceModeReader: () => editor.externalEditorWorkspaceMode,
  );
}
