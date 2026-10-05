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
import 'package:alera/src/features/workbench/application/workspace_folder_opener.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_relative_path.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_file_compare.dart';
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
  bool _loading = true;
  bool _suppressNextBackgroundMenu = false;
  String? _lastOpenedFilePath;
  DateTime? _lastOpenedFileAt;
  ExternalEditorKind? _pickedExternalEditorKind;
  final TextEditingController _filterController = TextEditingController();
  final FocusNode _filterFocusNode = FocusNode(debugLabel: 'explorer-filter');
  final FocusNode _explorerFocusNode = FocusNode(
    debugLabel: 'workspace-explorer',
  );
  bool _filterVisible = false;
  PageStorageBucket? _pageStorageBucket;
  final Set<String> _expandedDirectoryPaths = <String>{};
  String? _selectedRelativePath;

  @override
  void initState() {
    super.initState();
    _workspaceFiles = ref.read(workspaceFileServiceProvider);
    _editorSessions = ref.read(editorSessionRegistryProvider);
    _folderOpener = ref.read(workspaceFolderOpenerProvider);
    _gitBackend = ref.read(gitBackendProvider);
    _controller = tree.DirectoryTreeController(
      data: _buildTreeData(),
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
      _filterVisible = false;
      _expandedDirectoryPaths.clear();
      _selectedRelativePath = null;
      _resetExplorerProjection();
      _controller.dispose();
      _controller = tree.DirectoryTreeController(
        data: _buildTreeData(),
        flattenStrategy: const _AleraFlattenStrategy(),
      );
      unawaited(_restartExplorer());
    } else if (oldWidget.mode != widget.mode ||
        oldWidget.showHiddenFiles != widget.showHiddenFiles) {
      unawaited(_reloadForModeChange());
    }
  }

  @override
  void dispose() {
    _saveWorkspaceUiState(widget.workspace);
    unawaited(_stopNativeWatcher());
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
          filterVisible: _isFilterVisible,
          onToggleFilter: _toggleFilterVisibility,
          onRefresh: () => unawaited(_reloadRoot()),
          onCollapseAll: _collapseAll,
          onToggleMode: _toggleMode,
          onToggleHiddenFiles: _toggleHiddenFiles,
          onSaveAll: () => unawaited(_saveAllEditors()),
          onNewFile: () => unawaited(_createEntry(directory: false)),
          onNewFolder: () => unawaited(_createEntry(directory: true)),
        ),
        if (_isFilterVisible)
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
            },
            child: Focus(
              focusNode: _explorerFocusNode,
              child: Listener(
                behavior: .translucent,
                onPointerDown: (_) => _explorerFocusNode.requestFocus(),
                child: _ExplorerBackgroundMenu(
                  shouldSuppress: _consumeBackgroundMenuSuppression,
                  onAction: _handleBackgroundAction,
                  child: _loading && _controller.visibleNodes.isEmpty
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
          _canDrop(details.data, entry.relativePath),
      onAcceptWithDetails: (details) =>
          unawaited(_moveEntry(details.data.relativePath, entry.relativePath)),
      builder: (context, _, _) => TerminalPathDraggable<_ExplorerDragData>(
        data: _ExplorerDragData(
          relativePath: entry.relativePath,
          absolutePath: _absolutePath(entry.relativePath),
        ),
        feedback: Material(
          color: Colors.transparent,
          child: SizedBox(width: AleraTokens.sidebarDefaultWidth, child: child),
        ),
        child: child,
      ),
    );
  }

  Future<void> _reloadRoot() async {
    setState(() => _loading = true);
    try {
      _resetExplorerProjection();
      await _refreshGitStatusSnapshot();
      await _syncWatchedDirectories();
      await _loadDirectory('');
      if (!mounted) {
        return;
      }
      _rebuildTree();
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

  void _collapseAll() {
    _controller.expansions.collapseAll();
    _expandedDirectoryPaths.clear();
  }

  Object _workspaceUiStateIdentifier(Workspace workspace) =>
      'workspace-explorer-ui:${workspace.id}:${workspace.path}';

  void _saveWorkspaceUiState(Workspace workspace) {
    final bucket = _pageStorageBucket;
    if (bucket == null) {
      return;
    }
    bucket.writeState(
      context,
      _WorkspaceExplorerUiState(
        loadedDirectories: _childrenByDirectory.keys
            .where((path) => path.isNotEmpty)
            .toSet(),
        expandedDirectories: Set<String>.from(_expandedDirectoryPaths),
        selectedRelativePath: _selectedRelativePath,
      ),
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

  bool get _isFilterVisible =>
      _filterVisible || _filterController.text.trim().isNotEmpty;

  void _toggleFilterVisibility() {
    setState(() {
      _filterVisible = !_isFilterVisible;
    });
    if (_isFilterVisible) {
      // autofocus is skipped while the terminal or editor holds focus, so the
      // field asks for it once it has been mounted.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && _isFilterVisible) _filterFocusNode.requestFocus();
      });
    }
  }

  void _onFilterChanged(String value) {
    _controller.filterQuery = value;
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
