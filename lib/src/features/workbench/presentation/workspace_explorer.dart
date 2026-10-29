import 'dart:async';
import 'dart:io';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/forms/alera_search_focus.dart';
import 'package:alera/src/design_system/forms/alera_search_field.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/icons/alera_file_icon.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_submenu_entry.dart';
import 'package:alera/src/design_system/typography/alera_search_highlighted_text.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/features/external_editor/presentation/external_editor_menu_entries.dart';
import 'package:alera/src/features/workbench/application/workspace_explorer_reveal.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/application/windows_file_drag.dart';
import 'package:alera/src/features/workbench/application/workspace_folder_opener.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_relative_path.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_file_compare.dart';
import 'package:alera/src/features/workbench/presentation/terminal_path_drop.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:alera/src/shared/infra/files/path_identity.dart';
import 'package:alera/src/shared/infra/git/git_backend.dart';
import 'package:alera/src/shared/infra/git/git_explorer_status.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_directory_tree/flutter_directory_tree.dart' as tree;
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;

part 'workspace_explorer_actions.dart';
part 'workspace_explorer_refresh.dart';
part 'workspace_explorer_widgets.dart';

bool _isDirectoryEntry(native.WorkspaceFileEntry? entry) =>
    entry?.kind.name == 'directory';

/// Reuses the Explorer's exact file context menu on file-search surfaces such
/// as Ctrl+P Quick Open. The action names are mapped from the Explorer enum,
/// so the action list never diverges between search and the file tree.
class const WorkspaceFileSearchResultContextMenu({
  super.key,
  required final Widget child,
  required final String fileManagerLabel,
  required final ExternalEditorSpec? externalEditor,
  required final List<ExternalEditorSpec> installedExternalEditors,
  required final ValueChanged<ExternalEditorKind> onExternalEditorPicked,
  required final VoidCallback onMenuOpening,
  required final Future<void> Function(BuildContext, String) onAction,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final menu = _ExplorerMenuDelegate(
      fileManagerLabel: fileManagerLabel,
      canOpenInAlera: true,
      canFocusSourceControlFolders: false,
      isFocusedSourceControlRoot: (_) => false,
      externalEditor: externalEditor,
      installedExternalEditors: installedExternalEditors,
      onExternalEditorPicked: onExternalEditorPicked,
      onMenuOpening: () {},
      onAction: (_, _, _) async {},
    );
    return menu.wrapFilteredFileWithMenu(
      context,
      child,
      onMenuOpeningForFile: onMenuOpening,
      onEntryAction: (menuContext, action) =>
          onAction(menuContext, action.name),
    );
  }
}

class _WorkspaceExplorerUiState {
  const _WorkspaceExplorerUiState({
    required this.loadedDirectories,
    required this.expandedDirectories,
    required this.selectedRelativePath,
  });

  final Set<String> loadedDirectories;
  final Set<String> expandedDirectories;
  final String? selectedRelativePath;
}

