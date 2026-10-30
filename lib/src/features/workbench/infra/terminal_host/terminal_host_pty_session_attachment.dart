part of 'terminal_host_pty_session.dart';

/// Same-session attach and output cursor continuity; never replace a lost PTY.
extension _TerminalHostPtySessionAttachment on TerminalHostPtySession {
  Future<TerminalHostAttachment> _createOrAttach() {
    final launch = _launch;
    final workingDirectory = _workingDirectory;
    final cols = _cols;
    final rows = _rows;
    if (launch == null ||
        workingDirectory == null ||
        cols == null ||
        rows == null) {
      throw StateError('PTY session has not been started.');
    }
    if (_client case final TerminalHostViewLeaseClient viewClient) {
      return viewClient.createOrAttachView(
        sessionId: _sessionId,
        workspaceId: _workspaceId,
        tabId: _tabId,
        workingDirectory: workingDirectory,
        launch: launch,
        cols: cols,
        rows: rows,
        viewLeaseId: _viewLeaseId,
        resumeCursor: _retainedOutputCursor,
      );
    }
    return _client.createOrAttach(
      sessionId: _sessionId,
      workspaceId: _workspaceId,
      tabId: _tabId,
      workingDirectory: workingDirectory,
      launch: launch,
      cols: cols,
      rows: rows,
      resumeCursor: _retainedOutputCursor,
    );
  }

  Future<void> _applyAttachment(TerminalHostAttachment attachment) async {
    _startedNewProcess = attachment.created;
    if (_retainedOutputCursor != null) {
      // Only a cursor captured while output is parked may survive a
      // reattach. A normal full attachment keeps receiving live output, so
      // its response cursor would immediately become stale as parser state
      // advances.
      _retainedOutputCursor = attachment.outputCursor;
    }
    if (attachment.hasSnapshot || attachment.created) {
      final resetInteractionModes = attachment.created || !attachment.running;
      final snapshotText = attachment.snapshotText;
      _events.add(
        snapshotText != null
            ? TerminalPtySnapshotTextEvent(
                snapshotText,
                resetInteractionModes: resetInteractionModes,
              )
            : TerminalPtySnapshotEvent(
                attachment.snapshot,
                resetInteractionModes: resetInteractionModes,
              ),
      );
    }
    if (!attachment.running) {
      final exitCode = attachment.exitCode;
      if (exitCode != null) {
        _events.add(TerminalPtyExitEvent(exitCode, notifyRuntime: false));
      }
    }
    if (attachment.created) {
      await _onProcessCreated?.call();
    }
  }

  Future<TerminalHostAttachment> _reattach() {
    if (_disposed) {
      throw StateError('PTY session is disposed.');
    }
    final existing = _reattachFuture;
    if (existing != null) {
      return existing;
    }
    late final Future<TerminalHostAttachment> next;
    next = _createOrAttach()
        .then((attachment) async {
          await _applyAttachment(attachment);
          if (!attachment.created) {
            _transportRecoveryFailures = 0;
          }
          return attachment;
        })
        .whenComplete(() {
          if (identical(_reattachFuture, next)) {
            _reattachFuture = null;
          }
        });
    _reattachFuture = next;
    return next;
  }
}
