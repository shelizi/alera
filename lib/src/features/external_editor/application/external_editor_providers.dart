import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/infra/zed_external_editor_launcher.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/shared/infra/process/process_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'external_editor_providers.g.dart';

@Riverpod(keepAlive: true)
ExternalEditorLauncher externalEditorLauncher(Ref ref) {
  final editorConfig = ref.watch(
    settingsControllerProvider.select(
      (settings) => (
        executableMode: settings.editor.zedExecutableMode,
        executablePath: settings.editor.zedExecutablePath,
        workspaceMode: settings.editor.externalEditorWorkspaceMode,
      ),
    ),
  );
  return ZedExternalEditorLauncher(
    processRunner: ref.watch(processRunnerProvider),
    commandReader: () =>
        editorConfig.executableMode == ExternalEditorExecutableMode.custom
        ? editorConfig.executablePath
        : null,
    workspaceModeReader: () => editorConfig.workspaceMode,
  );
}
