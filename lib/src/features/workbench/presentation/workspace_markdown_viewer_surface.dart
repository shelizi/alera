import 'dart:async';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/icons/alera_file_icon.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/typography/alera_search_highlighted_text.dart';
import 'package:alera/src/design_system/menus/alera_text_selection_toolbar.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_editor_surface.dart';
import 'package:alera/src/features/workbench/presentation/workspace_markdown_uri_policy.dart';
import 'package:alera/src/features/workbench/presentation/workspace_markdown_viewer_images.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter/services.dart';
import 'package:gpt_markdown/gpt_markdown.dart';

@visibleForTesting
const Duration workspaceMarkdownViewerEditorUpdateDebounce = Duration(
  milliseconds: 75,
);

class const WorkspaceMarkdownViewerSurface({
  super.key,
  required final Workspace workspace,
  required final WorkspaceTabRecord tab,
  required final ValueChanged<String> onOpenEditorTab,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceMarkdownViewerSurface> createState() =>
      _WorkspaceMarkdownViewerSurfaceState();
}

class _WorkspaceMarkdownViewerSurfaceState
    extends ConsumerState<WorkspaceMarkdownViewerSurface> {
  late final WorkspaceFileService _workspaceFiles;
  late final EditorSessionRegistry _editorSessions;
  String? _content;
  Object? _loadError;
  bool _loading = true;
  bool _usingDirtyEditorContent = false;
  int _loadRequestId = 0;
  Listenable? _editorDocumentChanges;
  Timer? _editorUpdateDebounceTimer;
  late final ScrollController _verticalScrollController;
  late final TextEditingController _searchController;
  late final FocusNode _searchFocusNode;
  late final FocusNode _surfaceFocusNode;
  final GlobalKey _markdownContentKey = GlobalKey(
    debugLabel: 'markdown-viewer-content',
  );
  bool _searchOpen = false;
  int _searchMatchCount = 0;
  int _searchMatchIndex = -1;

  @override
  void initState() {
    super.initState();
    _workspaceFiles = ref.read(workspaceFileServiceProvider);
    _editorSessions = ref.read(editorSessionRegistryProvider);
    _verticalScrollController = ScrollController();
    _searchController = TextEditingController();
    _searchFocusNode = FocusNode(debugLabel: 'MarkdownPreviewSearch');
    _surfaceFocusNode = FocusNode(debugLabel: 'MarkdownPreview');
    _subscribeToEditorDocument();
    unawaited(_load());
  }

  @override
  void didUpdateWidget(covariant WorkspaceMarkdownViewerSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.workspace.path != widget.workspace.path ||
        oldWidget.tab.filePath != widget.tab.filePath) {
      _searchController.clear();
      _searchOpen = false;
      _searchMatchCount = 0;
      _searchMatchIndex = -1;
      _subscribeToEditorDocument();
      unawaited(_load());
    }
  }

  @override
  void dispose() {
    _editorUpdateDebounceTimer?.cancel();
    _editorDocumentChanges?.removeListener(_handleEditorSessionChanged);
    _verticalScrollController.dispose();
    _searchController.dispose();
    _searchFocusNode.dispose();
    _surfaceFocusNode.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final filePath = widget.tab.filePath;
    if (filePath == null) {
      return const _MarkdownViewerMessage(
        message: 'This markdown viewer tab has no file.',
      );
    }

    Widget content;
    if (_loading) {
      content = const Center(child: CircularProgressIndicator());
    } else if (_loadError case final loadError?) {
      content = _MarkdownViewerMessage(message: _messageFor(loadError));
    } else {
      content = ScrollbarTheme(
        data: Theme.of(context).scrollbarTheme.copyWith(
          thumbVisibility: WidgetStateProperty.all(true),
          trackVisibility: WidgetStateProperty.all(true),
          thickness: WidgetStateProperty.all(8),
          thumbColor: WidgetStateProperty.all(AleraTokens.foregroundMuted),
          trackColor: WidgetStateProperty.all(AleraTokens.surfaceElevated),
          trackBorderColor: WidgetStateProperty.all(AleraTokens.borderSubtle),
          radius: const Radius.circular(4),
        ),
        child: SelectionArea(
          contextMenuBuilder: AleraTextSelectionToolbar.selectableRegion,
          child: Scrollbar(
            controller: _verticalScrollController,
            thumbVisibility: true,
            notificationPredicate: (notification) =>
                notification.metrics.axis == Axis.vertical,
            child: SingleChildScrollView(
              key: PageStorageKey<String>(
                'markdown-viewer-scroll:${widget.workspace.id}:${widget.tab.id}:$filePath',
              ),
              controller: _verticalScrollController,
              padding: const EdgeInsets.all(AleraTokens.space24),
              child: KeyedSubtree(
                key: _markdownContentKey,
                child: GptMarkdownTheme(
                  gptThemeData: GptMarkdownThemeData(
                    brightness: .dark,
                    linkColor: AleraTokens.info,
                    highlightColor: AleraTokens.accentSubtle,
                  ),
                  child: DefaultTextStyle(
                    style:
                        Theme.of(context).textTheme.bodyMedium?.copyWith(
                          color: AleraTokens.foreground,
                          height: 1.45,
                        ) ??
                        const TextStyle(
                          color: AleraTokens.foreground,
                          height: 1.45,
                        ),
                    child: GptMarkdown(
                      _content ?? '',
                      styleSheet: const GptMarkdownStyleSheet(
                        latex: LatexStyle(scrollBlockHorizontally: true),
                      ),
                      inlinePatterns: _markdownSearchPatterns(),
                      codeBuilder: (context, name, code, closed) =>
                          _MarkdownViewerCodeBlock(
                            language: name,
                            code: code,
                            searchQuery: _searchOpen
                                ? _searchController.text
                                : '',
                          ),
                      imageBuilder: _buildImage,
                      onLinkTap: (url, _) => unawaited(_openLink(url)),
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      );
    }

    final platform = Theme.of(context).platform;
    final useMeta = platform == TargetPlatform.macOS;
    return CallbackShortcuts(
      bindings: <ShortcutActivator, VoidCallback>{
        SingleActivator(
          LogicalKeyboardKey.keyF,
          control: !useMeta,
          meta: useMeta,
        ): _openSearch,
      },
      child: Focus(
        focusNode: _surfaceFocusNode,
        autofocus: true,
        child: DecoratedBox(
          decoration: const BoxDecoration(color: AleraTokens.bg),
          child: Column(
            crossAxisAlignment: .stretch,
            children: <Widget>[
              _MarkdownViewerFileBar(
                path: workspaceEditorDisplayPath(
                  workspace: widget.workspace,
                  filePath: filePath,
                ),
                loading: _loading,
                searchOpen: _searchOpen,
                onSearch: _openSearch,
                onRefresh: () => unawaited(_load()),
                onOpenEditor: () => widget.onOpenEditorTab(filePath),
              ),
              if (_searchOpen)
                _MarkdownViewerSearchBar(
                  controller: _searchController,
                  focusNode: _searchFocusNode,
                  matchCount: _searchMatchCount,
                  matchIndex: _searchMatchIndex,
                  onChanged: _updateSearch,
                  onPrevious: _previousSearchMatch,
                  onNext: _nextSearchMatch,
                  onClose: _closeSearch,
                ),
              const Divider(height: 1, color: AleraTokens.borderSubtle),
              Expanded(child: content),
            ],
          ),
        ),
      ),
    );
  }

  List<InlinePattern>? _markdownSearchPatterns() {
    if (!_searchOpen) {
      return null;
    }
    final query = _searchController.text.trim();
    if (query.isEmpty) {
      return null;
    }
    return <InlinePattern>[
      InlinePattern(
        pattern: RegExp(
          RegExp.escape(query),
          caseSensitive: false,
          multiLine: true,
        ),
        scopes: MarkdownComponent.allScopes,
        builder: (context, match, style) =>
            TextSpan(text: match.group(0), style: aleraSearchMatchStyle(style)),
      ),
    ];
  }

  void _openSearch() {
    if (!_searchOpen) {
      setState(() => _searchOpen = true);
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted) {
        return;
      }
      _searchFocusNode.requestFocus();
      _searchController.selection = TextSelection(
        baseOffset: 0,
        extentOffset: _searchController.text.length,
      );
    });
  }

  void _closeSearch() {
    if (!_searchOpen) {
      return;
    }
    setState(() => _searchOpen = false);
    _surfaceFocusNode.requestFocus();
  }

  void _updateSearch(String query) {
    final offsets = markdownViewerSearchMatchOffsets(_content ?? '', query);
    setState(() {
      _searchMatchCount = offsets.length;
      _searchMatchIndex = offsets.isEmpty ? -1 : 0;
    });
    _revealSearchMatch(offsets);
  }

  void _previousSearchMatch() => _moveSearchMatch(-1);

  void _nextSearchMatch() => _moveSearchMatch(1);

  void _moveSearchMatch(int delta) {
    final offsets = markdownViewerSearchMatchOffsets(
      _content ?? '',
      _searchController.text,
    );
    if (offsets.isEmpty) {
      return;
    }
    setState(() {
      final current = _searchMatchIndex < 0 ? 0 : _searchMatchIndex;
      _searchMatchIndex = (current + delta) % offsets.length;
      if (_searchMatchIndex < 0) {
        _searchMatchIndex += offsets.length;
      }
      _searchMatchCount = offsets.length;
    });
    _revealSearchMatch(offsets);
  }

  void _refreshSearchForContentChange() {
    if (!_searchOpen || _searchController.text.isEmpty) {
      return;
    }
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        _updateSearch(_searchController.text);
      }
    });
  }

  void _revealSearchMatch([List<int>? offsets]) {
    if (!_verticalScrollController.hasClients || _searchMatchIndex < 0) {
      return;
    }
    final matches =
        offsets ??
        markdownViewerSearchMatchOffsets(
          _content ?? '',
          _searchController.text,
        );
    if (_searchMatchIndex >= matches.length || matches.isEmpty) {
      return;
    }
    final contentLength = (_content ?? '').length;
    if (contentLength == 0) {
      return;
    }
    final ratio = (matches[_searchMatchIndex] / contentLength).clamp(0.0, 1.0);
    final target = _verticalScrollController.position.maxScrollExtent * ratio;
    unawaited(
      _verticalScrollController.animateTo(
        target,
        duration: const Duration(milliseconds: 160),
        curve: Curves.easeOut,
      ),
    );
  }

  Future<void> _load() async {
    final filePath = widget.tab.filePath;
    if (filePath == null) {
      return;
    }
    final dirtyEditorContent = _dirtyEditorContentForCurrentFile();
    if (dirtyEditorContent != null) {
      if (!shouldUpdateMarkdownViewerDirtyContent(
        currentContent: _content,
        dirtyEditorContent: dirtyEditorContent,
        loading: _loading,
        loadError: _loadError,
        usingDirtyEditorContent: _usingDirtyEditorContent,
      )) {
        return;
      }
      _loadRequestId += 1;
      setState(() {
        _content = dirtyEditorContent;
        _loadError = null;
        _loading = false;
        _usingDirtyEditorContent = true;
      });
      _refreshSearchForContentChange();
      return;
    }
    final requestId = ++_loadRequestId;
    setState(() {
      _loading = true;
      _loadError = null;
      _usingDirtyEditorContent = false;
    });
    try {
      final file = await _workspaceFiles.readTextFile(
        workspacePath: widget.workspace.path,
        relativePath: filePath,
      );
      if (!mounted || requestId != _loadRequestId) {
        return;
      }
      final latestDirtyEditorContent = _dirtyEditorContentForCurrentFile();
      setState(() {
        _content = latestDirtyEditorContent ?? file.content;
        _loadError = null;
        _loading = false;
        _usingDirtyEditorContent = latestDirtyEditorContent != null;
      });
      _refreshSearchForContentChange();
    } catch (error) {
      if (!mounted || requestId != _loadRequestId) {
        return;
      }
      final latestDirtyEditorContent = _dirtyEditorContentForCurrentFile();
      setState(() {
        _content = latestDirtyEditorContent;
        _loadError = latestDirtyEditorContent == null ? error : null;
        _loading = false;
        _usingDirtyEditorContent = latestDirtyEditorContent != null;
      });
      _refreshSearchForContentChange();
    }
  }

  void _handleEditorSessionChanged() {
    if (!mounted) {
      return;
    }
    _editorUpdateDebounceTimer?.cancel();
    _editorUpdateDebounceTimer = Timer(
      workspaceMarkdownViewerEditorUpdateDebounce,
      () {
        _editorUpdateDebounceTimer = null;
        if (mounted) {
          _applyEditorSessionChange();
        }
      },
    );
  }

  void _applyEditorSessionChange() {
    final dirtyEditorContent = _dirtyEditorContentForCurrentFile();
    if (dirtyEditorContent != null) {
      _loadRequestId += 1;
      setState(() {
        _content = dirtyEditorContent;
        _loadError = null;
        _loading = false;
        _usingDirtyEditorContent = true;
      });
      _refreshSearchForContentChange();
      return;
    }
    if (_usingDirtyEditorContent) {
      unawaited(_load());
    }
  }

  void _subscribeToEditorDocument() {
    _editorDocumentChanges?.removeListener(_handleEditorSessionChanged);
    _editorUpdateDebounceTimer?.cancel();
    _editorUpdateDebounceTimer = null;
    final filePath = widget.tab.filePath;
    if (filePath == null) {
      _editorDocumentChanges = null;
      return;
    }
    _editorDocumentChanges = _editorSessions.documentChangesForPath(
      workspacePath: widget.workspace.path,
      relativePath: filePath,
    )..addListener(_handleEditorSessionChanged);
  }

  String? _dirtyEditorContentForCurrentFile() {
    final filePath = widget.tab.filePath;
    if (filePath == null) {
      return null;
    }
    return _editorSessions.dirtyTextForPath(
      workspacePath: widget.workspace.path,
      relativePath: filePath,
    );
  }

  Widget _buildImage(
    BuildContext context,
    String imageUrl,
    double? width,
    double? height,
  ) {
    return buildMarkdownViewerImage(
      workspacePath: widget.workspace.path,
      markdownPath: widget.tab.filePath,
      imageUrl: imageUrl,
      width: width,
      height: height,
    );
  }

  Future<void> _openLink(String rawUrl) async {
    final uri = Uri.tryParse(rawUrl);
    if (!isSupportedMarkdownViewerLinkUri(uri)) {
      _showToast('Link cannot be opened', tone: .error);
      return;
    }
    try {
      await ref.read(externalUriLauncherProvider).open(uri!);
    } catch (_) {
      _showToast('Link cannot be opened', tone: .error);
    }
  }

  void _showToast(String message, {AleraToastTone tone = AleraToastTone.info}) {
    if (!mounted) {
      return;
    }
    AleraToast.show(context, message: message, tone: tone);
  }

  String _messageFor(Object error) {
    if (error is native.WorkspaceFileError) {
      return switch (error.kind) {
        native.WorkspaceFileErrorKind.unsupported => 'File cannot be previewed',
        native.WorkspaceFileErrorKind.notFound => 'File not found',
        native.WorkspaceFileErrorKind.outsideWorkspace =>
          'File is outside the workspace',
        native.WorkspaceFileErrorKind.protectedPath => 'File is protected',
        _ => 'File operation failed',
      };
    }
    return 'File operation failed';
  }
}

@visibleForTesting
List<int> markdownViewerSearchMatchOffsets(String content, String query) {
  final needle = query.trim().toLowerCase();
  if (needle.isEmpty || content.isEmpty) {
    return const <int>[];
  }
  final haystack = content.toLowerCase();
  final offsets = <int>[];
  var start = 0;
  while (start <= haystack.length - needle.length) {
    final match = haystack.indexOf(needle, start);
    if (match < 0) {
      break;
    }
    offsets.add(match);
    start = match + needle.length;
  }
  return offsets;
}

@visibleForTesting
bool shouldUpdateMarkdownViewerDirtyContent({
  required String? currentContent,
  required String dirtyEditorContent,
  required bool loading,
  required Object? loadError,
  required bool usingDirtyEditorContent,
}) {
  return loading ||
      loadError != null ||
      !usingDirtyEditorContent ||
      currentContent != dirtyEditorContent;
}

class const _MarkdownViewerSearchBar({
  required final TextEditingController controller,
  required final FocusNode focusNode,
  required final int matchCount,
  required final int matchIndex,
  required final ValueChanged<String> onChanged,
  required final VoidCallback onPrevious,
  required final VoidCallback onNext,
  required final VoidCallback onClose,
}) extends StatelessWidget {
  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) {
      return KeyEventResult.ignored;
    }
    if (event.logicalKey == LogicalKeyboardKey.escape) {
      onClose();
      return KeyEventResult.handled;
    }
    if (event.logicalKey == LogicalKeyboardKey.enter ||
        event.logicalKey == LogicalKeyboardKey.f3) {
      if (HardwareKeyboard.instance.isShiftPressed) {
        onPrevious();
      } else {
        onNext();
      }
      return KeyEventResult.handled;
    }
    return KeyEventResult.ignored;
  }

  @override
  Widget build(BuildContext context) {
    final label = controller.text.isEmpty
        ? null
        : matchCount == 0
        ? 'No results'
        : '${matchIndex + 1}/$matchCount';
    return DecoratedBox(
      decoration: const BoxDecoration(
        color: AleraTokens.surface,
        border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
      ),
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space4),
        child: Focus(
          onKeyEvent: _handleKey,
          child: Row(
            children: <Widget>[
              Expanded(
                child: AleraTextField(
                  controller: controller,
                  focusNode: focusNode,
                  hintText: 'Find in Preview',
                  prefixIcon: AleraIcons.search,
                  dense: true,
                  fillColor: AleraTokens.surfaceVariant,
                  onChanged: onChanged,
                ),
              ),
              if (label != null) ...<Widget>[
                const SizedBox(width: AleraTokens.space8),
                Text(label, style: AleraTokens.monoStyle),
              ],
              const SizedBox(width: AleraTokens.space4),
              AleraIconButton(
                tooltip: 'Previous Match',
                icon: AleraIcons.chevronUp,
                onPressed: matchCount == 0 ? null : onPrevious,
                minSize: AleraTokens.space32,
              ),
              AleraIconButton(
                tooltip: 'Next Match',
                icon: AleraIcons.chevronDown,
                onPressed: matchCount == 0 ? null : onNext,
                minSize: AleraTokens.space32,
              ),
              AleraIconButton(
                tooltip: 'Close Search',
                icon: AleraIcons.close,
                onPressed: onClose,
                minSize: AleraTokens.space32,
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class const _MarkdownViewerCodeBlock({
  required final String language,
  required final String code,
  required final String searchQuery,
}) extends StatefulWidget {
  @override
  State<_MarkdownViewerCodeBlock> createState() =>
      _MarkdownViewerCodeBlockState();
}

class _MarkdownViewerCodeBlockState extends State<_MarkdownViewerCodeBlock> {
  late final ScrollController _horizontalController;

  @override
  void initState() {
    super.initState();
    _horizontalController = ScrollController();
  }

  @override
  void dispose() {
    _horizontalController.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final language = widget.language.trim();
    return Container(
      margin: const EdgeInsets.symmetric(vertical: AleraTokens.space8),
      decoration: BoxDecoration(
        color: AleraTokens.surfaceVariant,
        border: Border.all(color: AleraTokens.borderSubtle),
        borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
      ),
      clipBehavior: Clip.antiAlias,
      child: Column(
        crossAxisAlignment: .stretch,
        children: <Widget>[
          if (language.isNotEmpty)
            Padding(
              padding: const EdgeInsets.only(
                left: AleraTokens.space12,
                top: AleraTokens.space6,
                right: AleraTokens.space4,
                bottom: AleraTokens.space4,
              ),
              child: Row(
                children: <Widget>[
                  Expanded(
                    child: Text(
                      language,
                      style: Theme.of(context).textTheme.labelSmall?.copyWith(
                        color: AleraTokens.foregroundMuted,
                        fontFamily: 'JetBrains Mono',
                      ),
                    ),
                  ),
                  AleraIconButton(
                    tooltip: 'Copy code',
                    icon: AleraIcons.copy,
                    minSize: AleraTokens.space32,
                    onPressed: () => unawaited(
                      Clipboard.setData(ClipboardData(text: widget.code)),
                    ),
                  ),
                ],
              ),
            ),
          Scrollbar(
            key: const ValueKey<String>('markdown-code-x-scrollbar'),
            controller: _horizontalController,
            thumbVisibility: true,
            trackVisibility: true,
            thickness: 8,
            radius: const Radius.circular(4),
            interactive: true,
            scrollbarOrientation: ScrollbarOrientation.bottom,
            notificationPredicate: (notification) =>
                notification.metrics.axis == Axis.horizontal,
            child: SingleChildScrollView(
              controller: _horizontalController,
              scrollDirection: Axis.horizontal,
              padding: const EdgeInsets.fromLTRB(
                AleraTokens.space16,
                AleraTokens.space12,
                AleraTokens.space16,
                AleraTokens.space24,
              ),
              child: AleraSearchHighlightedText(
                text: widget.code,
                query: widget.searchQuery,
                style:
                    Theme.of(context).textTheme.bodySmall?.copyWith(
                      color: AleraTokens.foreground,
                      fontFamily: 'JetBrains Mono',
                    ) ??
                    const TextStyle(
                      color: AleraTokens.foreground,
                      fontFamily: 'JetBrains Mono',
                    ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class const _MarkdownViewerFileBar({
  required final String path,
  required final bool loading,
  required final bool searchOpen,
  required final VoidCallback onSearch,
  required final VoidCallback onRefresh,
  required final VoidCallback onOpenEditor,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return SizedBox(
      height: AleraTokens.sidebarHeaderHeight,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space8),
        child: Row(
          children: <Widget>[
            AleraFileIcon(
              pathOrName: path,
              kind: .file,
              size: 16,
              fallbackColor: AleraTokens.foregroundMuted,
            ),
            const SizedBox(width: AleraTokens.space8),
            Expanded(
              child: Text(
                path,
                maxLines: 1,
                overflow: .ellipsis,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  color: AleraTokens.foregroundMuted,
                  fontFamily: 'JetBrains Mono',
                ),
              ),
            ),
            const SizedBox(width: AleraTokens.space8),
            AleraIconButton(
              tooltip: searchOpen ? 'Focus preview search' : 'Search preview',
              icon: AleraIcons.search,
              iconColor: searchOpen
                  ? AleraTokens.accent
                  : AleraTokens.foregroundMuted,
              onPressed: onSearch,
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: loading ? 'Refreshing preview' : 'Refresh preview',
              icon: loading ? AleraIcons.loading : AleraIcons.refresh,
              onPressed: loading ? null : onRefresh,
            ),
            const SizedBox(width: AleraTokens.space2),
            AleraIconButton(
              tooltip: 'Open source file',
              icon: AleraIcons.code,
              onPressed: onOpenEditor,
            ),
          ],
        ),
      ),
    );
  }
}

class const _MarkdownViewerMessage({required final String message})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Center(
      child: Text(
        message,
        style: Theme.of(context).textTheme.bodyMedium
            ?.copyWith(color: AleraTokens.foregroundMuted),
      ),
    );
  }
}
