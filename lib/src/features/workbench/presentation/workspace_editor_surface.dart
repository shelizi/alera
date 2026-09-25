import 'package:alera/src/app/localization/alera_localizations.dart';

import 'dart:async';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/forms/alera_text_actions_scope.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/icons/alera_file_icon.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_activity.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_manager.dart';
import 'package:alera/src/features/language_intelligence/application/language_navigation_port.dart';
import 'package:alera/src/features/language_intelligence/application/language_provider_registry.dart';
import 'package:alera/src/features/language_intelligence/application/language_server_runtime.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/source_location.dart';
import 'package:alera/src/features/settings/domain/editor_syntax_theme_catalog.dart';
import 'package:alera/src/features/workbench/application/editor_autosave_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_file_preview_kind.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/application/workspace_references_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_text_encoding.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_relative_path.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:code_forge/code_forge.dart' as code_forge;
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter/scheduler.dart';
import 'package:path/path.dart' as p;
import 'package:re_highlight/languages/all.dart';
import 'package:re_highlight/re_highlight.dart';

part 'workspace_editor_language_registry.dart';
part 'workspace_editor_comments.dart';
part 'workspace_editor_language_intelligence.dart';
part 'workspace_editor_widgets.dart';
part 'workspace_editor_focus.dart';
part 'workspace_editor_reveal.dart';
part 'workspace_editor_loading.dart';
part 'workspace_editor_outline.dart';
part 'workspace_editor_save.dart';
part 'workspace_editor_text_actions.dart';
part 'workspace_editor_find.dart';

