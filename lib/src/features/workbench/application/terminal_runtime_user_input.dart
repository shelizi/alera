/// Reports workspaces whose terminal the user typed, pasted or submitted into.
/// Kept apart from the runtime interfaces because only the production runtime
/// can tell user input apart from the emulator's own replies.
abstract interface class TerminalRuntimeUserInputSource {
  /// Emits a workspace id at most once per [terminalUserInputActivityInterval]
  /// for each workspace, so typing does not re-sort the sidebar per keystroke.
  Stream<String> get userInputWorkspaceIds;
}

const Duration terminalUserInputActivityInterval = Duration(seconds: 30);
