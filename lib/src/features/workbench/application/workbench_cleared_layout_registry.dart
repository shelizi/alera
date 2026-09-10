final class WorkbenchClearedLayoutRegistry {
  final Set<String> _workspaceIds = <String>{};

  bool contains(String workspaceId) => _workspaceIds.contains(workspaceId);

  void mark(String workspaceId) {
    _workspaceIds.add(workspaceId);
  }

  void forget(String workspaceId) {
    _workspaceIds.remove(workspaceId);
  }

  void forgetAll(Iterable<String> workspaceIds) {
    _workspaceIds.removeAll(workspaceIds);
  }
}
