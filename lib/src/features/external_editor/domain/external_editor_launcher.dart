import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:dart_mappable/dart_mappable.dart';

part 'external_editor_launcher.mapper.dart';

@MappableEnum()
enum ExternalEditorKind { zed, vscode }

@MappableEnum()
enum CodeOpenTarget { alera, external }

@MappableEnum()
enum ExternalEditorWorkspaceMode { newWindow, defaultWindow }

class const ExternalEditorOpenRequest({
  required final String workspacePath,
  required final String filePath,
  final int? line,
  final int? column,
});

class const ExternalEditorOpenFilesRequest({
  required final String workspacePath,
  required final List<String> filePaths,
});

abstract interface class ExternalEditorLauncher {
  Future<ExternalEditorLaunchResult> openWorkspace(String workspacePath);

  Future<ExternalEditorLaunchResult> openFile(
    ExternalEditorOpenRequest request,
  );

  Future<ExternalEditorLaunchResult> openFiles(
    ExternalEditorOpenFilesRequest request,
  );

  /// Fast resolution-only probe used to gate menus. Unlike
  /// [checkAvailability] this never spawns the editor binary.
  Future<bool> isInstalled();

  Future<ExternalEditorAvailability> checkAvailability();
}
