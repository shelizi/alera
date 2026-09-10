final class WorkbenchWorkspaceTabClosingScope {
  final Map<String, int> _depthByWorkspaceId = <String, int>{};

  bool isClosing(String workspaceId) =>
      (_depthByWorkspaceId[workspaceId] ?? 0) > 0;

  Future<T> run<T>(String workspaceId, Future<T> Function() action) async {
    _depthByWorkspaceId.update(
      workspaceId,
      (depth) => depth + 1,
      ifAbsent: () => 1,
    );
    try {
      return await action();
    } finally {
      final depth = _depthByWorkspaceId[workspaceId] ?? 0;
      if (depth <= 1) {
        _depthByWorkspaceId.remove(workspaceId);
      } else {
        _depthByWorkspaceId[workspaceId] = depth - 1;
      }
    }
  }
}
