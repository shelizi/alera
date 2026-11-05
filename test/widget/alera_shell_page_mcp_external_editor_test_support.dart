part of 'alera_shell_page_test.dart';

class _McpFakeExternalEditor implements ExternalEditorLauncher {
  bool installed = true;
  ExternalEditorLaunchResult result =
      ExternalEditorLaunchResultFactories.opened;
  final List<ExternalEditorOpenRequest> requests =
      <ExternalEditorOpenRequest>[];

  @override
  Future<bool> isInstalled() async => installed;

  @override
  Future<ExternalEditorLaunchResult> openFile(
    ExternalEditorOpenRequest request,
  ) async {
    requests.add(request);
    return result;
  }

  @override
  Future<ExternalEditorLaunchResult> openFiles(
    ExternalEditorOpenFilesRequest request,
  ) async => result;

  @override
  Future<ExternalEditorLaunchResult> openWorkspace(
    String workspacePath,
  ) async => result;

  @override
  Future<ExternalEditorAvailability> checkAvailability() async =>
      ExternalEditorAvailability(available: installed);
}
