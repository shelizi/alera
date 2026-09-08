import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_file_open_coordinator.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'workspace_file_open_coordinator_provider.g.dart';

@Riverpod(keepAlive: true)
WorkspaceFileOpenCoordinator workspaceFileOpenCoordinator(Ref ref) {
  final target = ref.watch(settingsControllerProvider).editor.codeOpenTarget;
  return WorkspaceFileOpenCoordinator(
    externalEditorLauncher: ref.watch(externalEditorLauncherProvider),
    defaultTargetReader: () => target,
    implicitFailureNotice: (message) => AleraToast.publish(
      message: message,
      tone: AleraToastTone.info,
      duration: const Duration(seconds: 6),
    ),
  );
}