class const WorkspaceExplorer({
  super.key,
  required final Workspace workspace,
  required final WorkspaceExplorerMode mode,
  required final ValueChanged<WorkspaceExplorerMode> onModeChanged,
  required final bool showHiddenFiles,
  required final ValueChanged<bool> onShowHiddenFilesChanged,
  required final ValueChanged<String> onOpenFile,
  final ValueChanged<String>? onOpenFilePermanently,
  final ValueChanged<String>? onOpenFileInAlera,
  required final Future<void> Function(
    String oldRelativePath,
    String newRelativePath,
  )
  onPathMoved,
  final String? focusedSourceControlRoot,
  final Future<bool> Function(String relativePath)? onFocusSourceControlFolder,
  final VoidCallback? onClearSourceControlRoot,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceExplorer> createState() => _WorkspaceExplorerState();
}

class _WorkspaceExplorerState extends ConsumerState<WorkspaceExplorer> {
  static const String _rootId = 'workspace-root';
  static const String _placeholderPrefix = '__alera_placeholder__:';
  static const int _filterResultLimit = 200;

  late tree.DirectoryTreeController _controller;
  final Map<String, List<native.WorkspaceFileEntry>> _childrenByDirectory =
      <String, List<native.WorkspaceFileEntry>>{};
  final Map<String, native.WorkspaceFileEntry> _entryByPath =
      <String, native.WorkspaceFileEntry>{};
  final Map<String, native.WorkspaceFileEntry> _entryByNodeId =
      <String, native.WorkspaceFileEntry>{};
  native.WorkspaceExplorerTreeProjection? _projection;
  // Retain Dart TreeNode objects for unchanged IDs across structural native
  // Delta updates; only changed/removed nodes require new objects.
  final Map<String, tree.TreeNode> _treeNodesById = <String, tree.TreeNode>{};
  // Structural revisions change DirectoryTreeController.data; metadata
  // revisions repaint only mounted rows (DirectoryTreeView caches ListView).
  int _treeStructureRevision = 0;
  final ValueNotifier<int> _rowMetadataRevision = ValueNotifier<int>(0);
  native.WorkspaceExplorerProjectionHandle? _nativeProjectionHandle;
  Future<native.WorkspaceExplorerProjectionHandle>? _nativeProjectionStarting;
  Future<void> _nativeProjectionQueue = Future<void>.value();
  int _nativeProjectionGeneration = 0;
  bool _nativeProjectionDisabled = false;
  native.WorkspaceExplorerWatcherHandle? _watcherHandle;
  StreamSubscription<native.WorkspaceExplorerWatchBatch>? _watchSubscription;
  bool _watchRefreshInFlight = false;
  final Set<String> _pendingWatchedDirectories = <String>{};
  Set<String>? _lastSyncedWatchedDirectories;
  late final WorkspaceFileService _workspaceFiles;
  late final EditorSessionRegistry _editorSessions;
  late final WorkspaceFolderOpener _folderOpener;
  late final GitBackend _gitBackend;
  GitExplorerStatusSnapshot _gitStatusSnapshot = const .empty();
  _ExplorerClipboard? _clipboard;
  final WindowsFileDrag _nativeFileDrag = const WindowsFileDrag();
  bool _checkingNativeDrag = false;
  bool _nativeDragHandedOff = false;
  bool _loading = true;
  bool _suppressNextBackgroundMenu = false;
  String? _lastOpenedFilePath;
  DateTime? _lastOpenedFileAt;
  ExternalEditorKind? _pickedExternalEditorKind;
  final TextEditingController _filterController = TextEditingController();
  final FocusNode _filterFocusNode = FocusNode(debugLabel: 'explorer-filter');
  StreamSubscription<String>? _quickOpenRefreshSubscription;
  final FocusNode _explorerFocusNode = FocusNode(
    debugLabel: 'workspace-explorer',
  );
  List<native.WorkspaceQuickOpenMatch> _filterMatches =
      const <native.WorkspaceQuickOpenMatch>[];
  // One search projection is reused by rendered rows, selection and drag.
  List<native.WorkspaceFileEntry> _filterVisibleCache =
      const <native.WorkspaceFileEntry>[];
  String? _filterVisibleCacheQuery;
  List<native.WorkspaceQuickOpenMatch>? _filterVisibleCacheMatches;
  int _filterVisibleCacheEntryRevision = -1;
  int _entryProjectionRevision = 0;
  final Set<String> _filterSelectedPaths = <String>{};
  String? _filterSelectionAnchor;
  String _activeFilterQuery = '';
  bool _filterLoading = false;
  Object? _filterError;
  int _filterGeneration = 0;
  int _treeRevealGeneration = 0;
  PageStorageBucket? _pageStorageBucket;
  final Set<String> _expandedDirectoryPaths = <String>{};
  String? _selectedRelativePath;

  @override
  void initState() {
    super.initState();
    _workspaceFiles = ref.read(workspaceFileServiceProvider);
    _quickOpenRefreshSubscription = _workspaceFiles.quickOpenRefreshes.listen(
      _onQuickOpenIndexRefreshed,
    );
    _editorSessions = ref.read(editorSessionRegistryProvider);
    _folderOpener = ref.read(workspaceFolderOpenerProvider);
    _gitBackend = ref.read(gitBackendProvider);
    _controller = tree.DirectoryTreeController(
      data: _buildTreeData(),
      selection: tree.SelectionController(mode: tree.SelectionMode.multi),
      flattenStrategy: const _AleraFlattenStrategy(),
    );
    unawaited(_bootstrapExplorer());
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _pageStorageBucket = PageStorage.maybeOf(context);
  }

  @override
  void didUpdateWidget(covariant WorkspaceExplorer oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.workspace.id != widget.workspace.id ||
        oldWidget.workspace.path != widget.workspace.path) {
      _saveWorkspaceUiState(oldWidget.workspace);
      _loading = true;
      _clipboard = null;
      _filterController.clear();
      _filterMatches = const <native.WorkspaceQuickOpenMatch>[];
      _filterSelectedPaths.clear();
      _filterSelectionAnchor = null;
      _activeFilterQuery = '';
      _filterLoading = false;
      _filterError = null;
      _filterGeneration++;
      _treeRevealGeneration++;
      _expandedDirectoryPaths.clear();
      _selectedRelativePath = null;
      _resetExplorerProjection();
      _controller.dispose();
      _controller = tree.DirectoryTreeController(
        data: _buildTreeData(),
        selection: tree.SelectionController(mode: tree.SelectionMode.multi),
        flattenStrategy: const _AleraFlattenStrategy(),
      );
      unawaited(_restartExplorer());
    } else if (oldWidget.workspace.branch != widget.workspace.branch) {
      unawaited(_reloadForModeChange());
    } else if (oldWidget.mode != widget.mode ||
        oldWidget.showHiddenFiles != widget.showHiddenFiles) {
      unawaited(_reloadForModeChange());
    }
  }

  @override
  void dispose() {
    _treeRevealGeneration++;
    _saveWorkspaceUiState(widget.workspace);
    unawaited(_stopNativeWatcher());
    _disposeNativeProjectionSession();
    unawaited(_quickOpenRefreshSubscription?.cancel());
    _filterController.dispose();
    _filterFocusNode.dispose();
    _explorerFocusNode.dispose();
    _controller.dispose();
    _rowMetadataRevision.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _listenForRevealRequest();
    final resolvedEditor = ref.watch(resolvedExternalEditorProvider).value;
    final installedEditors =
        ref.watch(installedExternalEditorsProvider).value ??
        const <ExternalEditorSpec>[];
    final entryMenuDelegate = _ExplorerMenuDelegate(
      fileManagerLabel: _folderOpener.fileManagerLabel,
      canOpenInAlera: widget.onOpenFileInAlera != null,
      canFocusSourceControlFolders: widget.onFocusSourceControlFolder != null,
      externalEditor: resolvedEditor,
      installedExternalEditors: installedEditors,
      onExternalEditorPicked: (kind) => _pickedExternalEditorKind = kind,
      isFocusedSourceControlRoot: (node) =>
          _entryByNodeId[node.id]?.relativePath ==
          widget.focusedSourceControlRoot,
      onMenuOpening: _suppressBackgroundMenuOnce,
      onAction: _handleMenuAction,
    );
    return CallbackShortcuts(
      bindings: <ShortcutActivator, VoidCallback>{
        SingleActivator(
          LogicalKeyboardKey.keyF,
          control: Theme.of(context).platform != TargetPlatform.macOS,
          meta: Theme.of(context).platform == TargetPlatform.macOS,
        ): _focusFilter,
      },
      child: _buildExplorerContents(entryMenuDelegate),
    );
  }

  void _focusFilter() {
    aleraFocusSearchInput(
      focusNode: _filterFocusNode,
      controller: _filterController,
      canFocus: () => mounted && _filterFocusNode.canRequestFocus,
    );
  }

  Widget _buildExplorerContents(_ExplorerMenuDelegate entryMenuDelegate) {
    return Column(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        _ExplorerToolbar(
          title: widget.workspace.name,
          mode: widget.mode,
          showHiddenFiles: widget.showHiddenFiles,
          showHiddenToggle: defaultTargetPlatform == TargetPlatform.windows,
          loading: _loading,
          onRefresh: () {
            _workspaceFiles.scheduleQuickOpenRefresh(
              workspacePath: widget.workspace.path,
              debounce: Duration.zero,
            );
            unawaited(_reloadRoot());
          },
          onCollapseAll: _collapseAll,
          onToggleMode: _toggleMode,
          onToggleHiddenFiles: _toggleHiddenFiles,
          onSaveAll: () => unawaited(_saveAllEditors()),
          onNewFile: () => unawaited(_createEntry(directory: false)),
          onNewFolder: () => unawaited(_createEntry(directory: true)),
        ),
        Padding(
          padding: const EdgeInsets.fromLTRB(
            AleraTokens.space8,
            0,
            AleraTokens.space8,
            AleraTokens.space8,
          ),
          child: AleraSearchField(
            key: const ValueKey<String>('workspace-explorer-filter-input'),
            controller: _filterController,
            focusNode: _filterFocusNode,
            hintText: 'Filter files...',
            dense: true,
            onChanged: _onFilterChanged,
          ),
        ),
        const Divider(height: 1, color: AleraTokens.borderSubtle),
        Expanded(
          child: CallbackShortcuts(
            bindings: <ShortcutActivator, VoidCallback>{
              const SingleActivator(LogicalKeyboardKey.keyA, control: true):
                  _handleSelectAllShortcut,
              const SingleActivator(LogicalKeyboardKey.keyA, meta: true):
                  _handleSelectAllShortcut,
              const SingleActivator(LogicalKeyboardKey.keyV, control: true):
                  _handlePasteShortcut,
              const SingleActivator(LogicalKeyboardKey.keyC, control: true):
                  _handleCopyShortcut,
              const SingleActivator(LogicalKeyboardKey.keyX, control: true):
                  _handleCutShortcut,
              const SingleActivator(LogicalKeyboardKey.keyV, meta: true):
                  _handlePasteShortcut,
              const SingleActivator(LogicalKeyboardKey.keyC, meta: true):
                  _handleCopyShortcut,
              const SingleActivator(LogicalKeyboardKey.keyX, meta: true):
                  _handleCutShortcut,
            },
            child: Focus(
              focusNode: _explorerFocusNode,
              child: Listener(
                behavior: .translucent,
                onPointerDown: (_) => _explorerFocusNode.requestFocus(),
                child: _ExplorerBackgroundMenu(
                  shouldSuppress: _consumeBackgroundMenuSuppression,
                  onAction: _handleBackgroundAction,
                  child: _filterController.text.trim().isNotEmpty
                      ? _buildFilterResults(entryMenuDelegate)
                      : _loading && _controller.visibleNodes.isEmpty
                      ? const Center(child: CircularProgressIndicator())
                      : tree.DirectoryTreeTheme(
                          data: const tree.DirectoryTreeThemeData(
                            rowHeight: AleraTokens.space32,
                            indent: AleraTokens.space16,
                            selectionColor: AleraTokens.surfaceElevated,
                            focusColor: AleraTokens.surfaceElevated,
                            hoverColor: AleraTokens.surface,
                            roundedCorners: false,
                          ),
                          child: tree.DirectoryTreeView(
                            key: PageStorageKey<String>(
                              'workspace-explorer:${widget.workspace.id}',
                            ),
                            controller: _controller,
                            padding: const .symmetric(
                              vertical: AleraTokens.space4,
                            ),
                            expanderSize: AleraTokens.space24,
                            expanderGap: 0,
                            expanderBuilder: _buildExpander,
                            contextMenuDelegate: entryMenuDelegate,
                            nodeBuilder: _buildNode,
                          ),
                        ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildExpander(
    BuildContext context,
    tree.VisibleNode node,
    bool expanded,
    VoidCallback _,
  ) {
    return InkWell(
      onTap: () => unawaited(_toggleDirectory(node)),
      // InkWell defaults to adaptiveClickable, which is the basic arrow off the
      // web, so the hand cursor has to be requested explicitly here.
      mouseCursor: SystemMouseCursors.click,
      child: Icon(
        expanded ? AleraIcons.chevronDown : AleraIcons.chevronRight,
        size: 16,
        color: AleraTokens.foregroundMuted,
      ),
    );
  }

  Widget _buildNode(
    BuildContext context,
    tree.VisibleNode node,
    tree.NodeVisualState state,
  ) {
    // flutter_directory_tree caches its ListView between controller updates.
    // Listening in each mounted row refreshes Git badges/metadata while
    // retaining the existing TreeData, flattening, selection and scroll state.
    return ValueListenableBuilder<int>(
      valueListenable: _rowMetadataRevision,
      builder: (context, _, _) =>
          _buildNodeForCurrentMetadata(context, node, state),
    );
  }

  Widget _buildNodeForCurrentMetadata(
    BuildContext context,
    tree.VisibleNode node,
    tree.NodeVisualState state,
  ) {
    final entry = _entryByNodeId[node.id];
    final selected = _controller.selection.isSelected(node.id);
    final child = _ExplorerRow(
      name: node.name,
      filterQuery: _filterController.text,
      entry: entry,
      expanded: state.isExpanded,
      selected: selected,
      sourceControlRoot:
          entry != null &&
          entry.relativePath == widget.focusedSourceControlRoot,
      onTap: () => unawaited(_handlePrimaryTap(node)),
    );
    if (entry == null) {
      return child;
    }
    return _wrapExplorerDragTarget(entry: entry, child: child);
  }

  /// Both tree nodes and search rows are projections of the same file entry.
  /// Keep in-app drop targets, Windows Shell drag handoff and multi-selection
  /// payload construction in one place for both presentations.
  Widget _wrapExplorerDragTarget({
    required native.WorkspaceFileEntry entry,
    required Widget child,
  }) {
    return DragTarget<_ExplorerDragData>(
      onWillAcceptWithDetails: (details) =>
          !_nativeDragHandedOff &&
          _isDirectoryEntry(entry) &&
          _canDrop(details.data, entry.relativePath),
      onAcceptWithDetails: (details) => unawaited(
        _moveSelectedEntries(details.data.relativePaths, entry.relativePath),
      ),
      builder: (context, _, _) => TerminalPathDraggable<_ExplorerDragData>(
        onDragStarted: () => _nativeDragHandedOff = false,
        onDragUpdate: (details) =>
            unawaited(_maybeHandOffNativeDrag(details, entry)),
        data: _explorerDragData(entry),
        feedback: Material(
          color: Colors.transparent,
          child: SizedBox(width: AleraTokens.sidebarDefaultWidth, child: child),
        ),
        child: child,
      ),
    );
  }

  _ExplorerDragData _explorerDragData(native.WorkspaceFileEntry entry) {
    final relativePaths = _selectedPathsForEntry(entry);
    return _ExplorerDragData(
      relativePaths: relativePaths,
      absolutePaths: relativePaths.map(_absolutePath).toList(growable: false),
    );
  }

  Future<void> _maybeHandOffNativeDrag(
    DragUpdateDetails details,
    native.WorkspaceFileEntry entry,
  ) async {
    if (!Platform.isWindows ||
        _checkingNativeDrag ||
        _nativeDragHandedOff ||
        !mounted) {
      return;
    }
    _checkingNativeDrag = true;
    try {
      final view = View.of(context);
      final logicalWidth = view.physicalSize.width / view.devicePixelRatio;
      final logicalHeight = view.physicalSize.height / view.devicePixelRatio;
      final pointer = details.globalPosition;
      final outsideView =
          pointer.dx < 0 ||
          pointer.dy < 0 ||
          pointer.dx >= logicalWidth ||
          pointer.dy >= logicalHeight;
      if (!outsideView && !await _nativeFileDrag.cursorOutsideWindow()) return;
      if (!mounted || _nativeDragHandedOff) return;
      final paths = _selectedPathsForEntry(entry);
      if (paths.isEmpty) return;
      _nativeDragHandedOff = true;
      final effect = await _nativeFileDrag.startDrag(<String>[
        for (final path in paths) userVisibleWorkspacePath(_absolutePath(path)),
      ]);
      if (effect == WindowsFileDragEffect.move && mounted) {
        // The Windows Shell moves the files. Only refresh our tree; never
        // delete the source after the native drop finishes.
        for (final parent in paths.map(_parentPath).toSet()) {
          await _refreshDirectory(parent);
          if (!mounted) return;
        }
      }
    } on MissingPluginException {
      // Embedded test runners or older portable builds can lack this channel.
    } catch (error) {
      if (mounted) _showError(error);
    } finally {
      _checkingNativeDrag = false;
    }
  }

  Future<void> _reloadRoot() async {
    final snapshot = _captureWorkspaceUiState();
    setState(() => _loading = true);
    try {
      _resetExplorerProjection();
      await _refreshGitStatusSnapshot();
      await _syncWatchedDirectories();
      await _loadDirectory('');
      if (!mounted) {
        return;
      }
      await _restoreWorkspaceUiState(snapshot: snapshot);
    } catch (error) {
      if (mounted) {
        _expandedDirectoryPaths.clear();
        _selectedRelativePath = null;
        _rebuildTree(tryPreserveState: false);
      }
      _showError(error);
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  Future<void> _reloadForModeChange() async {
    final loadedDirectories =
        _childrenByDirectory.keys
            .where((relativePath) => relativePath.isNotEmpty)
            .toList(growable: false)
          ..sort(_compareDirectoryDepth);
    setState(() => _loading = true);
    try {
      _resetExplorerProjection();
      await _refreshGitStatusSnapshot();
      await _syncWatchedDirectories();
      await _loadDirectory('');
      for (final relativePath in loadedDirectories) {
        if (!mounted) {
          return;
        }
        if (!_isDirectoryEntry(_entryByPath[relativePath])) {
          continue;
        }
        await _loadDirectory(relativePath);
      }
      if (!mounted) {
        return;
      }
      _rebuildTree();
      if (_filterController.text.trim().isNotEmpty) {
        _onFilterChanged(_filterController.text);
      }
    } catch (error) {
      if (mounted) {
        _rebuildTree(tryPreserveState: false);
      }
      _showError(error);
    } finally {
      if (mounted) {
        setState(() => _loading = false);
      }
    }
  }

  Future<void> _loadDirectory(String relativePath) async {
    final rawChildren = await _workspaceFiles.listChildren(
      workspacePath: widget.workspace.path,
      relativePath: relativePath,
      hideIgnored: widget.mode == WorkspaceExplorerMode.hideIgnored,
      hideHidden:
          defaultTargetPlatform == TargetPlatform.windows &&
          !widget.showHiddenFiles,
    );
    final children = _workspaceFiles.applyGitStatusSnapshot(
      rawChildren,
      _gitStatusSnapshot,
    );
    if (!mounted) {
      return;
    }
    await _replaceDirectoryChildren(relativePath, children);
  }

  Future<void> _refreshGitStatusSnapshot() async {
    try {
      _gitStatusSnapshot = await _gitBackend.explorerStatusSnapshot(
        widget.workspace.path,
      );
    } catch (_) {
      _gitStatusSnapshot = const GitExplorerStatusSnapshot.empty();
    }
  }

  void _rebuildTree({bool tryPreserveState = true}) {
    if (!mounted) {
      return;
    }
    final next = _buildTreeData();
    _controller.rebuild(next, tryPreserveState: tryPreserveState);
  }

  int _compareDirectoryDepth(String left, String right) {
    final depthComparison = _directoryDepth(left)
        .compareTo(_directoryDepth(right));
    if (depthComparison != 0) {
      return depthComparison;
    }
    return left.compareTo(right);
  }

  int _directoryDepth(String relativePath) {
    if (relativePath.isEmpty) {
      return 0;
    }
    return relativePath.split('/').length;
  }

  tree.TreeData _buildTreeData() {
    final projection = _projection;
    final nodes = projection == null
        ? <String, tree.TreeNode>{
            _rootId: tree.TreeNode(
              id: _rootId,
              name: widget.workspace.name,
              type: tree.NodeType.root,
              parentId: '',
              virtualPath: '/',
              sourcePath: widget.workspace.path,
              isExpanded: true,
            ),
          }
        : _treeNodesById;
    return tree.TreeData(
      nodes: Map<String, tree.TreeNode>.unmodifiableOf(nodes),
      rootId: _rootId,
      visibleRootId: _rootId,
      omitContainerRowAtRoot: true,
    );
  }

  tree.TreeNode _toDartExplorerTreeNode(
    native.WorkspaceExplorerTreeNode node,
  ) => tree.TreeNode(
    id: node.id,
    name: node.name,
    type: _treeNodeType(node.kind),
    parentId: node.parentId,
    virtualPath: node.virtualPath,
    sourcePath: node.sourcePath,
    entryId: node.entryId,
    childIds: node.childIds,
    isExpanded: node.isExpanded,
    isVirtual: node.isVirtual,
  );

  Future<void> _handlePrimaryTap(tree.VisibleNode node) async {
    if (_handleModifiedSelection(node)) {
      return;
    }
    _select(node);
    final entry = _entryByNodeId[node.id];
    if (_isDirectoryEntry(entry)) {
      await _toggleDirectory(node);
      return;
    }
    if (entry != null) {
      final path = entry.relativePath;
      widget.onOpenFile(path);
      final onOpenPermanently = widget.onOpenFilePermanently;
      final now = DateTime.now();
      if (onOpenPermanently != null &&
          _lastOpenedFilePath == path &&
          _lastOpenedFileAt != null &&
          now.difference(_lastOpenedFileAt!) <= kDoubleTapTimeout) {
        onOpenPermanently(path);
        _lastOpenedFilePath = null;
        _lastOpenedFileAt = null;
      } else {
        _lastOpenedFilePath = path;
        _lastOpenedFileAt = now;
      }
    }
  }

  Future<void> _toggleDirectory(tree.VisibleNode node) async {
    final entry = _entryByNodeId[node.id];
    if (!_isDirectoryEntry(entry)) {
      if (!mounted) {
        return;
      }
      _controller.toggle(node.id);
      return;
    }
    final directory = entry!;
    if (!_childrenByDirectory.containsKey(directory.relativePath)) {
      await _loadDirectory(directory.relativePath);
      if (!mounted) {
        return;
      }
      _rebuildTree();
    }
    _controller.toggle(node.id);
    final relativePath = directory.relativePath;
    if (!_expandedDirectoryPaths.add(relativePath)) {
      _expandedDirectoryPaths.remove(relativePath);
    }
  }

  void _select(tree.VisibleNode node) {
    _controller.selection.selectOnly(node.id);
    _selectedRelativePath = _entryByNodeId[node.id]?.relativePath;
  }

  bool _handleModifiedSelection(tree.VisibleNode node) {
    final keyboard = HardwareKeyboard.instance;
    final toggle = keyboard.isControlPressed || keyboard.isMetaPressed;
    if (keyboard.isShiftPressed) {
      final anchor = _entryByNodeId.entries
          .where((entry) => entry.value.relativePath == _selectedRelativePath)
          .map((entry) => entry.key)
          .firstOrNull;
      if (anchor != null) {
        _controller.selectRange(anchorId: anchor, toId: node.id);
      } else {
        _select(node);
      }
      setState(() {});
      return true;
    }
    if (toggle) {
      _controller.selection.toggle(node.id);
      _selectedRelativePath = _entryByNodeId[node.id]?.relativePath;
      setState(() {});
      return true;
    }
    return false;
  }

  bool get _isFiltering => _filterController.text.trim().isNotEmpty;

  /// Selection is determined by the active projection, but all file
  /// operations consume the same normalized list of relative paths.
  List<String> _selectedVisiblePaths() {
    if (_isFiltering) {
      return <String>[
        for (final entry in _visibleFilteredEntries())
          if (_filterSelectedPaths.contains(entry.relativePath))
            entry.relativePath,
      ];
    }
    return <String>[
      for (final node in _controller.visibleNodes)
        if (_controller.selection.isSelected(node.id) &&
            _entryByNodeId[node.id] != null)
          _entryByNodeId[node.id]!.relativePath,
    ];
  }

  List<String> _selectedPathsForEntry(native.WorkspaceFileEntry entry) {
    final selected = _selectedVisiblePaths();
    if (!selected.contains(entry.relativePath)) {
      return <String>[entry.relativePath];
    }
    return _topLevelSelectedPaths(selected);
  }

  /// A selected parent folder subsumes any selected descendant for clipboard,
  /// in-app drag/drop, and native Windows Shell drag/drop alike.
  List<String> _topLevelSelectedPaths(Iterable<String> paths) {
    final selected = paths.toSet();
    return selected
        .where((path) {
          var ancestor = _parentPath(path);
          while (ancestor.isNotEmpty) {
            if (selected.contains(ancestor)) return false;
            ancestor = _parentPath(ancestor);
          }
          return true;
        })
        .toList(growable: false);
  }

  void _handleSelectAllShortcut() {
    if (_isFiltering) {
      final entries = _visibleFilteredEntries();
      if (entries.isEmpty) return;
      _filterSelectedPaths
        ..clear()
        ..addAll(entries.map((entry) => entry.relativePath));
      _filterSelectionAnchor = entries.first.relativePath;
      setState(() {});
      return;
    }
    final nodes = _controller.visibleNodes
        .where((node) => _entryByNodeId.containsKey(node.id))
        .toList(growable: false);
    if (nodes.isEmpty) return;
    _controller.selection.selectOnly(nodes.first.id);
    for (final node in nodes.skip(1)) {
      _controller.selection.toggle(node.id);
    }
    _selectedRelativePath = _entryByNodeId[nodes.first.id]?.relativePath;
    setState(() {});
  }

  void _handleCopyShortcut() => unawaited(_copySelection(cut: false));

  void _handleCutShortcut() => unawaited(_copySelection(cut: true));

  Future<void> _copySelection({required bool cut}) async {
    final paths = _selectedVisiblePaths();
    if (paths.isEmpty) return;
    await _copyToClipboard(_visibleEntryForPath(paths.first), cut: cut);
  }

  void _collapseAll() {
    _controller.expansions.collapseAll();
    _expandedDirectoryPaths.clear();
  }

  Object _workspaceUiStateIdentifier(Workspace workspace) =>
      'workspace-explorer-ui:${workspace.id}:${workspace.path}';

  _WorkspaceExplorerUiState _captureWorkspaceUiState() =>
      _WorkspaceExplorerUiState(
        loadedDirectories: _childrenByDirectory.keys
            .where((path) => path.isNotEmpty)
            .toSet(),
        expandedDirectories: Set<String>.from(_expandedDirectoryPaths),
        selectedRelativePath: _selectedRelativePath,
      );

  void _saveWorkspaceUiState(Workspace workspace) {
    final bucket = _pageStorageBucket;
    if (bucket == null) {
      return;
    }
    bucket.writeState(
      context,
      _captureWorkspaceUiState(),
      identifier: _workspaceUiStateIdentifier(workspace),
    );
  }

  _WorkspaceExplorerUiState? _readWorkspaceUiState() {
    final bucket = _pageStorageBucket;
    return bucket?.readState(
      context,
      identifier: _workspaceUiStateIdentifier(widget.workspace),
    ) as _WorkspaceExplorerUiState?;
  }

  void _handlePasteShortcut() {
    unawaited(_paste(_pasteTargetDirectory()));
  }

  void _onFilterChanged(String value, {bool refreshIndex = true}) {
    _controller.filterQuery = '';
    final query = value.trim();
    if (_activeFilterQuery != query) {
      _activeFilterQuery = query;
      _filterSelectedPaths.clear();
      _filterSelectionAnchor = null;
    }
    final generation = ++_filterGeneration;
    if (query.isEmpty) {
      setState(() {
        _filterMatches = const <native.WorkspaceQuickOpenMatch>[];
        _filterLoading = false;
        _filterError = null;
      });
      // Reattach the real tree and scroll to the file selected while
      // filtering, without disrupting search-result multi-selection.
      if (_selectedRelativePath case final target?) {
        final revealGeneration = _treeRevealGeneration;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (!mounted ||
              _isFiltering ||
              _selectedRelativePath != target ||
              revealGeneration != _treeRevealGeneration) {
            return;
          }
          unawaited(_revealPathInTree(target));
        });
      }
      return;
    }
    if (refreshIndex) {
      _workspaceFiles.scheduleQuickOpenFilterRefresh(
        workspacePath: widget.workspace.path,
      );
    }
    final excludedDirectories = ref
        .read(settingsControllerProvider)
        .editor
        .quickOpenExcludedDirectories;
    final includeGitignored = widget.mode == WorkspaceExplorerMode.showAll;
    final cached = _workspaceFiles.peekQuickOpenMatches(
      workspacePath: widget.workspace.path,
      excludedDirectories: excludedDirectories,
      query: query,
      includeGitignored: includeGitignored,
      limit: _filterResultLimit,
      includeDirectories: true,
    );
    setState(() {
      _filterMatches = cached ?? const <native.WorkspaceQuickOpenMatch>[];
      _filterLoading = cached == null;
      _filterError = null;
    });
    unawaited(
      _searchFilter(
        generation: generation,
        query: query,
        excludedDirectories: excludedDirectories,
        includeGitignored: includeGitignored,
      ),
    );
  }

  void _onQuickOpenIndexRefreshed(String workspacePath) {
    if (!mounted ||
        workspacePath != widget.workspace.path ||
        _filterController.text.trim().isEmpty) {
      return;
    }
    _onFilterChanged(_filterController.text, refreshIndex: false);
  }

  Future<void> _searchFilter({
    required int generation,
    required String query,
    required List<String> excludedDirectories,
    required bool includeGitignored,
  }) async {
    try {
      final matches = await _workspaceFiles.searchQuickOpenCached(
        workspacePath: widget.workspace.path,
        excludedDirectories: excludedDirectories,
        query: query,
        includeGitignored: includeGitignored,
        limit: _filterResultLimit,
        includeDirectories: true,
      );
      if (!mounted || generation != _filterGeneration) {
        return;
      }
      setState(() {
        _filterMatches = matches;
        final visible = _visibleFilteredEntries()
            .map((entry) => entry.relativePath)
            .toSet();
        _filterSelectedPaths.removeWhere((path) => !visible.contains(path));
        if (!_filterSelectedPaths.contains(_filterSelectionAnchor)) {
          _filterSelectionAnchor = null;
        }
        _filterLoading = false;
        _filterError = null;
      });
    } catch (error) {
      if (!mounted || generation != _filterGeneration) {
        return;
      }
      setState(() {
        _filterLoading = false;
        _filterError = error;
      });
    }
  }

  native.WorkspaceFileEntry _unloadedEntry(
    String path, {
    bool directory = false,
  }) => native.WorkspaceFileEntry(
    relativePath: path,
    name: path.split('/').last,
    kind: directory
        ? native.WorkspaceFileKind.directory
        : native.WorkspaceFileKind.file,
    size: BigInt.zero,
    modifiedMillis: 0,
    contentToken: '',
    isIgnored: false,
    isHidden: false,
    isSymlink: false,
    isProtected: false,
    hasChildrenHint: false,
  );

  /// The shared native index supplies both files and directories, including
  /// deeply nested empty folders not yet loaded by the Explorer tree.
  /// Loaded/ancestor folder matches supplement the bounded native result set.
  List<native.WorkspaceFileEntry> _visibleFilteredEntries() {
    final query = _filterController.text.trim().toLowerCase();
    if (query.isEmpty) return const <native.WorkspaceFileEntry>[];
    if (_filterVisibleCacheQuery == query &&
        identical(_filterVisibleCacheMatches, _filterMatches) &&
        _filterVisibleCacheEntryRevision == _entryProjectionRevision) {
      return _filterVisibleCache;
    }
    final folders = <String>{
      for (final entry in _entryByPath.values)
        if (_isDirectoryEntry(entry) &&
            aleraFuzzyTextContains(entry.relativePath, query))
          entry.relativePath,
      for (final match in _filterMatches)
        if (match.isDirectory) match.relativePath,
    };
    for (final match in _filterMatches) {
      var parent = _parentPath(match.relativePath);
      while (parent.isNotEmpty) {
        if (aleraFuzzyTextContains(parent, query)) folders.add(parent);
        parent = _parentPath(parent);
      }
    }
    final sortedFolders = folders.toList()..sort();
    final entries = <native.WorkspaceFileEntry>[
      for (final path in sortedFolders)
        _entryByPath[path] ?? _unloadedEntry(path, directory: true),
      for (final match in _filterMatches)
        if (!folders.contains(match.relativePath))
          _entryByPath[match.relativePath] ??
              _unloadedEntry(match.relativePath, directory: match.isDirectory),
    ];
    _filterVisibleCacheQuery = query;
    _filterVisibleCacheMatches = _filterMatches;
    _filterVisibleCacheEntryRevision = _entryProjectionRevision;
    return _filterVisibleCache = entries;
  }

  native.WorkspaceFileEntry _visibleEntryForPath(String path) =>
      _entryByPath[path] ??
      _visibleFilteredEntries()
          .where((entry) => entry.relativePath == path)
          .firstOrNull ??
      _unloadedEntry(path);

  void _handleFilteredContextMenuSelection(String path) {
    // Match normal file-tree semantics: keep the existing multi-selection only
    // when right-clicking one of its members; otherwise target just this file.
    if (!_filterSelectedPaths.contains(path)) {
      _filterSelectedPaths
        ..clear()
        ..add(path);
    }
    _filterSelectionAnchor = path;
    setState(() {});
  }

  void _handleFilteredPrimaryTap(String path) {
    final keyboard = HardwareKeyboard.instance;
    if (keyboard.isShiftPressed && _filterSelectionAnchor != null) {
      final entries = _visibleFilteredEntries();
      final first = entries.indexWhere(
        (entry) => entry.relativePath == _filterSelectionAnchor,
      );
      final last = entries.indexWhere((entry) => entry.relativePath == path);
      if (first >= 0 && last >= 0) {
        _filterSelectedPaths
          ..clear()
          ..addAll(
            entries
                .skip(first < last ? first : last)
                .take((first - last).abs() + 1)
                .map((entry) => entry.relativePath),
          );
        setState(() {});
        return;
      }
    }
    if (keyboard.isControlPressed || keyboard.isMetaPressed) {
      if (!_filterSelectedPaths.add(path)) {
        _filterSelectedPaths.remove(path);
      }
      _filterSelectionAnchor = path;
      setState(() {});
      return;
    }
    _filterSelectedPaths
      ..clear()
      ..add(path);
    _filterSelectionAnchor = path;
    setState(() {});
    if (_isDirectoryEntry(_visibleEntryForPath(path))) {
      // A folder is never treated as a text file by the search projection.
      // Like the ordinary tree, reveal its actual node for navigation.
      unawaited(_openFilteredDirectory(path));
    } else {
      widget.onOpenFile(path);
      // Keep the filtered list available for Ctrl/Shift multi-selection,
      // clipboard and native drag while revealing in the backing tree.
      unawaited(_revealPathInTree(path));
    }
  }

  Future<void> _openFilteredDirectory(String path) async {
    await _revealPathInTree(path, clearFilter: true, expandDirectory: true);
  }

  Widget _buildFilterResults(_ExplorerMenuDelegate entryMenuDelegate) {
    final query = _filterController.text.trim();
    final entries = _visibleFilteredEntries();
    if (_filterLoading && entries.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_filterError != null && entries.isEmpty) {
      return Center(
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: <Widget>[
            Text(
              context.tr('Could not filter workspace files.'),
              style: Theme.of(context).textTheme.bodySmall,
            ),
            if (_filterError case final native.WorkspaceFileError error)
              Padding(
                padding: const EdgeInsets.symmetric(
                  vertical: AleraTokens.space8,
                ),
                child: Text(
                  error.context,
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  textAlign: TextAlign.center,
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ),
            TextButton(
              onPressed: () => _onFilterChanged(_filterController.text),
              child: Text(context.tr('Retry')),
            ),
          ],
        ),
      );
    }
    if (entries.isEmpty) {
      return Center(
        child: Text(
          context.tr('No matching files.'),
          style: Theme.of(context).textTheme.bodySmall,
        ),
      );
    }
    return ListView.builder(
      padding: const EdgeInsets.symmetric(vertical: AleraTokens.space4),
      itemExtent: AleraTokens.space32,
      itemCount: entries.length,
      itemBuilder: (context, index) {
        final path = entries[index].relativePath;
        // Repaint filtered metadata through the same revision signal as tree
        // nodes so Git badges, folder kinds and status don't become stale.
        return ValueListenableBuilder<int>(
          valueListenable: _rowMetadataRevision,
          builder: (context, _, _) {
            final entry = _visibleEntryForPath(path);
            final row = _ExplorerRow(
              name: path,
              filterQuery: query,
              entry: entry,
              expanded: false,
              selected: _filterSelectedPaths.contains(path),
              sourceControlRoot: path == widget.focusedSourceControlRoot,
              leadingPadding: AleraTokens.space8,
              onTap: () => _handleFilteredPrimaryTap(path),
            );
            return entryMenuDelegate.wrapFilteredEntryWithMenu(
              context,
              _wrapExplorerDragTarget(entry: entry, child: row),
              entry: entry,
              isFocusedRoot: path == widget.focusedSourceControlRoot,
              onMenuOpeningForEntry: () =>
                  _handleFilteredContextMenuSelection(path),
              onEntryAction: (menuContext, action) => _handleEntryMenuAction(
                menuContext,
                entry: entry,
                action: action,
              ),
            );
          },
        );
      },
    );
  }

  void _setClipboard(_ExplorerClipboard? clipboard) {
    if (!mounted) {
      return;
    }
    setState(() => _clipboard = clipboard);
  }

  void _suppressBackgroundMenuOnce() {
    _suppressNextBackgroundMenu = true;
  }

  bool _consumeBackgroundMenuSuppression() {
    if (!_suppressNextBackgroundMenu) {
      return false;
    }
    _suppressNextBackgroundMenu = false;
    return true;
  }

  tree.NodeType _treeNodeType(native.WorkspaceExplorerTreeNodeKind kind) {
    return switch (kind) {
      native.WorkspaceExplorerTreeNodeKind.root => tree.NodeType.root,
      native.WorkspaceExplorerTreeNodeKind.folder => tree.NodeType.folder,
      native.WorkspaceExplorerTreeNodeKind.file => tree.NodeType.file,
    };
  }
}

class const _AleraFlattenStrategy() extends tree.FlattenStrategy {
  static const tree.DefaultFlattenStrategy _delegate =
      tree.DefaultFlattenStrategy();

  @override
  List<tree.VisibleNode> flatten({
    required tree.TreeData data,
    required Set<String> expandedIds,
    String? filterQuery,
  }) {
    return _delegate
        .flatten(data: data, expandedIds: expandedIds, filterQuery: filterQuery)
        .where(
          (node) =>
              !node.id.startsWith(_WorkspaceExplorerState._placeholderPrefix),
        )
        .toList(growable: false);
  }
}
