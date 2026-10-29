import 'package:flutter/foundation.dart';

/// Minimal metadata required by non-terminal features that show live titles.
/// Never creates a terminal session or exposes its renderer/PTY controls.
class const TerminalSessionTitleSnapshot({
  required final String workspaceId,
  required final String? terminalSessionId,
  required final String displayTitle,
  required final Listenable titleUpdates,
});

abstract interface class TerminalSessionTitleSource {
  /// Returns an existing tab's title metadata, or null if it is not mounted.
  TerminalSessionTitleSnapshot? peekTitleForTab(String tabId);
}
