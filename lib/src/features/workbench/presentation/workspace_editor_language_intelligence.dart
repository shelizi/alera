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

  bool _canOfferLanguageNavigation(LanguageCapability capability) {
    final filePath = widget.tab.filePath;
    if (filePath == null ||
        _loading ||
        _loadError != null ||
        _languageServerTarget(widget.workspace) !=
            LanguageServerTarget.localWorkspace) {
      return false;
    }
    final registry = ref.read(languageExtensionRegistryProvider);
    final language = registry.languageForPath(filePath);
    if (language == null) {
      return false;
    }
    final activation = ref
        .read(settingsControllerProvider)
        .editor
        .languageIntelligence
        .forLanguage(language.id);
    if (!activation.enabled) {
      return false;
    }
    final provider = registry.semanticProviderFor(
      language.id,
      preferredProviderId: activation.semanticProviderId,
    );
    return provider?.capabilities.contains(capability) ?? false;
  }

  void _showLanguageNavigationUnavailable(LanguageCapability capability) {
    final filePath = widget.tab.filePath;
    if (filePath == null) return;
    if (_languageServerTarget(widget.workspace) !=
        LanguageServerTarget.localWorkspace) {
      _showToast('Language servers are not available for remote workspaces.');
      return;
    }
    final registry = ref.read(languageExtensionRegistryProvider);
    final language = registry.languageForPath(filePath);
    if (language == null) {
      _showToast('No language intelligence is registered for this file.');
      return;
    }
    final activation = ref
        .read(settingsControllerProvider)
        .editor
        .languageIntelligence
        .forLanguage(language.id);
    if (!activation.enabled) {
      _showToast(
        '${language.displayName} semantic navigation is disabled. '
        'Enable it in Settings > Language Intelligence.',
      );
      return;
    }
    final provider = registry.semanticProviderFor(
      language.id,
      preferredProviderId: activation.semanticProviderId,
    );
    if (provider == null) {
      _showToast(
        'No semantic language server is configured for this language.',
      );
      return;
    }
    if (!provider.capabilities.contains(capability)) {
      _showLanguageNavigationUnsupported(capability);
      return;
    }
    _showToast('Language server is not ready');
  }

  void _showLanguageNavigationUnsupported(LanguageCapability capability) {
    _showToast(switch (capability) {
      LanguageCapability.definition =>
        'The selected language server does not support Go to Definition.',
      LanguageCapability.implementation =>
        'The selected language server does not support Go to Implementation.',
      _ => 'The selected language server does not support Find References.',
    });
  }

  Future<void> _goToDefinition() => _goToSourceLocations(
    capability: LanguageCapability.definition,
    query: _languageIntelligence.definition,
    pickerTitle: 'Definitions',
    emptyMessage: 'No definition found',
    outsideMessage: 'Definition is outside the current workspace',
    openFailedMessage: 'Could not open definition',
  );

  Future<void> _goToImplementation() => _goToSourceLocations(
    capability: LanguageCapability.implementation,
    query: _languageIntelligence.implementation,
    pickerTitle: 'Implementations',
    emptyMessage: 'No implementation found',
    outsideMessage: 'Implementation is outside the current workspace',
    openFailedMessage: 'Could not open implementation',
  );

  Future<void> _goToSourceLocations({
    required LanguageCapability capability,
    required Future<List<SourceLocation>> Function({
      required String workspaceId,
      required String path,
      required SourcePosition position,
    })
    query,
    required String pickerTitle,
    required String emptyMessage,
    required String outsideMessage,
    required String openFailedMessage,
  }) async {
    final filePath = widget.tab.filePath;
    if (filePath == null) {
      return;
    }
    if (!_canOfferLanguageNavigation(capability)) {
      _showLanguageNavigationUnavailable(capability);
      return;
    }
    await _flushLanguageIntelligenceDocumentSync();
    if (!mounted || widget.tab.filePath != filePath) {
      return;
    }
    if (_languageIntelligenceDocumentPath == null) {
      await _refreshLanguageIntelligenceDocument();
    }
    final sourcePath = _languageIntelligenceDocumentPath;
    if (!mounted || sourcePath == null || widget.tab.filePath != filePath) {
      _showToast('Language server is not ready');
      return;
    }

    final String sourceText;
    try {
      sourceText = await _languageIntelligenceSourceText(filePath);
    } catch (_) {
      _showToast('Language server is not ready');
      return;
    }
    final position = workspaceEditorSourcePositionForDisplayOffset(
      displayText: _controller.text,
      sourceText: sourceText,
      displayScalarOffset: _controller.selection.extentOffset,
      tabSize: _currentEditorTabSize(),
    );

    final List<SourceLocation> locations;
    try {
      locations = await query(
        workspaceId: widget.workspace.id,
        path: sourcePath,
        position: position,
      );
    } on TimeoutException {
      _showToast('Language server did not respond');
      return;
    } on LanguageServerRequestException catch (error) {
      if (error.methodNotSupported) {
        _showLanguageNavigationUnsupported(capability);
      } else {
        _showToast('Language server request failed', tone: .error);
      }
      return;
    } catch (_) {
      _showToast('Language server is not ready');
      return;
    }
    if (!mounted || widget.tab.filePath != filePath) {
      return;
    }

    final choices = <_WorkspaceEditorDefinitionChoice>[];
    final seen = <String>{};
    for (final location in locations) {
      final target = workspaceEditorNavigationTargetForLocation(
        workspaceId: widget.workspace.id,
        workspacePath: widget.workspace.path,
        location: location,
      );
      if (target == null) {
        continue;
      }
      final key =
          '${target.relativePath}:${target.reveal.line}:${target.reveal.column}';
      if (seen.add(key)) {
        choices.add(_WorkspaceEditorDefinitionChoice(target: target));
      }
    }
    if (choices.isEmpty) {
      _showToast(locations.isEmpty ? emptyMessage : outsideMessage);
      return;
    }

    final choice = choices.length == 1
        ? choices.first
        : await _pickDefinitionChoice(choices, title: pickerTitle);
    if (choice == null || !mounted) {
      return;
    }
    await _openDefinitionChoice(choice, failedMessage: openFailedMessage);
  }

  Future<void> _findReferences() async {
    final filePath = widget.tab.filePath;
    if (filePath == null) {
      return;
    }
    if (!_canOfferLanguageNavigation(LanguageCapability.references)) {
      _showLanguageNavigationUnavailable(LanguageCapability.references);
      return;
    }
    await _flushLanguageIntelligenceDocumentSync();
    if (!mounted || widget.tab.filePath != filePath) {
      return;
    }
    if (_languageIntelligenceDocumentPath == null) {
      await _refreshLanguageIntelligenceDocument();
    }
    final sourcePath = _languageIntelligenceDocumentPath;
    if (!mounted || sourcePath == null || widget.tab.filePath != filePath) {
      _showToast('Language server is not ready');
      return;
    }

    final String sourceText;
    try {
      sourceText = await _languageIntelligenceSourceText(filePath);
    } catch (_) {
      _showToast('Language server is not ready');
      return;
    }
    final position = workspaceEditorSourcePositionForDisplayOffset(
      displayText: _controller.text,
      sourceText: sourceText,
      displayScalarOffset: _controller.selection.extentOffset,
      tabSize: _currentEditorTabSize(),
    );

    final workbench = ref.read(workbenchControllerProvider.notifier);
    workbench.setRightSidebarVisible(true);
    workbench.setContextPanelTab(.references);
    await ref
        .read(
          workspaceReferencesControllerProvider(widget.workspace.id).notifier,
        )
        .search(
          workspacePath: widget.workspace.path,
          sourcePath: sourcePath,
          position: position,
        );
  }

  Future<_WorkspaceEditorDefinitionChoice?> _pickDefinitionChoice(
    List<_WorkspaceEditorDefinitionChoice> choices, {
    required String title,
  }) {
    final height = choices.length > 5 ? 360.0 : 88.0 + choices.length * 52.0;
    return showDialog<_WorkspaceEditorDefinitionChoice>(
      context: context,
      builder: (dialogContext) => AleraDialog(
        maxWidth: 560,
        maxHeight: 440,
        child: Padding(
          padding: const EdgeInsets.all(AleraTokens.space20),
          child: SizedBox(
            height: height,
            child: Column(
              crossAxisAlignment: .stretch,
              children: <Widget>[
                Text(
                  dialogContext.tr(title),
                  style: Theme.of(dialogContext).textTheme.titleMedium,
                ),
                const SizedBox(height: AleraTokens.space12),
                Expanded(
                  child: ListView.separated(
                    itemCount: choices.length,
                    separatorBuilder: (_, _) => const Divider(
                      height: 1,
                      color: AleraTokens.borderSubtle,
                    ),
                    itemBuilder: (context, index) {
                      final choice = choices[index];
                      return InkWell(
                        onTap: () => Navigator.of(context).pop(choice),
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: AleraTokens.space8,
                            vertical: AleraTokens.space12,
                          ),
                          child: Row(
                            children: <Widget>[
                              Expanded(
                                child: Text(
                                  choice.target.relativePath,
                                  maxLines: 1,
                                  overflow: .ellipsis,
                                ),
                              ),
                              const SizedBox(width: AleraTokens.space12),
                              Text(
                                'L${choice.target.reveal.line}',
                                style: Theme.of(context).textTheme.bodySmall
                                    ?.copyWith(
                                      color: AleraTokens.foregroundMuted,
                                    ),
                              ),
                            ],
                          ),
                        ),
                      );
                    },
                  ),
                ),
                const SizedBox(height: AleraTokens.space8),
                Align(
                  alignment: Alignment.centerRight,
                  child: TextButton(
                    onPressed: () => Navigator.of(dialogContext).pop(),
                    child: Text(dialogContext.tr('Cancel')),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _openDefinitionChoice(
    _WorkspaceEditorDefinitionChoice choice, {
    required String failedMessage,
  }) async {
    try {
      final tab = await ref
          .read(workbenchControllerProvider.notifier)
          .openEditorTab(
            workspace: widget.workspace,
            relativePath: choice.target.relativePath,
            preview: false,
          );
      if (!mounted) {
        return;
      }
      _editorSessions.reveal(tab.id, choice.target.reveal);
    } catch (_) {
      _showToast(failedMessage, tone: .error);
    }
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

class const _WorkspaceEditorDefinitionChoice({
  required final WorkspaceEditorSourceNavigationTarget target,
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

@visibleForTesting
SourcePosition workspaceEditorSourcePositionForDisplayOffset({
  required String displayText,
  required String sourceText,
  required int displayScalarOffset,
  required int tabSize,
}) {
  final displayPosition = _workspaceEditorDisplayScalarPosition(
    displayText,
    displayScalarOffset,
  );
  final sourceLines = sourceText.split(RegExp(r'\r\n|\r|\n'));
  if (sourceLines.isEmpty) {
    return SourcePosition(line: displayPosition.line, scalarColumn: 0);
  }
  final sourceLineIndex = displayPosition.line.clamp(0, sourceLines.length - 1);
  final sourceColumn = _workspaceEditorSourceScalarColumnForDisplayColumn(
    sourceLine: sourceLines[sourceLineIndex],
    displayScalarColumn: displayPosition.scalarColumn,
    tabSize: tabSize,
  );
  return SourcePosition(line: sourceLineIndex, scalarColumn: sourceColumn);
}

({int line, int scalarColumn}) _workspaceEditorDisplayScalarPosition(
  String text,
  int scalarOffset,
) {
  final runes = text.runes.toList(growable: false);
  final limit = scalarOffset.clamp(0, runes.length);
  var line = 0;
  var column = 0;
  var index = 0;
  while (index < limit) {
    final rune = runes[index];
    if (rune == 0x0D) {
      line += 1;
      column = 0;
      if (index + 1 < limit && runes[index + 1] == 0x0A) {
        index += 1;
      }
    } else if (rune == 0x0A) {
      line += 1;
      column = 0;
    } else {
      column += 1;
    }
    index += 1;
  }
  return (line: line, scalarColumn: column);
}

int _workspaceEditorSourceScalarColumnForDisplayColumn({
  required String sourceLine,
  required int displayScalarColumn,
  required int tabSize,
}) {
  final target = displayScalarColumn.clamp(0, 1 << 30);
  final effectiveTabSize = tabSize.clamp(1, 8);
  var sourceColumn = 0;
  var visualColumn = 0;
  for (final rune in sourceLine.runes) {
    final width = rune == 0x09
        ? effectiveTabSize - (visualColumn % effectiveTabSize)
        : 1;
    if (target < visualColumn + width) {
      return sourceColumn;
    }
    visualColumn += width;
    sourceColumn += 1;
  }
  return sourceColumn;
}

@visibleForTesting
WorkspaceEditorSourceNavigationTarget?
workspaceEditorNavigationTargetForLocation({
  required String workspaceId,
  required String workspacePath,
  required SourceLocation location,
}) {
  if (location.workspaceId != workspaceId) {
    return null;
  }
  final root = p.normalize(p.absolute(workspacePath));
  final targetPath = p.normalize(
    p.isAbsolute(location.path) ? location.path : p.join(root, location.path),
  );
  final relativePath = workspaceRelativePath(
    workspacePath: root,
    filePath: targetPath,
  );
  if (relativePath == null) {
    return null;
  }
  final start = location.range.start;
  final end = location.range.end;
  final matchLength = start.line == end.line
      ? (end.scalarColumn - start.scalarColumn).clamp(1, 1 << 30)
      : 1;
  return WorkspaceEditorSourceNavigationTarget(
    relativePath: relativePath,
    reveal: WorkspaceEditorRevealTarget(
      line: start.line + 1,
      column: start.scalarColumn + 1,
      matchLength: matchLength,
    ),
  );
}

@visibleForTesting
class const WorkspaceEditorSourceNavigationTarget({
  required final String relativePath,
  required final WorkspaceEditorRevealTarget reveal,
});

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
