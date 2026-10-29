import 'package:alera/src/features/workbench/application/workbench_git_repository_probe_adapter.dart';
import 'package:alera/src/shared/infra/git/git_repository_probe_port.dart';
import 'package:flutter_test/flutter_test.dart';

final class _RepositoryProbe implements GitRepositoryProbePort {
  final List<String> paths = <String>[];

  @override
  Future<bool> isGitRepository(String path) async {
    paths.add(path);
    return path == '/repository';
  }
}

void main() {
  test(
    'repository focus probe depends on a single read-only operation',
    () async {
      final probe = _RepositoryProbe();
      final adapter = WorkbenchGitRepositoryProbeAdapter(probe);

      expect(await adapter.isGitRepository('/repository'), isTrue);
      expect(await adapter.isGitRepository('/plain-folder'), isFalse);
      expect(probe.paths, <String>['/repository', '/plain-folder']);
    },
  );
}
