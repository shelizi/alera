part of 'workspace_editor_surface.dart';

extension _WorkspaceEditorSave on _WorkspaceEditorSurfaceState {
  Future<void> _save() async {
    final filePath = widget.tab.filePath;
    if (filePath == null || _loading || _saving) {
      return;
    }
    final loadError = _loadError;
    if (loadError != null) {
      _showToast(_messageFor(loadError), tone: .error);
      return;
    }
    if (!_document.canSave || !mounted) {
      return;
    }
    final contentBeingSaved = _controller.text;
    final documentVersionBeingSaved = _controller.documentVersion;
    _setEditorState(() => _saving = true);
    try {
      final saved = await _write(
        overwriteIfChanged: false,
        currentDisplayContent: contentBeingSaved,
      );
      if (!mounted) {
        return;
      }
      _acceptSavedIfUnchanged(
        saved,
        contentBeingSaved,
        documentVersionBeingSaved,
      );
      _acceptLanguageIntelligenceSaved(saved);
      await _notifyLanguageIntelligenceSaved(
        saved: saved,
        savedDocumentVersion: documentVersionBeingSaved,
      );
      _autosave.resume();
      _showToast('File saved');
    } catch (error) {
      if (!mounted) {
        return;
      }
      if (error is native.WorkspaceFileError &&
          error.kind == native.WorkspaceFileErrorKind.conflict) {
        _autosave.pause();
        await _resolveSaveConflict();
      } else {
        _showToast(_messageFor(error), tone: .error);
      }
    } finally {
      if (mounted) {
        _setEditorState(() => _saving = false);
      }
      _autosave.notifyStateChanged();
    }
  }

  Future<void> _saveAutomatically() async {
    final filePath = widget.tab.filePath;
    if (!mounted ||
        filePath == null ||
        _loading ||
        _saving ||
        _loadError != null ||
        !_document.canSave) {
      return;
    }
    final contentBeingSaved = _controller.text;
    final documentVersionBeingSaved = _controller.documentVersion;
    _setEditorState(() => _saving = true);
    try {
      final saved = await _write(
        overwriteIfChanged: false,
        currentDisplayContent: contentBeingSaved,
      );
      if (!mounted) {
        return;
      }
      _acceptSavedIfUnchanged(
        saved,
        contentBeingSaved,
        documentVersionBeingSaved,
      );
      _acceptLanguageIntelligenceSaved(saved);
      await _notifyLanguageIntelligenceSaved(
        saved: saved,
        savedDocumentVersion: documentVersionBeingSaved,
      );
    } finally {
      if (mounted) {
        _setEditorState(() => _saving = false);
      }
    }
  }

  Future<void> _discardChanges() async {
    if (_loading || _saving || !_document.canSave) {
      return;
    }
    if (_document.nativeBacked) {
      _clearPendingDocumentSnapshot();
      _undoController.clear();
      await _load();
      if (mounted) {
        _showToast('Changes discarded');
      }
      return;
    }
    final loadedText = _document.loadedText;
    if (loadedText == null) {
      return;
    }
    _clearPendingDocumentSnapshot();
    _document.updateCurrentText(loadedText);
    _replaceControllerText(loadedText);
    _undoController.clear();
    unawaited(_refreshLanguageIntelligenceDocument());
    if (mounted) {
      _setEditorState(() {});
      _showToast('Changes discarded');
    }
    _autosave.notifyStateChanged();
  }

  Future<bool> _resolveSaveConflict() async {
    final overwrite = await showDialog<bool>(
      context: context,
      builder: (context) => const AleraConfirmDialog(
        title: 'File changed on disk',
        message: 'Overwrite the file with the editor contents?',
        confirmLabel: 'Overwrite',
        destructive: true,
      ),
    );
    if (overwrite != true || !mounted) {
      return false;
    }
    final contentBeingSaved = _controller.text;
    final documentVersionBeingSaved = _controller.documentVersion;
    try {
      final saved = await _write(
        overwriteIfChanged: true,
        currentDisplayContent: contentBeingSaved,
      );
      if (!mounted) {
        return false;
      }
      _acceptSavedIfUnchanged(
        saved,
        contentBeingSaved,
        documentVersionBeingSaved,
      );
      _acceptLanguageIntelligenceSaved(saved);
      await _notifyLanguageIntelligenceSaved(
        saved: saved,
        savedDocumentVersion: documentVersionBeingSaved,
      );
      _autosave.resume();
      _showToast('File overwritten');
      return true;
    } catch (error) {
      if (mounted) {
        _showToast(_messageFor(error), tone: .error);
      }
      return false;
    }
  }

  Future<native.WorkspaceEditorTextFile> _write({
    required bool overwriteIfChanged,
    required String currentDisplayContent,
  }) {
    return _workspaceFiles.writeEditorTextFile(
      workspacePath: widget.workspace.path,
      relativePath: widget.tab.filePath!,
      currentDisplayContent: currentDisplayContent,
      originalRawContent: _document.loadedRawText,
      originalDisplayContent: _document.loadedText,
      expectedContentToken: _document.contentToken,
      overwriteIfChanged: overwriteIfChanged,
      tabSize: _currentEditorTabSize(),
      encoding: _document.encoding!,
    );
  }

