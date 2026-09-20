part of 'workspace_editor_surface.dart';

const Duration _workspaceEditorLanguageIntelligenceSyncDebounce = Duration(
  milliseconds: 100,
);

extension _WorkspaceEditorLanguageIntelligence on _WorkspaceEditorSurfaceState {
  bool _semanticEnabledFor({
    required String filePath,
    required LanguageIntelligenceSettings settings,
  }) {
    final language = ref
        .read(languageExtensionRegistryProvider)
        .languageForPath(filePath);
    if (language == null) {
      return false;
    }
    return settings.forLanguage(language.id).enabled;
  }

  String _languageIntelligenceAbsolutePath(
    Workspace workspace,
    String filePath,
  ) {
    if (p.isAbsolute(filePath)) {
      return p.normalize(filePath);
    }
    return p.normalize(p.join(workspace.path, filePath));
  }

  LanguageServerTarget _languageServerTarget(Workspace workspace) {
    final hostId = workspace.hostId.trim();
    return hostId.isEmpty || hostId == 'local'
        ? LanguageServerTarget.localWorkspace
        : LanguageServerTarget.remoteWorkspace;
  }

  Future<void> _refreshLanguageIntelligenceDocument({
    LanguageIntelligenceSettings? settings,
  }) async {
    final filePath = widget.tab.filePath;
    if (!mounted || filePath == null || _loading || _loadError != null) {
      return;
    }
    final workspace = widget.workspace;
    final effectiveSettings =
        settings ??
        ref.read(settingsControllerProvider).editor.languageIntelligence;
    final absolutePath = _languageIntelligenceAbsolutePath(workspace, filePath);
    final target = _languageServerTarget(workspace);

    if (!_semanticEnabledFor(filePath: filePath, settings: effectiveSettings)) {
      if (_languageIntelligenceDocumentPath != null) {
        try {
          await _languageIntelligence.openDocument(
            workspaceId: workspace.id,
            workspaceRoot: workspace.path,
            path: absolutePath,
            text: '',
            settings: effectiveSettings,
            target: target,
          );
        } catch (_) {
          // Semantic providers are optional and must not destabilize the editor.
        }
      }
      _languageIntelligenceWorkspaceId = null;
      _languageIntelligenceDocumentPath = null;
      _resetLanguageIntelligenceSource();
      return;
    }

    // The current CodeForge runtime is local-only. Do not materialize or read a
    // remote workspace through local filesystem APIs while the remote adapter
    // is intentionally not implemented yet.
    if (target == LanguageServerTarget.remoteWorkspace) {
      return;
    }

    final String text;
    try {
      text = await _languageIntelligenceSourceText(filePath);
    } catch (_) {
      return;
    }
    if (!mounted ||
        widget.workspace.id != workspace.id ||
        widget.tab.filePath != filePath) {
      return;
    }
    try {
      await _languageIntelligence.openDocument(
        workspaceId: workspace.id,
        workspaceRoot: workspace.path,
        path: absolutePath,
        text: text,
        settings: effectiveSettings,
        target: target,
      );
    } catch (_) {
      return;
    }
    _languageIntelligenceWorkspaceId = workspace.id;
    _languageIntelligenceDocumentPath = absolutePath;
  }

  void _scheduleLanguageIntelligenceDocumentSync() {
    final filePath = widget.tab.filePath;
    if (filePath == null ||
        _loading ||
        _loadError != null ||
        !_semanticEnabledFor(
          filePath: filePath,
          settings: ref
              .read(settingsControllerProvider)
              .editor
              .languageIntelligence,
        )) {
      return;
    }
    _languageIntelligenceSyncTimer?.cancel();
    _languageIntelligenceSyncTimer = Timer(
      _workspaceEditorLanguageIntelligenceSyncDebounce,
      () => unawaited(_flushLanguageIntelligenceDocumentSync()),
    );
  }

  Future<void> _flushLanguageIntelligenceDocumentSync() async {
    _languageIntelligenceSyncTimer?.cancel();
    _languageIntelligenceSyncTimer = null;
    final filePath = widget.tab.filePath;
    if (!mounted || filePath == null || _loading || _loadError != null) {
      return;
    }
    final settings = ref
        .read(settingsControllerProvider)
        .editor
        .languageIntelligence;
    if (!_semanticEnabledFor(filePath: filePath, settings: settings)) {
      return;
    }
    if (_languageServerTarget(widget.workspace) ==
        LanguageServerTarget.remoteWorkspace) {
      return;
    }

    final generation = ++_languageIntelligenceSyncGeneration;
    final absolutePath = _languageIntelligenceAbsolutePath(
      widget.workspace,
      filePath,
    );
    if (_languageIntelligenceDocumentPath != absolutePath) {
      await _refreshLanguageIntelligenceDocument(settings: settings);
      return;
    }
    final String text;
    try {
      text = await _languageIntelligenceSourceText(filePath);
    } catch (_) {
      return;
    }
    if (!mounted ||
        generation != _languageIntelligenceSyncGeneration ||
        widget.tab.filePath != filePath) {
      return;
    }
    try {
      await _languageIntelligence.replaceDocument(
        workspaceId: widget.workspace.id,
        path: absolutePath,
        text: text,
      );
    } catch (_) {
      // Provider failures remain isolated from editor state.
    }
  }