class const WorkspaceEditorSurface({
  super.key,
  required final Workspace workspace,
  final WorkspaceSourceControlScope? sourceControlScope,
  required final WorkspaceTabRecord tab,
  required final bool autofocus,
  final ValueChanged<String>? onOpenMermanPreview,
  required final ValueChanged<String> onOpenMarkdownViewerTab,
  final VoidCallback? onKeepPreview,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceEditorSurface> createState() =>
      _WorkspaceEditorSurfaceState();
}

class _WorkspaceEditorSurfaceState
    extends ConsumerState<WorkspaceEditorSurface> {
  late final code_forge.CodeForgeController _controller;
  late final code_forge.UndoRedoController _undoController;
  late final code_forge.FindController _findController;
  late final ScrollController _verticalScrollController;
  late final ScrollController _horizontalScrollController;
  late WorkspaceEditorFocusNode _focusNode;
  late final WorkspaceFileService _workspaceFiles;
  late final EditorSessionRegistry _editorSessions;
  late final EditorSessionHandle _sessionHandle;
  late final EditorAutosaveController _autosave;
  late final LanguageIntelligenceManager _languageIntelligence;
  late final LanguageIntelligenceActivityReporter _languageActivity;
  late EditorDocumentSession _document;
  Object? _loadError;
  bool _loading = true;
  bool _saving = false;
  bool _stateRefreshQueued = false;
  WorkspaceEditorPerformanceProfile? _lastPerformanceProfile;
  Offset? _lastSecondaryTapGlobalPosition;
  int _loadRequestId = 0;
  late int _lastObservedDocumentVersion;
  Timer? _documentSnapshotDebounceTimer;
  Timer? _languageIntelligenceSyncTimer;
  _WorkspaceEditorSemanticSourceSnapshot? _languageSemanticSource;
  String? _languageIntelligenceWorkspaceId;
  String? _languageIntelligenceDocumentPath;
  int _languageIntelligenceSyncGeneration = 0;
  Timer? _outlineRefreshTimer;
  bool _hasPendingDocumentSnapshot = false;
  bool _suppressControllerChangeHandling = false;
  bool _outlineOpen = false;
  bool _outlineLoading = false;
  bool _outlineTruncated = false;
  int _outlineRequestId = 0;
  code_forge.CodeForgeDocumentSymbolSource? _outlineSource;
  List<code_forge.CodeForgeDocumentSymbol> _outlineSymbols = const [];

  @override
  void initState() {
    super.initState();
    _controller = code_forge.CodeForgeController();
    _controller.nativeSyntaxStatus.addListener(_handleNativeSyntaxStatus);
    _lastObservedDocumentVersion = _controller.documentVersion;
    _applyEditorSettings(
      normalizeWorkspaceEditorTabSize(
        ref.read(settingsControllerProvider).editor.tabSize,
      ),
    );
    _undoController = code_forge.UndoRedoController();
    _findController = code_forge.FindController(_controller);
    _verticalScrollController = ScrollController();
    _horizontalScrollController = ScrollController();
    _focusNode = WorkspaceEditorFocusNode();
    _workspaceFiles = ref.read(workspaceFileServiceProvider);
    _editorSessions = ref.read(editorSessionRegistryProvider);
    _languageIntelligence = ref.read(languageIntelligenceManagerProvider);
    _languageActivity = ref.read(languageIntelligenceActivityProvider);
    _sessionHandle = EditorSessionHandle(
      isDirty: _isDirty,
      save: _save,
      discard: _discardChanges,
      snapshotText: () => _controller.text,
      reveal: _revealOrDefer,
      reload: _reloadFromDiskAfterExternalChange,
      runNavigationCommand: (command) => switch (command) {
        EditorSessionNavigationCommand.goToDefinition => _goToDefinition(),
        EditorSessionNavigationCommand.goToImplementation =>
          _goToImplementation(),
        EditorSessionNavigationCommand.findReferences => _findReferences(),
      },
    );
    _controller.addListener(_handleControllerChanged);
    _document = _editorSessions.documentFor(widget.tab.id);
    final editorSettings = ref.read(settingsControllerProvider).editor;
    _autosave = EditorAutosaveController(
      enabled: editorSettings.autosaveEnabled,
      debounce: editorSettings.autosaveDebounce,
      isDirty: _isDirty,
      isReady: _isReadyForAutosave,
      save: _saveAutomatically,
      onError: _handleAutosaveError,
    );
    ref.listenManual(
      settingsControllerProvider.select((settings) => settings.editor),
      (previous, next) {
        _autosave.updateSettings(
          enabled: next.autosaveEnabled,
          debounce: next.autosaveDebounce,
        );
        if (previous?.languageIntelligence != next.languageIntelligence) {
          unawaited(
            _refreshLanguageIntelligenceDocument(
              settings: next.languageIntelligence,
            ),
          );
        }
      },
    );
    _registerSession(widget.tab.id);
    _restoreDocumentOrLoad();
  }

  @override
  void didUpdateWidget(covariant WorkspaceEditorSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.tab.id != widget.tab.id ||
        oldWidget.workspace.path != widget.workspace.path ||
        oldWidget.tab.filePath != widget.tab.filePath) {
      // `_document` still belongs to the tab being left.
      _rememberEditorViewState();
      _removeNativeSyntaxActivityForPath(oldWidget.tab.filePath);
      unawaited(_closeLanguageIntelligenceDocument());
      _resetLanguageIntelligenceSource();
      _autosave.cancelPending();
      _flushPendingDocumentSnapshot(refreshState: false, notifyAutosave: false);
      _materializeNativeDirtySnapshot();
      _replaceFocusNode();
      _editorSessions.unregister(oldWidget.tab.id, _sessionHandle);
      _document = _editorSessions.documentFor(widget.tab.id);
      if (oldWidget.tab.id == widget.tab.id &&
          oldWidget.tab.filePath != widget.tab.filePath) {
        _document.clearSnapshot();
      }
      _resetOutlineForDocumentChange();
      _registerSession(widget.tab.id);
      _restoreDocumentOrLoad();
    }
  }

  @override
  void deactivate() {
    // The framework unmounts children before calling dispose(), so by then the
    // editor's Scrollable has detached from the scroll controllers and every
    // offset reads as absent. deactivate() still runs with them attached.
    _rememberEditorViewState();
    super.deactivate();
  }

  @override
  void dispose() {
    _languageIntelligenceSyncTimer?.cancel();
    unawaited(_closeLanguageIntelligenceDocument());
    _outlineRefreshTimer?.cancel();
    _flushPendingDocumentSnapshot(refreshState: false, notifyAutosave: false);
    _materializeNativeDirtySnapshot();
    _autosave.dispose();
    _editorSessions.unregister(widget.tab.id, _sessionHandle);
    _focusNode.suppressThirdPartyListeners();
    _focusNode.unfocus();
    _controller.removeListener(_handleControllerChanged);
    _removeNativeSyntaxActivityForPath(widget.tab.filePath);
    _controller.nativeSyntaxStatus.removeListener(_handleNativeSyntaxStatus);
    _findController.dispose();
    _undoController.dispose();
    _controller.dispose();
    _verticalScrollController.dispose();
    _horizontalScrollController.dispose();
    _focusNode.dispose();
    super.dispose();
  }

  void _handleNativeSyntaxStatus() {
    final status = _controller.nativeSyntaxStatus.value;
    final filePath = widget.tab.filePath;
    if (filePath == null) return;
    final language = ref
        .read(languageExtensionRegistryProvider)
        .languageForPath(filePath)
        ?.id;
    if (language == null) return;
    if (status.state == code_forge.CodeForgeNativeSyntaxState.disabled) {
      _languageActivity.removeStructuralParserDocument(
        language: language,
        documentId: filePath,
      );
      return;
    }
    _languageActivity.reportStructuralParserDocument(
      StructuralParserDocumentSnapshot(
        language: language,
        documentId: filePath,
        state: switch (status.state) {
          code_forge.CodeForgeNativeSyntaxState.parsing =>
            StructuralParserActivityState.parsing,
          code_forge.CodeForgeNativeSyntaxState.ready =>
            StructuralParserActivityState.ready,
          code_forge.CodeForgeNativeSyntaxState.unsupported =>
            StructuralParserActivityState.unsupported,
          code_forge.CodeForgeNativeSyntaxState.failed =>
            StructuralParserActivityState.failed,
          code_forge.CodeForgeNativeSyntaxState.disabled =>
            StructuralParserActivityState.unsupported,
        },
        revision: status.revision,
        currentByteOffset: status.currentByteOffset,
        totalBytes: status.totalBytes,
        detail: status.error,
      ),
    );
  }

  void _removeNativeSyntaxActivityForPath(String? filePath) {
    if (filePath == null) return;
    final language = ref
        .read(languageExtensionRegistryProvider)
        .languageForPath(filePath)
        ?.id;
    if (language != null) {
      _languageActivity.removeStructuralParserDocument(
        language: language,
        documentId: filePath,
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final filePath = widget.tab.filePath;
    if (filePath == null) {
      return const _EditorMessage(message: 'This editor tab has no file.');
    }
    final editorSettings = ref.watch(
      settingsControllerProvider.select((settings) => settings.editor),
    );
    final effectiveTabSize = normalizeWorkspaceEditorTabSize(
      editorSettings.tabSize,
    );
    _applyEditorSettings(effectiveTabSize);
    final editorTheme = editorSyntaxThemeForName(editorSettings.themeName);
    final rootStyle = editorSyntaxRootStyleForName(editorSettings.themeName);
    final effectiveThemeName =
        editorSyntaxThemeEntryForName(editorSettings.themeName)?.name ??
        EditorSyntaxThemeNames.alera;
    final languageRegistry = ref.watch(languageExtensionRegistryProvider);
    final syntaxLanguageId = workspaceEditorSyntaxLanguageIdForPath(
      filePath: filePath,
      registry: languageRegistry,
    );
    final enableNativeSyntax = workspaceEditorNativeSyntaxEnabled(
      filePath: filePath,
      registry: languageRegistry,
      settings: editorSettings.languageIntelligence,
    );
    final performanceProfile = workspaceEditorPerformanceProfile(
      lineCount: _controller.lineCount,
      contentLength: _document.currentText?.length ?? 0,
    );
    _lastPerformanceProfile = performanceProfile;
    Widget content;
    if (_loading) {
      content = const Center(child: CircularProgressIndicator());
    } else if (_loadError case final loadError?) {
      content = _EditorMessage(message: _messageFor(loadError));
    } else {
      content = Stack(
        children: <Widget>[
          Listener(
            onPointerDown: _captureEditorPointerDown,
            child: CallbackShortcuts(
              bindings: <ShortcutActivator, VoidCallback>{
                const SingleActivator(
                  LogicalKeyboardKey.slash,
                  control: true,
                ): () => _toggleEditorComment(
                  filePath: filePath,
                  languageId: syntaxLanguageId,
                ),
                const SingleActivator(
                  LogicalKeyboardKey.slash,
                  meta: true,
                ): () => _toggleEditorComment(
                  filePath: filePath,
                  languageId: syntaxLanguageId,
                ),
              },
              child: Focus(
                canRequestFocus: false,
                onKeyEvent: _handleEditorFindNavigationKey,
                child: code_forge.CodeForge(
                  key: ValueKey<String>(
                    workspaceEditorCodeForgeKey(
                      tabId: widget.tab.id,
                      filePath: filePath,
                      themeName: effectiveThemeName,
                    ),
                  ),
                  controller: _controller,
                  undoController: _undoController,
                  findController: _findController,
                  verticalScrollController: _verticalScrollController,
                  horizontalScrollController: _horizontalScrollController,
                  focusNode: _focusNode,
                  autoFocus: widget.autofocus,
                  lineWrap: performanceProfile.lineWrap,
                  enableLocalSuggestions: false,
                  enableGuideLines: performanceProfile.guideLines,
                  enableFolding: performanceProfile.folding,
                  largeFilePerformanceMode:
                      performanceProfile.largeFilePerformanceMode,
                  enableGutter: true,
                  enableGutterDivider: false,
                  editorTheme: editorTheme,
                  language: performanceProfile.syntaxHighlighting
                      ? workspaceEditorSyntaxModeForPath(
                          filePath: filePath,
                          registry: languageRegistry,
                          syntaxLanguageId: syntaxLanguageId,
                        )
                      : _plainTextLanguage,
                  languageId: syntaxLanguageId,
                  enableNativeSyntax: enableNativeSyntax,
                  tabSize: effectiveTabSize,
                  useSpaceAsTab: true,
                  keyboardShotcuts: workspaceEditorKeyboardShortcutsForPlatform(
                    Theme.of(context).platform,
                  ),
                  finderBuilder: (context, findController) =>
                      _WorkspaceEditorFindBar(
                        controller: findController,
                        onClose: _closeEditorFind,
                      ),
                  scrollbarDecoration: workspaceEditorScrollbarDecoration(),
                  suggestionStyle: _editorOverlayStyle(context),
                  customContextMenuItems: _editorContextMenuItems(context),
                  onNavigationClick: (_) =>
                      unawaited(_goToImplementationOrDefinition()),
                  textStyle: Theme.of(context).textTheme.bodyMedium?.copyWith(
                    fontFamily: 'JetBrains Mono',
                    color: rootStyle.color ?? AleraTokens.foreground,
                    height: 1.35,
                  ),
                ),
              ),
            ),
          ),
          if (_saving)
            const Positioned(
              top: AleraTokens.space8,
              right: AleraTokens.space8,
              child: SizedBox.square(
                dimension: AleraTokens.space16,
                child: CircularProgressIndicator(strokeWidth: 2),
              ),
            ),
        ],
      );
    }
    return DecoratedBox(
      decoration: const BoxDecoration(color: AleraTokens.bg),
      child: Column(
        crossAxisAlignment: .stretch,
        children: <Widget>[
          _EditorFileBar(
            path: workspaceEditorDisplayPath(
              workspace: widget.workspace,
              filePath: filePath,
            ),
            dirty: _document.isDirty,
            saving: _saving,
            encodingSelection: WorkspaceTextEncodingSelection.fromEncoding(
              _document.requestedEncoding,
            ),
            detectedEncoding: _document.encoding,
            onEncodingSelected: _loading || _saving
                ? null
                : (selection) => unawaited(_changeEncoding(selection)),
            onViewDiff: !_loading ? () => unawaited(_openDiffForFile()) : null,
            onSave: _document.isDirty && !_loading && !_saving
                ? () => unawaited(_save())
                : null,
            onDiscard: _document.isDirty && !_loading && !_saving
                ? () => unawaited(_discardChanges())
                : null,
            onOpenPreview: _openPreviewActionFor(filePath),
            outlineOpen: _outlineOpen,
            onToggleOutline: _loading || _loadError != null
                ? null
                : _toggleOutline,
          ),
          const Divider(height: 1, color: AleraTokens.borderSubtle),
          Expanded(
            child: Row(
              crossAxisAlignment: .stretch,
              children: <Widget>[
                Expanded(child: content),
                if (_outlineOpen) ...<Widget>[
                  const VerticalDivider(
                    width: 1,
                    thickness: 1,
                    color: AleraTokens.borderSubtle,
                  ),
                  SizedBox(
                    width: 280,
                    child: _EditorOutlinePanel(
                      loading: _outlineLoading,
                      symbols: _outlineSymbols,
                      truncated: _outlineTruncated,
                      source: _outlineSource,
                      onRefresh: _loading ? null : _refreshOutline,
                      onSelect: _navigateToOutlineSymbol,
                    ),
                  ),
                ],
              ],
            ),
          ),
        ],
      ),
    );
  }

  VoidCallback? _openPreviewActionFor(String filePath) {
    if (isWorkspaceMermanFilePath(filePath) &&
        widget.onOpenMermanPreview != null) {
      return () => widget.onOpenMermanPreview?.call(filePath);
    }
    if (isWorkspaceMarkdownFilePath(filePath)) {
      return () => widget.onOpenMarkdownViewerTab(filePath);
    }
    return null;
  }

  Future<void> _openDiffForFile() async {
    final filePath = widget.tab.filePath;
    if (filePath == null) {
      return;
    }
    final diffTarget = workspaceEditorDiffTargetForFile(
      workspace: widget.workspace,
      filePath: filePath,
      sourceControlScope: widget.sourceControlScope,
    );
    if (diffTarget == null) {
      _showToast('No Git diff for this file');
      return;
    }
    try {
      final status = await ref
          .read(gitBackendProvider)
          .statusForPath(
            path: diffTarget.gitPath,
            filePath: diffTarget.gitFilePath,
          );
      final entries = status.entriesForPath(diffTarget.gitFilePath);
      if (!mounted) {
        return;
      }
      if (entries.isEmpty) {
        _showToast('No Git diff for this file');
        return;
      }
      if (entries.length == 1) {
        await ref
            .read(workbenchControllerProvider.notifier)
            .openGitDiffTab(
              workspace: widget.workspace,
              relativePath: filePath,
              area: entries.single.area,
              scope: .file,
              gitDiffRoot: diffTarget.gitDiffRoot,
              preview: true,
            );
        return;
      }
      final choice = await _showDiffChoiceMenu(entries);
      if (!mounted || choice == null) {
        return;
      }
      if (choice.allForFile) {
        await ref
            .read(workbenchControllerProvider.notifier)
            .openGitDiffTab(
              workspace: widget.workspace,
              relativePath: filePath,
              scope: .fileAll,
              gitDiffRoot: diffTarget.gitDiffRoot,
              preview: true,
            );
        return;
      }
      await ref
          .read(workbenchControllerProvider.notifier)
          .openGitDiffTab(
            workspace: widget.workspace,
            relativePath: filePath,
            area: choice.area,
            scope: .file,
            gitDiffRoot: diffTarget.gitDiffRoot,
            preview: true,
          );
    } catch (_) {
      if (mounted) {
        _showToast('Could not open Git diff', tone: .error);
      }
    }
  }

  Future<_DiffOpenChoice?> _showDiffChoiceMenu(List<GitChangeEntry> entries) {
    final overlay =
        Navigator.of(context).overlay!.context.findRenderObject()! as RenderBox;
    return showMenu<_DiffOpenChoice>(
      context: context,
      position: .fromLTRB(
        overlay.size.width - 280,
        AleraTokens.sidebarHeaderHeight,
        AleraTokens.space16,
        0,
      ),
      items: <PopupMenuEntry<_DiffOpenChoice>>[
        for (final entry in entries)
          PopupMenuItem<_DiffOpenChoice>(
            value: _DiffOpenChoice(area: entry.area),
            child: Text(context.tr('${entry.area.label} changes')),
          ),
        const PopupMenuDivider(height: AleraTokens.space8),
        PopupMenuItem<_DiffOpenChoice>(
          value: const _DiffOpenChoice(allForFile: true),
          child: Text(context.tr('All changes for file')),
        ),
      ],
    );
  }

  bool _isDirty() => _hasPendingDocumentSnapshot || _document.isDirty;

  void _materializeNativeDirtySnapshot() {
    if (!_document.nativeBacked ||
        !_document.isDirty ||
        _document.currentText != null) {
      return;
    }
    _document.updateCurrentText(_controller.text);
  }

  Future<code_forge.WorkspaceSourceInfo> _openControllerFromWorkspace({
    required String workspacePath,
    required String relativePath,
    required int tabSize,
    native.WorkspaceTextEncoding? encoding,
  }) async {
    _suppressControllerChangeHandling = true;
    try {
      final info = await _controller.openWorkspaceFile(
        workspacePath: workspacePath,
        relativePath: relativePath,
        tabSize: tabSize,
        encoding: _toCodeForgeEncoding(encoding),
      );
      _lastObservedDocumentVersion = _controller.documentVersion;
      return info;
    } finally {
      _suppressControllerChangeHandling = false;
    }
  }

  void _replaceControllerText(String text) {
    _suppressControllerChangeHandling = true;
    try {
      _controller.text = text;
      _lastObservedDocumentVersion = _controller.documentVersion;
    } finally {
      _suppressControllerChangeHandling = false;
    }
  }

  void _refreshStateSafely() {
    if (!mounted) {
      return;
    }
    final phase = SchedulerBinding.instance.schedulerPhase;
    if (phase == SchedulerPhase.idle ||
        phase == SchedulerPhase.postFrameCallbacks) {
      setState(() {});
      return;
    }
    if (_stateRefreshQueued) {
      return;
    }
    _stateRefreshQueued = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _stateRefreshQueued = false;
      if (mounted) {
        setState(() {});
      }
    });
  }

  void _setEditorState(VoidCallback update) {
    setState(update);
  }

  void _registerSession(String tabId) {
    _editorSessions.register(tabId, _sessionHandle);
  }

  void _replaceFocusNode() {
    final previous = _focusNode;
    previous.suppressThirdPartyListeners();
    previous.unfocus();
    _focusNode = WorkspaceEditorFocusNode();
    SchedulerBinding.instance.addPostFrameCallback((_) {
      previous.dispose();
    });
  }

  void _applyEditorSettings(int tabSize) {
    if (_controller.tabSize != tabSize) {
      _controller.tabSize = tabSize;
    }
    if (!_controller.useSpaceAsTab) {
      _controller.useSpaceAsTab = true;
    }
  }

  int _currentEditorTabSize() {
    return normalizeWorkspaceEditorTabSize(
      ref.read(settingsControllerProvider).editor.tabSize,
    );
  }

  void _showToast(String message, {AleraToastTone tone = AleraToastTone.info}) {
    if (!mounted) {
      return;
    }
    AleraToast.show(context, message: context.tr(message), tone: tone);
  }

  String _messageFor(Object error) {
    if (error is native.WorkspaceFileError) {
      return switch (error.kind) {
        native.WorkspaceFileErrorKind.unsupported => 'File cannot be edited',
        native.WorkspaceFileErrorKind.notFound => 'File not found',
        native.WorkspaceFileErrorKind.outsideWorkspace =>
          'File is outside the workspace',
        native.WorkspaceFileErrorKind.protectedPath => 'File is protected',
        native.WorkspaceFileErrorKind.conflict => 'File changed on disk',
        _ => 'File operation failed',
      };
    }
    return 'File operation failed';
  }
}

