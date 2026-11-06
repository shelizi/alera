part of 'workspace_editor_surface.dart';

extension _WorkspaceEditorLoading on _WorkspaceEditorSurfaceState {
  Future<void> _load({
    _WorkspaceEditorScrollPosition? restoreScrollPosition,
  }) async {
    _clearSelectedOccurrences();
    _autosave.cancelPending();
    _clearPendingDocumentSnapshot();
    _resetLanguageIntelligenceSource();
    final requestId = ++_loadRequestId;
    final workspacePath = _editorWorkspacePath;
    final filePath = _editorRelativePath;
    if (filePath == null) {
      unawaited(_closeLanguageIntelligenceDocument());
      _resetLanguageIntelligenceSource();
      if (mounted) {
        _setEditorState(() {
          _loading = false;
          _loadError = null;
        });
      }
      return;
    }
    _setEditorState(() {
      _loading = true;
      _loadError = null;
    });
    try {
      final tabSize = _currentEditorTabSize();
      final info = await _openControllerFromWorkspace(
        workspacePath: workspacePath,
        relativePath: filePath,
        tabSize: tabSize,
        encoding: _document.requestedEncoding,
      );
      if (!_isCurrentLoadRequest(requestId, workspacePath, filePath)) {
        return;
      }
      _document.acceptNativeLoaded(
        encoding: _fromCodeForgeEncoding(info.encoding),
        contentToken: info.contentToken,
        documentVersion: _controller.documentVersion,
        tabSize: tabSize,
        requestedEncoding: _document.requestedEncoding,
      );
    } catch (error) {
      if (!_isCurrentLoadRequest(requestId, workspacePath, filePath)) {
        return;
      }
      _document.acceptLoadError(error);
      _loadError = error;
    } finally {
      if (_isCurrentLoadRequest(requestId, workspacePath, filePath)) {
        _setEditorState(() => _loading = false);
        if (restoreScrollPosition != null) {
          _scheduleEditorScrollRestore(
            restoreScrollPosition,
            requestId: requestId,
          );
        }
        _applyPendingReveal();
        _autosave.notifyStateChanged();
        _scheduleOutlineRefresh(immediate: true);
        if (_loadError == null) {
          unawaited(_refreshLanguageIntelligenceDocument());
        }
      }
    }
  }

  Future<void> _changeEncoding(WorkspaceTextEncodingSelection selection) async {
    final requested = selection.encoding;
    if (_document.requestedEncoding == requested && _document.hasSnapshot) {
      return;
    }
    if (_isDirty()) {
      final discard = await showDialog<bool>(
        context: context,
        builder: (context) => const AleraConfirmDialog(
          title: 'Reopen File With Encoding?',
          message:
              'Unsaved changes will be discarded before reopening the file.',
          confirmLabel: 'Reopen',
          destructive: true,
        ),
      );
      if (discard != true || !mounted) {
        return;
      }
    }
    _document.requestedEncoding = requested;
    _undoController.clear();
    await _load();
  }

  void _restoreDocumentOrLoad() {
    final filePath = _editorRelativePath;
    if (widget.tab.isUntitled) {
      if (_document.hasSnapshot) {
        _replaceControllerText(_document.currentText ?? '');
        _loading = false;
        _loadError = null;
      } else {
        unawaited(_loadUntitledDraft(widget.tab.id));
      }
      return;
    }
    if (filePath == null) {
      _invalidatePendingLoads();
      _autosave.cancelPending();
      _loading = false;
      _loadError = null;
      return;
    }
    _document.attachFile(
      workspacePath: _editorWorkspacePath,
      relativePath: filePath,
    );
    if (_document.hasSnapshot &&
        (!_document.nativeBacked || _document.currentText != null)) {
      _invalidatePendingLoads();
      _replaceControllerText(_document.currentText ?? '');
      _loadError = _document.loadError;
      _loading = false;
      final remembered = _rememberedEditorScrollPosition();
      if (remembered != null) {
        _scheduleEditorScrollRestore(remembered, requestId: _loadRequestId);
      }
      _applyPendingReveal();
      _autosave.notifyStateChanged();
      _scheduleOutlineRefresh(immediate: true);
      unawaited(_refreshLanguageIntelligenceDocument());
      return;
    }
    _loading = true;
    _loadError = null;
    // Native-backed documents keep no text snapshot and reload on return.
    unawaited(_load(restoreScrollPosition: _rememberedEditorScrollPosition()));
  }

