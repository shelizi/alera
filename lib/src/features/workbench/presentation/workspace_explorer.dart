import 'dart:async';
import 'dart:io';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
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
  bool _filterLoading = false;
  Object? _filterError;
  int _filterGeneration = 0;
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
      _filterLoading = false;
      _filterError = null;
      _filterGeneration++;
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
    _saveWorkspaceUiState(widget.workspace);
    unawaited(_stopNativeWatcher());
    unawaited(_quickOpenRefreshSubscription?.cancel());
    _filterController.dispose();
    _filterFocusNode.dispose();
    _explorerFocusNode.dispose();
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    _listenForRevealRequest();
    final resolvedEditor = ref.watch(resolvedExternalEditorProvider).value;
    final installedEditors =
        ref.watch(installedExternalEditorsProvider).value ??
        const <ExternalEditorSpec>[];
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
                      ? _buildFilterResults()
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
                            contextMenuDelegate: _ExplorerMenuDelegate(
                              fileManagerLabel: _folderOpener.fileManagerLabel,
                              canOpenInAlera: widget.onOpenFileInAlera != null,
                              canFocusSourceControlFolders:
                                  widget.onFocusSourceControlFolder != null,
                              externalEditor: resolvedEditor,
                              installedExternalEditors: installedEditors,
                              onExternalEditorPicked: (kind) =>
                                  _pickedExternalEditorKind = kind,
                              isFocusedSourceControlRoot: (node) {
                                return _entryByNodeId[node.id]?.relativePath ==
                                    widget.focusedSourceControlRoot;
                              },
                              onMenuOpening: _suppressBackgroundMenuOnce,
                              onAction: _handleMenuAction,
                            ),
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
    return DragTarget<_ExplorerDragData>(
      onWillAcceptWithDetails: (details) =>
          !_nativeDragHandedOff && _canDrop(details.data, entry.relativePath),
      onAcceptWithDetails: (details) => unawaited(
        _moveSelectedEntries(details.data.relativePaths, entry.relativePath),
      ),
      builder: (context, _, _) => TerminalPathDraggable<_ExplorerDragData>(
        onDragStarted: () => _nativeDragHandedOff = false,
        onDragUpdate: (details) =>
            unawaited(_maybeHandOffNativeDrag(details, entry)),
        data: _ExplorerDragData(
          relativePaths: _selectedPathsForEntry(entry),
          absolutePaths: _selectedPathsForEntry(entry)
              .map((path) => _absolutePath(path))
              .toList(growable: false),
        ),
        feedback: Material(
          color: Colors.transparent,
          child: SizedBox(width: AleraTokens.sidebarDefaultWidth, child: child),
        ),
        child: child,
      ),
    );
  }

  Future<void> _maybeHandOffNativeDrag(
    DragUpdateDetails details,
    native.WorkspaceFileEntry entry,
  ) async {
    if (!Platform.isWindows ||
        _checkingNativeDrag ||
        _nativeDragHandedOff ||
        !mounted)
      return;
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
        : <String, tree.TreeNode>{
            for (final node in projection.nodes)
              node.id: tree.TreeNode(
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
              ),
          };
    return tree.TreeData(
      nodes: Map<String, tree.TreeNode>.unmodifiableOf(nodes),
      rootId: _rootId,
      visibleRootId: _rootId,
      omitContainerRowAtRoot: true,
    );
  }

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
      return true;
    }
    if (toggle) {
      _controller.selection.toggle(node.id);
      _selectedRelativePath = _entryByNodeId[node.id]?.relativePath;
      return true;
    }
    return false;
  }

  List<String> _selectedPathsForEntry(native.WorkspaceFileEntry entry) {
    final clickedNodeId = _nodeIdForRelativePath(entry.relativePath);
    if (clickedNodeId == null ||
        !_controller.selection.isSelected(clickedNodeId)) {
      return <String>[entry.relativePath];
    }
    final selected = <String>{};
    for (final node in _controller.visibleNodes) {
      if (!_controller.selection.isSelected(node.id)) continue;
      final selectedEntry = _entryByNodeId[node.id];
      if (selectedEntry != null) selected.add(selectedEntry.relativePath);
    }
    // If a folder and its child are both selected, transfer the folder once.
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

  void _handleCopyShortcut() => unawaited(_copySelection(cut: false));

  void _handleCutShortcut() => unawaited(_copySelection(cut: true));

  Future<void> _copySelection({required bool cut}) async {
    for (final node in _controller.visibleNodes) {
      if (!_controller.selection.isSelected(node.id)) continue;
      final entry = _entryByNodeId[node.id];
      if (entry != null) {
        await _copyToClipboard(entry, cut: cut);
        return;
      }
    }
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
    final generation = ++_filterGeneration;
    if (query.isEmpty) {
      setState(() {
        _filterMatches = const <native.WorkspaceQuickOpenMatch>[];
        _filterLoading = false;
        _filterError = null;
      });
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
      );
      if (!mounted || generation != _filterGeneration) {
        return;
      }
      setState(() {
        _filterMatches = matches;
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

  Widget _buildFilterResults() {
    final query = _filterController.text.trim();
    if (_filterLoading && _filterMatches.isEmpty) {
      return const Center(child: CircularProgressIndicator());
    }
    if (_filterError != null && _filterMatches.isEmpty) {
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
    final matches = _filterMatches;
    if (matches.isEmpty) {
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
      itemCount: matches.length,
      itemBuilder: (context, index) {
        final path = matches[index].relativePath;
        return InkWell(
          onTap: () => widget.onOpenFile(path),
          child: Padding(
            padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space8),
            child: Row(
              children: <Widget>[
                AleraFileIcon(
                  pathOrName: path,
                  kind: AleraFileIconKind.file,
                  size: 15,
                  fallbackColor: AleraTokens.foregroundMuted,
                ),
                const SizedBox(width: AleraTokens.space6),
                Expanded(
                  child: AleraSearchHighlightedText(
                    text: path,
                    query: query,
                    maxLines: 1,
                    overflow: .ellipsis,
                    style: Theme.of(context).textTheme.bodySmall
                        ?.copyWith(color: AleraTokens.foregroundMuted),
                  ),
                ),
              ],
            ),
          ),
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