@visibleForTesting
const int workspaceEditorLargeFileLineThreshold = 5000;

@visibleForTesting
const int workspaceEditorLargeFileCharacterThreshold = 512 * 1024;
const Duration workspaceEditorLargeFileSnapshotDebounce = Duration(
  milliseconds: 75,
);

typedef WorkspaceEditorPerformanceProfile = ({
  bool lineWrap,
  bool guideLines,
  bool folding,
  bool syntaxHighlighting,
  bool largeFilePerformanceMode,
});

@visibleForTesting
WorkspaceEditorPerformanceProfile workspaceEditorPerformanceProfile({
  required int lineCount,
  required int contentLength,
}) {
  final largeFile =
      lineCount >= workspaceEditorLargeFileLineThreshold ||
      contentLength >= workspaceEditorLargeFileCharacterThreshold;
  return (
    lineWrap: !largeFile,
    guideLines: !largeFile,
    folding: !largeFile,
    syntaxHighlighting: !largeFile,
    largeFilePerformanceMode: largeFile,
  );
}

@visibleForTesting
bool workspaceEditorShouldDeferDocumentSnapshot({
  required int lineCount,
  required int contentLength,
}) {
  return lineCount >= workspaceEditorLargeFileLineThreshold ||
      contentLength >= workspaceEditorLargeFileCharacterThreshold;
}

