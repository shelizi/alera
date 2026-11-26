part of 'fake_git_backend.dart';

mixin _FakeGitBackendStatus {
  List<GitBackendCall> get calls;
  GitException? get statusError;

  GitStatusResult gitStatusResult = const GitStatusResult(entries: []);
  GitExplorerStatusSnapshot gitExplorerStatusSnapshot = const .empty();

  Future<GitStatusResult> status(String path) async {
    calls.add(GitBackendCall('status', <String, Object?>{'path': path}));
    final error = statusError;
    if (error != null) {
      throw error;
    }
    return gitStatusResult;
  }

  Future<List<String>?> diffChangedPaths(String path) async {
    calls.add(
      GitBackendCall('diffChangedPaths', <String, Object?>{'path': path}),
    );
    final error = statusError;
    if (error != null) throw error;
    if (gitStatusResult.entries.any((entry) {
      final submodule = entry.submodule;
      return submodule != null &&
          (submodule.trackedChanges || submodule.untrackedChanges);
    })) {
      return null;
    }
    return <String>{for (final entry in gitStatusResult.entries) entry.path}
        .toList(growable: false);
  }

  Future<GitExplorerStatusSnapshot> explorerStatusSnapshot(String path) async {
    calls.add(
      GitBackendCall('explorerStatusSnapshot', <String, Object?>{'path': path}),
    );
    return gitExplorerStatusSnapshot;
  }
}
