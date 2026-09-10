abstract interface class WorkbenchReplaceableTabEditorSessions {
  bool isDirty(String tabId);

  void forget(String tabId);
}