  Future<String> _languageIntelligenceSourceText(String filePath) async {
    var source = _languageSemanticSource;
    final absolutePath = _languageIntelligenceAbsolutePath(
      widget.workspace,
      filePath,
    );
    if (source == null || source.absolutePath != absolutePath) {
      if (!_document.nativeBacked &&
          _document.loadedRawText != null &&
          _document.loadedText != null) {
        source = _WorkspaceEditorSemanticSourceSnapshot(
          absolutePath: absolutePath,
          originalRawText: _document.loadedRawText!,
          originalDisplayText: _document.loadedText!,
        );
      } else {
        final loaded = await _workspaceFiles.readEditorTextFile(
          workspacePath: widget.workspace.path,
          relativePath: filePath,
          tabSize: _currentEditorTabSize(),
          encoding: _document.encoding,
        );
        source = _WorkspaceEditorSemanticSourceSnapshot(
          absolutePath: absolutePath,
          originalRawText: loaded.rawContent,
          originalDisplayText: loaded.displayContent,
        );
      }
      _languageSemanticSource = source;
    }
    return workspaceEditorEncodeSemanticSourceText(
      currentDisplayText: _controller.text,
      originalRawText: source.originalRawText,
      originalDisplayText: source.originalDisplayText,
    );
  }

  void _acceptLanguageIntelligenceSaved(native.WorkspaceEditorTextFile saved) {
    final filePath = widget.tab.filePath;
    if (filePath == null || _languageIntelligenceDocumentPath == null) {
      return;
    }
    _languageSemanticSource = _WorkspaceEditorSemanticSourceSnapshot(
      absolutePath: _languageIntelligenceAbsolutePath(
        widget.workspace,
        filePath,
      ),
      originalRawText: saved.rawContent,
      originalDisplayText: saved.displayContent,
    );
  }

  Future<void> _notifyLanguageIntelligenceSaved({
    required native.WorkspaceEditorTextFile saved,
    required int savedDocumentVersion,
  }) async {
    final path = _languageIntelligenceDocumentPath;
    final workspaceId = _languageIntelligenceWorkspaceId;
    if (path == null || workspaceId == null) {
      return;
    }
    try {
      await _languageIntelligence.replaceDocument(
        workspaceId: workspaceId,
        path: path,
        text: saved.rawContent,
      );
      await _languageIntelligence.saveDocument(
        workspaceId: workspaceId,
        path: path,
      );
    } catch (_) {
      return;
    }
    if (_controller.documentVersion != savedDocumentVersion) {
      _scheduleLanguageIntelligenceDocumentSync();
    }
  }

  Future<void> _closeLanguageIntelligenceDocument() async {
    _languageIntelligenceSyncTimer?.cancel();
    _languageIntelligenceSyncTimer = null;
    _languageIntelligenceSyncGeneration += 1;
    final path = _languageIntelligenceDocumentPath;
    final workspaceId = _languageIntelligenceWorkspaceId;
    _languageIntelligenceWorkspaceId = null;
    _languageIntelligenceDocumentPath = null;
    if (path == null || workspaceId == null) {
      return;
    }
    try {
      await _languageIntelligence.closeDocument(
        workspaceId: workspaceId,
        path: path,
      );
    } catch (_) {
      // Teardown of an optional provider must not affect editor disposal.
    }
  }

  void _resetLanguageIntelligenceSource() {
    _languageIntelligenceSyncTimer?.cancel();
    _languageIntelligenceSyncTimer = null;
    _languageIntelligenceSyncGeneration += 1;
    _languageSemanticSource = null;
  }
}

class const _WorkspaceEditorSemanticSourceSnapshot({
  required final String absolutePath,
  required final String originalRawText,
  required final String originalDisplayText,
});

@visibleForTesting
String workspaceEditorEncodeSemanticSourceText({
  required String currentDisplayText,
  required String originalRawText,
  required String originalDisplayText,
}) {
  if (currentDisplayText == originalDisplayText) {
    return originalRawText;
  }
  final rawSegments = _workspaceEditorSplitLineSegments(originalRawText);
  final displaySegments = _workspaceEditorSplitLineSegments(
    originalDisplayText,
  );
  final currentSegments = _workspaceEditorSplitLineSegments(currentDisplayText);
  if (rawSegments.length != displaySegments.length) {
    return currentDisplayText;
  }

  var prefix = 0;
  while (prefix < displaySegments.length &&
      prefix < currentSegments.length &&
      displaySegments[prefix] == currentSegments[prefix]) {
    prefix += 1;
  }

  var suffix = 0;
  while (suffix < displaySegments.length - prefix &&
      suffix < currentSegments.length - prefix) {
    final originalIndex = displaySegments.length - suffix - 1;
    final currentIndex = currentSegments.length - suffix - 1;
    if (displaySegments[originalIndex] != currentSegments[currentIndex]) {
      break;
    }
    suffix += 1;
  }

  final encoded = StringBuffer();
  final suffixStart = currentSegments.length - suffix;
  for (var index = 0; index < currentSegments.length; index += 1) {
    if (index < prefix) {
      encoded.write(rawSegments[index]);
    } else if (index >= suffixStart) {
      final originalIndex =
          rawSegments.length - (currentSegments.length - index);
      encoded.write(rawSegments[originalIndex]);
    } else {
      encoded.write(currentSegments[index]);
    }
  }
  return encoded.toString();
}

List<String> _workspaceEditorSplitLineSegments(String text) {
  final segments = <String>[];
  var start = 0;
  for (var index = 0; index < text.length; index += 1) {
    final codeUnit = text.codeUnitAt(index);
    if (codeUnit == 0x0D) {
      if (index + 1 < text.length && text.codeUnitAt(index + 1) == 0x0A) {
        index += 1;
      }
      segments.add(text.substring(start, index + 1));
      start = index + 1;
    } else if (codeUnit == 0x0A) {
      segments.add(text.substring(start, index + 1));
      start = index + 1;
    }
  }
  if (start < text.length) {
    segments.add(text.substring(start));
  }
  return segments;
}