  Future<void> _loadUntitledDraft(String tabId) async {
    final requestId = ++_loadRequestId;
    _loading = true;
    _loadError = null;
    try {
      final text = await _drafts.read(tabId) ?? '';
      if (!mounted || requestId != _loadRequestId || widget.tab.id != tabId) {
        return;
      }
      _document.acceptUntitled(text);
      _replaceControllerText(text);
      _setEditorState(() => _loading = false);
      _scheduleOutlineRefresh(immediate: true);
    } catch (error) {
      if (!mounted || requestId != _loadRequestId || widget.tab.id != tabId) {
        return;
      }
      _setEditorState(() {
        _loadError = error;
        _loading = false;
      });
    }
  }

  bool _isCurrentLoadRequest(
    int requestId,
    String workspacePath,
    String filePath,
  ) {
    return mounted &&
        workspaceEditorLoadRequestMatches(
          requestId: requestId,
          currentRequestId: _loadRequestId,
          workspacePath: workspacePath,
          activeWorkspacePath: _editorWorkspacePath,
          filePath: filePath,
          activeFilePath: _editorRelativePath,
        );
  }

  void _invalidatePendingLoads() {
    _loadRequestId += 1;
  }
}

code_forge.NativeWorkspaceTextEncoding? _toCodeForgeEncoding(
  native.WorkspaceTextEncoding? encoding,
) {
  if (encoding == null) return null;
  return switch (encoding) {
    native.WorkspaceTextEncoding.utf8 =>
      code_forge.NativeWorkspaceTextEncoding.utf8,
    native.WorkspaceTextEncoding.utf8Bom =>
      code_forge.NativeWorkspaceTextEncoding.utf8Bom,
    native.WorkspaceTextEncoding.utf16Le =>
      code_forge.NativeWorkspaceTextEncoding.utf16Le,
    native.WorkspaceTextEncoding.utf16Be =>
      code_forge.NativeWorkspaceTextEncoding.utf16Be,
    native.WorkspaceTextEncoding.big5 =>
      code_forge.NativeWorkspaceTextEncoding.big5,
    native.WorkspaceTextEncoding.gbk =>
      code_forge.NativeWorkspaceTextEncoding.gbk,
    native.WorkspaceTextEncoding.shiftJis =>
      code_forge.NativeWorkspaceTextEncoding.shiftJis,
    native.WorkspaceTextEncoding.eucJp =>
      code_forge.NativeWorkspaceTextEncoding.eucJp,
    native.WorkspaceTextEncoding.eucKr =>
      code_forge.NativeWorkspaceTextEncoding.eucKr,
    native.WorkspaceTextEncoding.windows1252 =>
      code_forge.NativeWorkspaceTextEncoding.windows1252,
  };
}

native.WorkspaceTextEncoding _fromCodeForgeEncoding(
  code_forge.NativeWorkspaceTextEncoding encoding,
) {
  return switch (encoding) {
    code_forge.NativeWorkspaceTextEncoding.utf8 =>
      native.WorkspaceTextEncoding.utf8,
    code_forge.NativeWorkspaceTextEncoding.utf8Bom =>
      native.WorkspaceTextEncoding.utf8Bom,
    code_forge.NativeWorkspaceTextEncoding.utf16Le =>
      native.WorkspaceTextEncoding.utf16Le,
    code_forge.NativeWorkspaceTextEncoding.utf16Be =>
      native.WorkspaceTextEncoding.utf16Be,
    code_forge.NativeWorkspaceTextEncoding.big5 =>
      native.WorkspaceTextEncoding.big5,
    code_forge.NativeWorkspaceTextEncoding.gbk =>
      native.WorkspaceTextEncoding.gbk,
    code_forge.NativeWorkspaceTextEncoding.shiftJis =>
      native.WorkspaceTextEncoding.shiftJis,
    code_forge.NativeWorkspaceTextEncoding.eucJp =>
      native.WorkspaceTextEncoding.eucJp,
    code_forge.NativeWorkspaceTextEncoding.eucKr =>
      native.WorkspaceTextEncoding.eucKr,
    code_forge.NativeWorkspaceTextEncoding.windows1252 =>
      native.WorkspaceTextEncoding.windows1252,
  };
}

@visibleForTesting
bool workspaceEditorLoadRequestMatches({
  required int requestId,
  required int currentRequestId,
  required String workspacePath,
  required String activeWorkspacePath,
  required String filePath,
  required String? activeFilePath,
}) {
  return requestId == currentRequestId &&
      workspacePath == activeWorkspacePath &&
      filePath == activeFilePath;
}
