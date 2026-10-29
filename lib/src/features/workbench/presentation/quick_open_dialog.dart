import 'dart:async';
import 'dart:io';

import 'package:alera/src/app/providers.dart';
import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/forms/alera_checkbox.dart';
import 'package:alera/src/design_system/forms/alera_search_focus.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/icons/alera_file_icon.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/design_system/typography/alera_search_highlighted_text.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/features/keyboard/application/keybinding_resolver.dart';
import 'package:alera/src/features/keyboard/domain/key_chord.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
import 'package:alera/src/features/workbench/application/workspace_file_open_coordinator_provider.dart';
import 'package:alera/src/features/workbench/application/workspace_file_service.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_relative_path.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_path_drop.dart';
import 'package:alera/src/features/workbench/presentation/workspace_explorer.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_file_compare.dart';
import 'package:alera/src/rust/api/workspace_files.dart' as native;
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;

part 'quick_open_dialog_context_actions.dart';

const _quickOpenResultLimit = 50;

class const QuickOpenDialog({super.key}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<QuickOpenDialog> createState() => _QuickOpenDialogState();
}

class _QuickOpenDialogState extends ConsumerState<QuickOpenDialog> {
  late final TextEditingController _queryController;
  late final FocusNode _queryFocusNode;
  late final WorkspaceFileService _workspaceFiles;

  String? _workspaceId;
  List<native.WorkspaceQuickOpenMatch> _matches =
      const <native.WorkspaceQuickOpenMatch>[];
  bool _loading = true;
  Object? _loadError;
  int _selectedIndex = 0;
  int _workspaceGeneration = 0;
  int _searchGeneration = 0;
  final Map<String, GlobalKey> _rowKeys = <String, GlobalKey>{};
  final ScrollController _resultsScrollController = ScrollController();
  final Set<String> _contextSelectedPaths = <String>{};
  String? _contextSelectionAnchor;
  _QuickOpenContextClipboard? _contextClipboard;
  ExternalEditorKind? _pickedExternalEditorKind;
  StreamSubscription<String>? _indexRefreshSubscription;
  String _previousQuery = '';

  @override
  void initState() {
    super.initState();
    _queryController = TextEditingController();
    _queryFocusNode = FocusNode();
    _workspaceFiles = ref.read(workspaceFileServiceProvider);
    _indexRefreshSubscription = _workspaceFiles.quickOpenRefreshes.listen(
      _handleQuickOpenIndexRefreshed,
    );
    ref.listenManual<String?>(
      workbenchControllerProvider.select((state) => state.activeWorkspaceId),
      (previous, next) {
        if (previous != next) {
          unawaited(_reloadWorkspace());
        }
      },
      fireImmediately: true,
    );
  }

  @override
  void dispose() {
    _workspaceGeneration++;
    _searchGeneration++;
    _queryController.dispose();
    unawaited(_indexRefreshSubscription?.cancel());
    _queryFocusNode.dispose();
    _resultsScrollController.dispose();
    super.dispose();
  }

  Future<void> _reloadWorkspace() async {
    final workspace = ref.read(workbenchControllerProvider).activeWorkspace;
    final workspaceId = workspace?.id;
    final generation = ++_workspaceGeneration;
    ++_searchGeneration;
    final workspaceChanged = _workspaceId != workspaceId;
    _workspaceId = workspaceId;
    if (workspaceChanged) {
      _queryController.clear();
      _contextSelectedPaths.clear();
      _contextSelectionAnchor = null;
      _contextClipboard = null;
      _previousQuery = '';
    }
    if (!mounted) {
      return;
    }
    final includeGitignored = ref
        .read(workbenchControllerProvider)
        .viewPrefs
        .quickOpenIncludeGitignored;
    final excludedDirectories = _excludedDirectories();
    final cached = workspace == null
        ? null
        : _workspaceFiles.peekQuickOpenMatches(
            workspacePath: workspace.path,
            excludedDirectories: excludedDirectories,
            query: _queryController.text,
            includeGitignored: includeGitignored,
            limit: _quickOpenResultLimit,
          );
    _resetResultsScroll();
    setState(() {
      _matches = cached ?? const <native.WorkspaceQuickOpenMatch>[];
      _selectedIndex = 0;
      _loadError = null;
      _loading = workspace != null && cached == null;
      _rowKeys.clear();
    });
    if (workspace == null) {
      return;
    }
    final searchGeneration = ++_searchGeneration;
    await _searchWorkspace(
      workspacePath: workspace.path,
      excludedDirectories: excludedDirectories,
      workspaceGeneration: generation,
      searchGeneration: searchGeneration,
      query: _queryController.text,
      includeGitignored: includeGitignored,
    );
  }

