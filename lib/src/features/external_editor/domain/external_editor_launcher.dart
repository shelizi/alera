import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';

enum ExternalEditorKind { zed }

enum ExternalEditorWorkspaceMode { newWindow, defaultWindow }

class const ExternalEditorOpenRequest({
  required this.workspacePath,
  required this.filePath,
  this.line,
  this.column,
});

abstract interface class ExternalEditorLauncher {
  Future<ExternalEditorLaunchResult> openWorkspace(String workspacePath);

  Future<ExternalEditorLaunchResult> openFile(ExternalEditorOpenRequest request);

  Future<ExternalEditorAvailability> checkAvailability();
}
