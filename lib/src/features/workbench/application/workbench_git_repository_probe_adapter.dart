import 'package:alera/src/features/workbench/application/workbench_source_control_folder_focus_service.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';

final class WorkbenchGitRepositoryProbeAdapter
    implements WorkbenchGitRepositoryProbe {
  const WorkbenchGitRepositoryProbeAdapter(this._backend);

  final GitBackend _backend;

  @override
  Future<bool> isGitRepository(String path) => _backend.isGitRepository(path);
}