@visibleForTesting
bool workspaceEditorShouldSyncControllerText({
  required int previousDocumentVersion,
  required int currentDocumentVersion,
}) {
  return previousDocumentVersion != currentDocumentVersion;
}

@visibleForTesting
bool workspaceEditorShouldRefreshSurface({
  required bool wasDirty,
  required bool isDirty,
  required WorkspaceEditorPerformanceProfile previousProfile,
  required WorkspaceEditorPerformanceProfile currentProfile,
}) {
  return wasDirty != isDirty || previousProfile != currentProfile;
}

@visibleForTesting
code_forge.ScrollbarDecoration workspaceEditorScrollbarDecoration() {
  return const code_forge.ScrollbarDecoration(
    showLineNumberIndicator: false,
    thickness: 6,
    thumbColor: AleraTokens.foregroundMuted,
    interactive: true,
    minThumbLength: 24,
    crossAxisMargin: 2,
    mainAxisMargin: 2,
    trackVisibility: false,
    trackColor: Colors.transparent,
    trackBorderColor: Colors.transparent,
  );
}

class const _DiffOpenChoice({
  final GitChangeArea? area,
  final bool allForFile = false,
});

typedef WorkspaceEditorDiffTarget = ({
  String gitPath,
  String gitFilePath,
  String? gitDiffRoot,
});

@visibleForTesting
WorkspaceEditorDiffTarget? workspaceEditorDiffTargetForFile({
  required Workspace workspace,
  required String filePath,
  required WorkspaceSourceControlScope? sourceControlScope,
}) {
  final sourceFilePath = sourceControlScope?.toSourceRelativePath(filePath);
  if (sourceControlScope != null && sourceFilePath == null) {
    return null;
  }
  return (
    gitPath: sourceControlScope?.path ?? workspace.path,
    gitFilePath: sourceFilePath ?? filePath,
    gitDiffRoot: sourceControlScope?.relativeRoot,
  );
}

String workspaceEditorDisplayPath({
  required Workspace workspace,
  required String filePath,
}) {
  if (!p.isAbsolute(filePath)) {
    return filePath;
  }
  return workspaceRelativePath(
        workspacePath: workspace.path,
        filePath: filePath,
      ) ??
      filePath;
}
