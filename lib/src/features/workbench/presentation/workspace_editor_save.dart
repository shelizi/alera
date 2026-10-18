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
    final useNative = _canSaveNative;
    final benchmark = EditorSavePerfProbe.start(
      native: useNative,
      length: _controller.length,
      source: 'manual',
    );
    var completed = false;
    try {
      if (useNative) _controller.flushPendingBuffer();
      final contentBeingSaved = useNative ? '' : _controller.text;
      final documentVersionBeingSaved = _controller.documentVersion;
      benchmark?.markPreparationComplete();
      _setEditorState(() => _saving = true);
      if (useNative) {
        await _writeNative(
          overwriteIfChanged: false,
          savedDocumentVersion: documentVersionBeingSaved,
        );
        benchmark?.markWriteComplete();
        if (!mounted) return;
        _autosave.resume();
        _showToast('File saved');
        completed = true;
        return;
      }
      final saved = await _write(
        overwriteIfChanged: false,
        currentDisplayContent: contentBeingSaved,
      );
      benchmark?.markWriteComplete();
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
      completed = true;
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
      benchmark?.finish(success: completed);
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
    final useNative = _canSaveNative;
    if (useNative) _controller.flushPendingBuffer();
    final contentBeingSaved = useNative ? '' : _controller.text;
    final documentVersionBeingSaved = _controller.documentVersion;
    _setEditorState(() => _saving = true);
    try {
      if (useNative) {
        await _writeNative(
          overwriteIfChanged: false,
          savedDocumentVersion: documentVersionBeingSaved,
        );
        return;
      }
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
    final useNative = _canSaveNative;
    if (useNative) _controller.flushPendingBuffer();
    final contentBeingSaved = useNative ? '' : _controller.text;
    final documentVersionBeingSaved = _controller.documentVersion;
    try {
      if (useNative) {
        await _writeNative(
          overwriteIfChanged: true,
          savedDocumentVersion: documentVersionBeingSaved,
        );
        if (!mounted) return false;
        _autosave.resume();
        _showToast('File overwritten');
        return true;
      }
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

  bool get _canSaveNative => workspaceEditorShouldUseNativeSave(
    nativeSaveEnabled:
        const bool.fromEnvironment('ALERA_NATIVE_ROPE_SAVE') &&
        !editorSaveBenchmarkForceLegacy,
    nativeBacked: _document.nativeBacked,
    languageServiceDocumentOpen: _languageIntelligenceDocumentPath != null,
  );

  Future<void> _writeNative({
    required bool overwriteIfChanged,
    required int savedDocumentVersion,
  }) async {
    final encoding = _document.encoding!;
    final token = _document.contentToken;
    if (token == null) {
      throw const native.WorkspaceFileError(
        kind: native.WorkspaceFileErrorKind.conflict,
        context: 'missing saved file token',
      );
    }
    final outcome = await _controller.saveWorkspaceSnapshot(
      workspacePath: widget.workspace.path,
      relativePath: widget.tab.filePath!,
      expectedContentToken: token,
      overwriteIfChanged: overwriteIfChanged,
      tabSize: _currentEditorTabSize(),
      encoding: _toCodeForgeEncoding(encoding)!,
    );
    if (outcome.status != code_forge.SaveStatus.saved) {
      final kind = switch (outcome.status) {
        code_forge.SaveStatus.conflict =>
          native.WorkspaceFileErrorKind.conflict,
        code_forge.SaveStatus.invalidPath =>
          native.WorkspaceFileErrorKind.invalidPath,
        code_forge.SaveStatus.protected =>
          native.WorkspaceFileErrorKind.protectedPath,
        code_forge.SaveStatus.unsupported =>
          native.WorkspaceFileErrorKind.unsupported,
        code_forge.SaveStatus.io => native.WorkspaceFileErrorKind.io,
        code_forge.SaveStatus.saved => native.WorkspaceFileErrorKind.io,
      };
      throw native.WorkspaceFileError(
        kind: kind,
        context: 'Native workspace save failed: ${outcome.status.name}',
      );
    }
    if (!mounted) return;
    _clearPendingDocumentSnapshot();
    _document.acceptNativeSaved(
      encoding: encoding,
      contentToken: outcome.token,
      savedDocumentVersion: savedDocumentVersion,
      currentDocumentVersion: _controller.documentVersion,
      tabSize: _currentEditorTabSize(),
    );
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
    _scheduleSelectedOccurrencesForSelection();
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

@visibleForTesting
bool workspaceEditorShouldUseNativeSave({
  required bool nativeSaveEnabled,
  required bool nativeBacked,
  required bool languageServiceDocumentOpen,
}) => nativeSaveEnabled && nativeBacked && !languageServiceDocumentOpen;