  Future<void> _searchWorkspace({
    required String workspacePath,
    required List<String> excludedDirectories,
    required int workspaceGeneration,
    required int searchGeneration,
    required String query,
    required bool includeGitignored,
  }) async {
    try {
      final matches = await _workspaceFiles.searchQuickOpenCached(
        workspacePath: workspacePath,
        excludedDirectories: excludedDirectories,
        query: query,
        includeGitignored: includeGitignored,
        limit: _quickOpenResultLimit,
      );
      if (!_isCurrentSearch(workspaceGeneration, searchGeneration)) {
        return;
      }
      setState(() {
        _matches = matches;
        _selectedIndex = 0;
        _contextSelectedPaths.removeWhere(
          (path) => !matches.any((match) => match.relativePath == path),
        );
        _loading = false;
        _loadError = null;
      });
    } catch (error) {
      if (!_isCurrentSearch(workspaceGeneration, searchGeneration)) {
        return;
      }
      setState(() {
        // A failed background refresh should not hide an already available
        // index. Keep the cached results usable until the next search.
        _loadError = _matches.isEmpty ? error : null;
        _loading = false;
      });
    }
  }

  String _errorDescription(Object error) {
    if (error is native.WorkspaceFileError) {
      return error.context.trim().isNotEmpty ? error.context : error.kind.name;
    }
    return error.toString();
  }

  bool _isCurrentSearch(int workspaceGeneration, int searchGeneration) {
    return mounted &&
        workspaceGeneration == _workspaceGeneration &&
        searchGeneration == _searchGeneration;
  }

  List<String> _excludedDirectories() =>
      ref.read(settingsControllerProvider).editor.quickOpenExcludedDirectories;

  void _updateQuery(String query) {
    if (_previousQuery != query) {
      _previousQuery = query;
      _contextSelectedPaths.clear();
      _contextSelectionAnchor = null;
    }
    final workspace = ref.read(workbenchControllerProvider).activeWorkspace;
    final workspaceGeneration = _workspaceGeneration;
    final searchGeneration = ++_searchGeneration;
    final includeGitignored = ref
        .read(workbenchControllerProvider)
        .viewPrefs
        .quickOpenIncludeGitignored;
    final excludedDirectories = _excludedDirectories();
    final cached = workspace == null
        ? null
        : _workspaceFiles.peekQuickOpenMatches(
            workspacePath: workspace.path,
            excludedDirectories: excludedDirectories,
            query: query,
            includeGitignored: includeGitignored,
            limit: _quickOpenResultLimit,
          );
    _resetResultsScroll();
    setState(() {
      _matches = cached ?? const <native.WorkspaceQuickOpenMatch>[];
      _selectedIndex = 0;
      _loadError = null;
      _loading = workspace != null && cached == null;
    });
    if (workspace != null) {
      unawaited(
        _searchWorkspace(
          workspacePath: workspace.path,
          excludedDirectories: excludedDirectories,
          workspaceGeneration: workspaceGeneration,
          searchGeneration: searchGeneration,
          query: query,
          includeGitignored: includeGitignored,
        ),
      );
    }
  }

  void _setIncludeGitignored(bool include) {
    final prefs = ref.read(workbenchControllerProvider).viewPrefs;
    if (prefs.quickOpenIncludeGitignored == include) {
      return;
    }
    ref
        .read(workbenchControllerProvider.notifier)
        .setQuickOpenIncludeGitignored(include);
    _updateQuery(_queryController.text);
  }

  void _resetResultsScroll() {
    if (_resultsScrollController.hasClients) {
      _resultsScrollController.jumpTo(0);
    }
  }