  void _acceptSaved(native.WorkspaceEditorTextFile saved) {
    _clearPendingDocumentSnapshot();
    if (_document.nativeBacked) {
      final documentVersion = _controller.documentVersion;
      _document.acceptNativeSaved(
        encoding: saved.encoding,
        contentToken: saved.contentToken,
        savedDocumentVersion: documentVersion,
        currentDocumentVersion: documentVersion,
        tabSize: _currentEditorTabSize(),
      );
      return;
    }
    _document.acceptSaved(saved, tabSize: _currentEditorTabSize());
    _replaceControllerText(_document.currentText ?? '');
  }

  void _acceptSavedIfUnchanged(
    native.WorkspaceEditorTextFile saved,
    String contentBeingSaved,
    int documentVersionBeingSaved,
  ) {
    if (_document.nativeBacked) {
      _clearPendingDocumentSnapshot();
      _document.acceptNativeSaved(
        encoding: saved.encoding,
        contentToken: saved.contentToken,
        savedDocumentVersion: documentVersionBeingSaved,
        currentDocumentVersion: _controller.documentVersion,
        tabSize: _currentEditorTabSize(),
      );
      return;
    }
    if (_controller.text == contentBeingSaved) {
      _acceptSaved(saved);
      return;
    }
    final currentText = _controller.text;
    _clearPendingDocumentSnapshot();
    _document.acceptSaved(saved, tabSize: _currentEditorTabSize());
    _document.updateCurrentText(currentText);
  }

  bool _isReadyForAutosave() {
    return mounted &&
        !_loading &&
        !_saving &&
        _loadError == null &&
        _document.canSave;
  }

  void _handleControllerChanged() {
    if (_suppressControllerChangeHandling) {
      return;
    }
    final documentVersion = _controller.documentVersion;
    if (!workspaceEditorShouldSyncControllerText(
      previousDocumentVersion: _lastObservedDocumentVersion,
      currentDocumentVersion: documentVersion,
    )) {
      return;
    }
    _lastObservedDocumentVersion = documentVersion;
    _scheduleOutlineRefresh();
    final wasDirty = _isDirty();
    final lineCount = _controller.lineCount;
    final contentLength = _controller.length;
    final previousProfile =
        _lastPerformanceProfile ??
        workspaceEditorPerformanceProfile(
          lineCount: lineCount,
          contentLength: contentLength,
        );
    final deferSnapshot = workspaceEditorShouldDeferDocumentSnapshot(
      lineCount: lineCount,
      contentLength: contentLength,
    );
    if (_document.nativeBacked) {
      _clearPendingDocumentSnapshot();
      if (!_document.updateNativeDocumentVersion(documentVersion)) {
        return;
      }
    } else if (deferSnapshot) {
      _scheduleDocumentSnapshot();
    } else {
      _clearPendingDocumentSnapshot();
      if (!_document.updateCurrentText(_controller.text)) {
        return;
      }
    }
    final isDirty = _isDirty();
    if (!_loading && widget.tab.isPreview && !wasDirty && isDirty) {
      widget.onKeepPreview?.call();
    }
    _autosave.notifyTextChanged();
    _scheduleLanguageIntelligenceDocumentSync();
    final currentProfile = workspaceEditorPerformanceProfile(
      lineCount: lineCount,
      contentLength: contentLength,
    );
    _lastPerformanceProfile = currentProfile;
    if (workspaceEditorShouldRefreshSurface(
      wasDirty: wasDirty,
      isDirty: isDirty,
      previousProfile: previousProfile,
      currentProfile: currentProfile,
    )) {
      _refreshStateSafely();
    }
  }

  void _scheduleDocumentSnapshot() {
    _hasPendingDocumentSnapshot = true;
    _documentSnapshotDebounceTimer?.cancel();
    _documentSnapshotDebounceTimer = Timer(
      workspaceEditorLargeFileSnapshotDebounce,
      _flushPendingDocumentSnapshot,
    );
  }

  void _clearPendingDocumentSnapshot() {
    _documentSnapshotDebounceTimer?.cancel();
    _documentSnapshotDebounceTimer = null;
    _hasPendingDocumentSnapshot = false;
  }

  void _flushPendingDocumentSnapshot({
    bool refreshState = true,
    bool notifyAutosave = true,
  }) {
    if (!_hasPendingDocumentSnapshot) {
      _documentSnapshotDebounceTimer?.cancel();
      _documentSnapshotDebounceTimer = null;
      return;
    }
    final wasDirty = _isDirty();
    _documentSnapshotDebounceTimer?.cancel();
    _documentSnapshotDebounceTimer = null;
    _hasPendingDocumentSnapshot = false;
    if (_document.nativeBacked) {
      return;
    }
    _document.updateCurrentText(_controller.text);
    final isDirty = _isDirty();
    if (refreshState && mounted && wasDirty != isDirty) {
      _refreshStateSafely();
    }
    if (notifyAutosave) {
      _autosave.notifyStateChanged();
    }
  }

  void _handleAutosaveError(Object error, StackTrace stackTrace) {
    if (!mounted) {
      return;
    }
    if (error is native.WorkspaceFileError &&
        error.kind == native.WorkspaceFileErrorKind.conflict) {
      _showToast('Autosave paused because the file changed on disk.');
      unawaited(_resolveSaveConflict());
      return;
    }
    _showToast('Autosave paused: ${_messageFor(error)}', tone: .error);
  }
}
