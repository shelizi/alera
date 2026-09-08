import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:dart_mappable/dart_mappable.dart';

part 'external_editor_launcher.mapper.dart';

@MappableEnum()
enum ExternalEditorKind { zed }

@MappableEnum()
enum ExternalEditorExecutableMode { automatic, custom }

@MappableEnum()
enum ExternalEditorWorkspaceMode { newWindow, defaultWindow }

class const ExternalEditorOpenRequest({
  required final String workspacePath,
  required final String filePath,
  final int? line,
  final int? column,
});

abstract interface class ExternalEditorLauncher {
  Future<ExternalEditorLaunchResult> openWorkspace(String workspacePath);

  Future<ExternalEditorLaunchResult> openFile(
    ExternalEditorOpenRequest request,
  );

  Future<ExternalEditorAvailability> checkAvailability();
}