  void _moveSelection(int delta) {
    if (_matches.isEmpty) {
      return;
    }
    setState(() {
      _selectedIndex = (_selectedIndex + delta) % _matches.length;
      if (_selectedIndex < 0) {
        _selectedIndex += _matches.length;
      }
    });
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (!mounted || _matches.isEmpty) {
        return;
      }
      final rowContext =
          _rowKeys[_matches[_selectedIndex].relativePath]?.currentContext;
      if (rowContext != null) {
        Scrollable.ensureVisible(
          rowContext,
          alignment: 0.5,
          duration: AleraTokens.durationFast,
        );
      } else if (_resultsScrollController.hasClients) {
        final position = _resultsScrollController.position;
        final itemExtent = AleraTokens.space32;
        final itemTop = _selectedIndex * itemExtent;
        final itemBottom = itemTop + itemExtent;
        final visibleTop = position.pixels;
        final visibleBottom = visibleTop + position.viewportDimension;
        final target = itemTop < visibleTop
            ? itemTop
            : itemBottom > visibleBottom
            ? itemBottom - position.viewportDimension
            : position.pixels;
        if (target != position.pixels) {
          _resultsScrollController.animateTo(
            target.clamp(0.0, position.maxScrollExtent),
            duration: AleraTokens.durationFast,
            curve: Curves.easeOut,
          );
        }
      }
    });
  }

  void _openSelected() {
    final workspace = ref.read(workbenchControllerProvider).activeWorkspace;
    if (workspace == null || _matches.isEmpty) {
      return;
    }
    _openFile(_matches[_selectedIndex].relativePath);
  }

  void _openFile(String relativePath, {bool forceAlera = false}) {
    final workspace = ref.read(workbenchControllerProvider).activeWorkspace;
    if (workspace == null) return;
    // Opening a Ctrl+P result also selects it in the real Explorer tree.
    // Queue the reveal before closing the dialog, so an Explorer tab that
    // mounts afterward can consume the request.
    ref
        .read(workbenchControllerProvider.notifier)
        .revealInExplorer(workspace: workspace, relativePath: relativePath);
    Navigator.of(context).pop();
    if (forceAlera) {
      unawaited(
        ref
            .read(workbenchControllerProvider.notifier)
            .openFileTab(
              workspace: workspace,
              relativePath: relativePath,
              preview: false,
            ),
      );
      return;
    }
    unawaited(
      ref
          .read(workspaceFileOpenCoordinatorProvider)
          .open<WorkspaceTabRecord>(
            workspace: workspace,
            relativePath: relativePath,
            openInAlera:
                ({
                  required workspace,
                  required relativePath,
                  required preview,
                }) => ref
                    .read(workbenchControllerProvider.notifier)
                    .openFileTab(
                      workspace: workspace,
                      relativePath: relativePath,
                      preview: preview,
                    ),
          ),
    );
  }

  KeyEventResult _handleKey(FocusNode node, KeyEvent event) {
    if (event is! KeyDownEvent) {
      return KeyEventResult.ignored;
    }
    final action =
        KeybindingResolver(
          settings: ref.read(settingsControllerProvider).keyboard,
        ).resolveAction(
          event,
          KeyModifierState.fromKeyboard(HardwareKeyboard.instance),
        );
    if (action?.id == KeyboardActionId.openQuickOpen) {
      _focusQuery();
      return KeyEventResult.handled;
    }
    switch (event.logicalKey) {
      case LogicalKeyboardKey.escape:
        Navigator.of(context).pop();
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowDown:
        _moveSelection(1);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.arrowUp:
        _moveSelection(-1);
        return KeyEventResult.handled;
      case LogicalKeyboardKey.enter:
        _openSelected();
        return KeyEventResult.handled;
      default:
        return KeyEventResult.ignored;
    }
  }

  void _focusQuery() {
    aleraFocusSearchInput(
      focusNode: _queryFocusNode,
      controller: _queryController,
      canFocus: () => mounted && (ModalRoute.of(context)?.isCurrent ?? true),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final workspace = ref.watch(
      workbenchControllerProvider.select((state) => state.activeWorkspace),
    );
    final includeGitignored = ref.watch(
      workbenchControllerProvider.select(
        (state) => state.viewPrefs.quickOpenIncludeGitignored,
      ),
    );
    final externalEditor = ref.watch(resolvedExternalEditorProvider).value;
    final installedExternalEditors =
        ref.watch(installedExternalEditorsProvider).value ??
        const <ExternalEditorSpec>[];
    return Focus(
      onKeyEvent: _handleKey,
      child: AleraDialog(
        maxWidth: AleraTokens.dialogWideWidth,
        maxHeight: AleraTokens.dialogMaxHeight,
        child: SizedBox(
          width: AleraTokens.dialogWideWidth,
          height: AleraTokens.dialogMaxHeight,
          child: Padding(
            padding: const EdgeInsets.all(AleraTokens.space20),
            child: Column(
              crossAxisAlignment: .stretch,
              children: <Widget>[
                Row(
                  children: <Widget>[
                    Text(
                      context.tr('Quick Open'),
                      style: theme.textTheme.titleMedium,
                    ),
                    const SizedBox(width: AleraTokens.space12),
                    if (workspace != null)
                      Expanded(
                        child: Text(
                          workspace.name,
                          maxLines: 1,
                          overflow: .ellipsis,
                          style: theme.textTheme.bodySmall?.copyWith(
                            color: AleraTokens.foregroundMuted,
                          ),
                        ),
                      ),
                  ],
                ),
                const SizedBox(height: AleraTokens.space16),
                AleraTextField(
                  key: const ValueKey<String>('quick-open-query-input'),
                  controller: _queryController,
                  focusNode: _queryFocusNode,
                  autofocus: true,
                  hintText: 'Search workspace files',
                  prefixIcon: AleraIcons.search,
                  onChanged: _updateQuery,
                  onSubmitted: (_) => _openSelected(),
                ),
                const SizedBox(height: AleraTokens.space8),
                Align(
                  alignment: Alignment.centerLeft,
                  child: AleraCheckbox(
                    value: includeGitignored,
                    onChanged: _setIncludeGitignored,
                    enabled: workspace != null,
                    label: context.tr('Include Git-ignored files'),
                  ),
                ),
                const SizedBox(height: AleraTokens.space12),
                Expanded(
                  child: _buildResults(
                    theme,
                    externalEditor: externalEditor,
                    installedExternalEditors: installedExternalEditors,
                  ),
                ),
                const Divider(height: AleraTokens.space20),
                Text(
                  context.tr(
                    'Use Up and Down to navigate, Enter to open, or Escape to close.',
                  ),
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: AleraTokens.foregroundFaint,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _buildResults(
    ThemeData theme, {
    required ExternalEditorSpec? externalEditor,
    required List<ExternalEditorSpec> installedExternalEditors,
  }) {
    if (_loading) {
      return Center(
        child: Column(
          mainAxisSize: .min,
          children: <Widget>[
            const CircularProgressIndicator(),
            const SizedBox(height: AleraTokens.space12),
            Text(context.tr('Loading workspace files...')),
          ],
        ),
      );
    }
    if (_loadError != null) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(AleraTokens.space16),
          child: Column(
            mainAxisSize: .min,
            children: <Widget>[
              Icon(AleraIcons.error, color: AleraTokens.error),
              const SizedBox(height: AleraTokens.space8),
              Text(
                context.tr('Could not load workspace files.'),
                style: theme.textTheme.bodyMedium,
                textAlign: .center,
              ),
              const SizedBox(height: AleraTokens.space4),
              Text(
                _errorDescription(_loadError!),
                maxLines: 3,
                overflow: .ellipsis,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: AleraTokens.foregroundMuted,
                ),
                textAlign: .center,
              ),
              const SizedBox(height: AleraTokens.space12),
              TextButton(
                onPressed: () => unawaited(_reloadWorkspace()),
                child: Text(context.tr('Retry')),
              ),
            ],
          ),
        ),
      );
    }
    if (_matches.isEmpty) {
      final query = _queryController.text.trim();
      return Center(
        child: Text(
          query.isEmpty
              ? context.tr('No files are available in this workspace.')
              : '${context.tr('No files match')} "$query".',
          style: theme.textTheme.bodyMedium?.copyWith(
            color: AleraTokens.foregroundMuted,
          ),
          textAlign: .center,
        ),
      );
    }
    return ListView.builder(
      key: const ValueKey<String>('quick-open-results'),
      controller: _resultsScrollController,
      itemExtent: AleraTokens.space32,
      padding: EdgeInsets.zero,
      itemCount: _matches.length,
      itemBuilder: (context, index) {
        final match = _matches[index];
        final selected = index == _selectedIndex;
        final rowKey = _rowKeys.putIfAbsent(
          match.relativePath,
          () => GlobalKey(),
        );
        final row = Material(
          key: rowKey,
          color: selected || _contextSelectedPaths.contains(match.relativePath)
              ? AleraTokens.accentSubtle
              : null,
          child: InkWell(
            onTap: () {
              if (_handleQuickOpenModifiedSelection(index)) return;
              setState(() {
                _selectedIndex = index;
                _contextSelectedPaths
                  ..clear()
                  ..add(match.relativePath);
                _contextSelectionAnchor = match.relativePath;
              });
              _openSelected();
            },
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: AleraTokens.space12,
                vertical: AleraTokens.space8,
              ),
              child: Row(
                children: <Widget>[
                  AleraFileIcon(
                    pathOrName: match.relativePath,
                    kind: .file,
                    size: AleraTokens.space16,
                  ),
                  const SizedBox(width: AleraTokens.space12),
                  Expanded(
                    child: AleraSearchHighlightedText(
                      text: match.relativePath,
                      query: _queryController.text,
                      fuzzy: true,
                      maxLines: 1,
                      overflow: .ellipsis,
                      style: theme.textTheme.bodyMedium,
                    ),
                  ),
                ],
              ),
            ),
          ),
        );
        return WorkspaceFileSearchResultContextMenu(
          fileManagerLabel: ref
              .read(workspaceFolderOpenerProvider)
              .fileManagerLabel,
          externalEditor: externalEditor,
          installedExternalEditors: installedExternalEditors,
          onExternalEditorPicked: (kind) => _pickedExternalEditorKind = kind,
          onMenuOpening: () => _selectQuickOpenContextTarget(index),
          onAction: (menuContext, action) => _handleQuickOpenContextAction(
            menuContext,
            match.relativePath,
            action,
          ),
          child: row,
        );
      },
    );
  }
}
