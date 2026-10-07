import 'dart:async';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/forms/alera_search_field.dart';
import 'package:alera/src/design_system/forms/alera_text_actions_scope.dart';
import 'package:alera/src/design_system/icons/alera_file_icon.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_confirm_dialog.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_entry.dart';
import 'package:alera/src/design_system/menus/alera_dropdown_submenu_entry.dart';
import 'package:alera/src/design_system/typography/alera_search_highlighted_text.dart';
import 'package:alera/src/design_system/surfaces/hover_container.dart';
import 'package:alera/src/features/external_editor/application/external_editor_providers.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/features/external_editor/presentation/external_editor_menu_entries.dart';
import 'package:alera/src/features/ai_dictation/presentation/ai_dictation_field_overlay.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_providers.dart';
import 'package:alera/src/features/ai_assist/application/ai_assist_service.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/projects/application/project_providers.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_git_commit_compare_cache.dart';
import 'package:alera/src/features/workbench/application/workspace_git_history_loader.dart';
import 'package:alera/src/features/workbench/application/workspace_service.dart';
import 'package:alera/src/features/workbench/application/workspace_source_control_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_submodule_status_provider.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/presentation/terminal_path_drop.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_file_compare.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_ignore.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_actions.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_graph.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:alera/src/shared/infra/git/git_history_graph.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter/services.dart';

part 'workspace_git_diff_panel_types.dart';
part 'workspace_git_diff_panel_groups.dart';
part 'workspace_git_diff_panel_grouping_cache.dart';
part 'workspace_git_diff_panel_rows.dart';
part 'workspace_git_diff_panel_amend_dialog.dart';
part 'workspace_git_diff_panel_stash_dialog.dart';
part 'workspace_git_diff_panel_toolbar.dart';
part 'workspace_git_diff_panel_commit_message_field.dart';
part 'workspace_git_diff_panel_tree.dart';
part 'workspace_git_diff_panel_submodules.dart';
part 'workspace_git_history_panel.dart';
part 'workspace_git_history_panel_row.dart';
part 'workspace_git_history_panel_actions.dart';
part 'workspace_git_history_panel_files.dart';
part 'workspace_git_diff_panel_preview_opening.dart';
part 'workspace_git_diff_panel_context_menu.dart';
part 'workspace_git_diff_panel_inline_actions.dart';
part 'workspace_git_diff_panel_navigation.dart';

class _GitDiffPanelUiState {
  const _GitDiffPanelUiState({
    required this.collapsedSections,
    required this.collapsedTreeNodes,
    required this.expandedSubmodules,
    required this.historyCollapsed,
  });

  final Set<String> collapsedSections;
  final Set<String> collapsedTreeNodes;
  final Set<String> expandedSubmodules;
  final bool historyCollapsed;
}

class const WorkspaceGitDiffPanel({
  super.key,
  required final Workspace workspace,
  required final WorkspaceSourceControlScope sourceControlScope,
  required final GitDiffViewMode viewMode,
  required final ValueChanged<GitDiffViewMode> onViewModeChanged,
  required final GitDiffGroupMode groupMode,
  required final ValueChanged<GitDiffGroupMode> onGroupModeChanged,
  required final OpenGitDiffTabCallback onOpenGitDiff,
  required final OpenGitCommitDiffTabCallback onOpenGitCommitDiff,
  final Future<void> Function(String branch)? onSwitchBranch,
  final ValueChanged<String>? onOpenFile,
  final ValueChanged<String>? onRevealInExplorer,
  final VoidCallback? onClearSourceControlRoot,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceGitDiffPanel> createState() =>
      _WorkspaceGitDiffPanelState();
}

class _WorkspaceGitDiffPanelState extends ConsumerState<WorkspaceGitDiffPanel> {
  final TextEditingController _messageController = TextEditingController();
  final FocusNode _messageFocusNode = FocusNode();
  final TextEditingController _filterController = TextEditingController();
  final FocusNode _filterFocusNode = FocusNode(debugLabel: 'git-diff-filter');
  final Set<String> _collapsedSections = <String>{};
  final Set<String> _collapsedTreeNodes = <String>{};
  final Set<String> _expandedSubmodules = <String>{};
  late final AiAssistService _aiAssistService;
  bool _filterVisible = false;
  bool _generatingCommitMessage = false;
  bool _historyCollapsed = true;
  late final WorkspaceGitHistoryLoader _historyLoader;
  late final WorkspaceGitCommitCompareCache _commitCompareCache;
  int _commitMessageGenerationId = 0;
  final _GitDiffPreviewOpening _previewOpening = _GitDiffPreviewOpening();
  // Status objects are immutable and replaced by the controller on every
  // load, so identity-keyed caches let collapse toggles and unrelated
  // rebuilds skip the sort + tree rebuild.
  GitStatusResult? _cachedStatusSource;
  String _cachedFilterQuery = '';
  GitStatusResult _cachedFilteredStatus = const GitStatusResult(
    entries: <GitChangeEntry>[],
  );
  GitStatusResult? _cachedGroupsSource;
  GitDiffGroupMode? _cachedGroupsMode;
  List<GitChangeGroup> _cachedGroups = const <GitChangeGroup>[];
  int _unifiedGroupsGeneration = 0;
  GitStatusResult? _pendingUnifiedSource;
  List<GitChangeGroup>? _cachedCollapsibleGroups;
  Set<String> _cachedCollapsibleKeys = const <String>{};
  ExternalEditorKind? _pickedExternalEditorKind;
  PageStorageBucket? _pageStorageBucket;
  bool _restoredInitialPanelUiState = false;

  void _applyUnifiedGroups(
    GitStatusResult status,
    List<GitChangeGroup> groups,
  ) {
    setState(() {
      _pendingUnifiedSource = null;
      _cachedGroupsSource = status;
      _cachedGroupsMode = GitDiffGroupMode.unified;
      _cachedGroups = groups;
    });
  }

  @override
  void initState() {
    super.initState();
    _aiAssistService = ref.read(aiAssistServiceProvider);
    final backend = ref.read(gitBackendProvider);
    _historyLoader = WorkspaceGitHistoryLoader(
      backend: backend,
      scopePath: widget.sourceControlScope.path,
      onChanged: _onGitHistoryChanged,
    );
    _commitCompareCache = WorkspaceGitCommitCompareCache(
      backend: backend,
      scopePath: widget.sourceControlScope.path,
    );
  }

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _pageStorageBucket = PageStorage.maybeOf(context);
    if (_restoredInitialPanelUiState) {
      return;
    }
    _restorePanelUiState(widget.sourceControlScope.path);
    _restoredInitialPanelUiState = true;
  }

  @override
  void didUpdateWidget(covariant WorkspaceGitDiffPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.sourceControlScope.path != widget.sourceControlScope.path) {
      _savePanelUiState(oldWidget.sourceControlScope.path);
      _aiAssistService.cancel(
        oldWidget.sourceControlScope.path,
        .commitMessage,
      );
      _commitMessageGenerationId += 1;
      _messageController.clear();
      _filterController.clear();
      _collapsedSections.clear();
      _collapsedTreeNodes.clear();
      _expandedSubmodules.clear();
      _filterVisible = false;
      _generatingCommitMessage = false;
      _historyCollapsed = true;
      _restorePanelUiState(widget.sourceControlScope.path);
      _historyLoader.rebind(widget.sourceControlScope.path);
      _commitCompareCache.rebind(widget.sourceControlScope.path);
    }
  }

  @override
  void dispose() {
    _savePanelUiState(widget.sourceControlScope.path);
    _aiAssistService.cancel(widget.sourceControlScope.path, .commitMessage);
    _commitMessageGenerationId += 1;
    _historyLoader.detach();
    _messageController.dispose();
    _messageFocusNode.dispose();
    _filterController.dispose();
    _filterFocusNode.dispose();
    super.dispose();
  }

  void _setPanelState(VoidCallback fn) => setState(fn);

  Object _panelUiStateIdentifier(String scopePath) =>
      'workspace-git-diff-ui:$scopePath';

  void _savePanelUiState(String scopePath) {
    final bucket = _pageStorageBucket;
    if (bucket == null) {
      return;
    }
    bucket.writeState(
      context,
      _GitDiffPanelUiState(
        collapsedSections: Set<String>.from(_collapsedSections),
        collapsedTreeNodes: Set<String>.from(_collapsedTreeNodes),
        expandedSubmodules: Set<String>.from(_expandedSubmodules),
        historyCollapsed: _historyCollapsed,
      ),
      identifier: _panelUiStateIdentifier(scopePath),
    );
  }

  void _restorePanelUiState(String scopePath) {
    final snapshot = _pageStorageBucket?.readState(
      context,
      identifier: _panelUiStateIdentifier(scopePath),
    ) as _GitDiffPanelUiState?;
    if (snapshot == null) {
      return;
    }
    _collapsedSections
      ..clear()
      ..addAll(snapshot.collapsedSections);
    _collapsedTreeNodes
      ..clear()
      ..addAll(snapshot.collapsedTreeNodes);
    _expandedSubmodules
      ..clear()
      ..addAll(snapshot.expandedSubmodules);
    _historyCollapsed = snapshot.historyCollapsed;
  }

  @override
  Widget build(BuildContext context) {
    final sourceControlProvider = workspaceSourceControlControllerProvider(
      widget.sourceControlScope.path,
    );
    ref.listen<AsyncValue<WorkspaceSourceControlState>>(sourceControlProvider, (
      previous,
      next,
    ) {
      final previousData = previous?.asData?.value;
      final nextData = next.asData?.value;
      if (previousData == null || nextData == null || nextData.isBusy) {
        return;
      }
      _invalidateGitHistoryAfterRepositoryChange();
    });
    final state = ref.watch(sourceControlProvider);
    final aiAssistSettings = ref.watch(
      settingsControllerProvider.select((settings) => settings.aiAssist),
    );
    final resolvedEditor = ref.watch(resolvedExternalEditorProvider).value;
    final installedEditors =
        ref.watch(installedExternalEditorsProvider).value ??
        const <ExternalEditorSpec>[];
    return Column(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        _SourceControlToolbar(
          messageController: _messageController,
          messageFocusNode: _messageFocusNode,
          filterController: _filterController,
          filterFocusNode: _filterFocusNode,
          viewMode: widget.viewMode,
          groupMode: widget.groupMode,
          state: state,
          aiAssistSettings: aiAssistSettings,
          generatingCommitMessage: _generatingCommitMessage,
          allCollapsed: _allVisibleNodesCollapsed(state.asData?.value),
          filterVisible: _isFilterVisible,
          sourceControlRootLabel: widget.sourceControlScope.relativeRoot,
          onMessageChanged: () => setState(() {}),
          onGenerateCommitMessage: () => unawaited(_generateCommitMessage()),
          onCancelGenerateCommitMessage: _cancelGenerateCommitMessage,
          onFilterChanged: () => setState(() {}),
          onToggleFilter: _toggleFilterVisibility,
          onRefresh: () => unawaited(_refresh()),
          onClearSourceControlRoot: widget.onClearSourceControlRoot,
          onToggleCollapseAll: () =>
              _toggleAllVisibleNodes(state.asData?.value),
          onViewModeChanged: widget.onViewModeChanged,
          onGroupModeChanged: widget.onGroupModeChanged,
          canOpenChangesExternally: _openableChangedSourcePaths(
            state.asData?.value,
          ).isNotEmpty,
          externalEditor: resolvedEditor,
          installedExternalEditors: installedEditors,
          onExternalEditorPicked: (kind) => _pickedExternalEditorKind = kind,
          onOpenAll: () => unawaited(
            widget.onOpenGitDiff(
              scope: .all,
              gitDiffRoot: widget.sourceControlScope.relativeRoot,
            ),
          ),
          onPrimaryAction: (action) => unawaited(_runToolbarAction(action)),
          onSelectMenuAction: (action) => unawaited(_handleMenuAction(action)),
        ),
        const Divider(height: 1, color: AleraTokens.borderSubtle),
        Expanded(
          child: Column(
            children: <Widget>[
              Expanded(
                child: state.when(
                  loading: () =>
                      const Center(child: CircularProgressIndicator()),
                  error: (error, _) =>
                      _GitDiffMessage(message: _messageFor(error)),
                  data: (data) {
                    final status = _filteredStatus(data.status);
                    final entries = status.entries;
                    if (entries.isEmpty) {
                      return _GitDiffMessage(
                        message: _filterController.text.trim().isEmpty
                            ? 'No changes'
                            : 'No files match the current filter',
                      );
                    }
                    return _GitDiffGroups(
                      groups: _groupsFor(status),
                      filterQuery: _filterController.text,
                      workspacePath: widget.sourceControlScope.path,
                      viewMode: widget.viewMode,
                      busy: data.isBusy,
                      collapsedSections: _collapsedSections,
                      collapsedTreeNodes: _collapsedTreeNodes,
                      expandedSubmodules: _expandedSubmodules,
                      onToggleSection: _toggleSectionCollapsed,
                      onToggleTreeNode: _toggleTreeNodeCollapsed,
                      onToggleSubmodule: _toggleSubmodule,
                      onOpenGitDiff: _openGitDiff,
                      onOpenFile: widget.onOpenFile == null
                          ? null
                          : _openWorkspaceFile,
                      onOpenWithDefaultApplication:
                          _openWorkspaceFileWithDefaultApplication,
                      onOpenExternally: _openWorkspaceFileExternally,
                      externalEditor: resolvedEditor,
                      installedExternalEditors: installedEditors,
                      onRevealInExplorer: _revealInExplorer,
                      onViewFileHistory: _openSourceFileHistory,
                      onCompareLatestGitRevision:
                          _compareSourceFileWithLatestGitRevision,
                      onCompareGitBranch: _compareSourceFileWithBranch,
                      onAddToGitIgnore: _addToGitIgnore,
                      onStage: _stageEntry,
                      onUnstage: _unstageEntry,
                      onDiscard: _discardEntry,
                      onStageArea: _stageArea,
                      onUnstageArea: _unstageArea,
                      onDiscardArea: _discardAreaWithConfirmation,
                      onStagePath: _stage,
                      onUnstagePath: _unstage,
                      onDiscardPath: _discard,
                    );
                  },
                ),
              ),
              _GitHistoryPanel(
                state: _historyPanelState,
                collapsed: _historyCollapsed,
                onToggle: _toggleGitHistory,
                onRefresh: _refreshGitHistory,
                onLoadCommitFiles: _loadCommitFiles,
                onOpenCommit: _openCommitDiff,
                onOpenCommitFile: _openCommitFile,
                onCopyCommitText: _copyCommitText,
                onCheckoutCommit: _checkoutCommit,
                onRevertCommit: _revertCommit,
                onResetToCommit: _resetToCommit,
                onRefAction: _handleGitHistoryRefAction,
                onBoundaryAction: _handleGitHistoryBoundaryAction,
                currentUpstream: state.asData?.value.repositoryState.upstream,
                onCommitChanges: _focusCommitMessage,
                onOpenCommitGraph: () => unawaited(_openCommitGraph()),
              ),
            ],
          ),
        ),
      ],
    );
  }

  _GitHistoryPanelLoadState get _historyPanelState {
    final result = _historyLoader.result;
    if (_historyLoader.error case final error?) {
      return _GitHistoryPanelLoadState.error(
        error: _messageFor(error),
        result: result,
        loading: _historyLoader.refreshing,
      );
    }
    if (_historyLoader.loading && result == null) {
      return const _GitHistoryPanelLoadState.loading();
    }
    if (result != null) {
      return _GitHistoryPanelLoadState.ready(
        result: result,
        loading: _historyLoader.refreshing,
      );
    }
    return const _GitHistoryPanelLoadState.idle();
  }

  void _toggleGitHistory() {
    final opening = _historyCollapsed;
    setState(() => _historyCollapsed = !_historyCollapsed);
    if (opening && _historyLoader.needsLoad) {
      unawaited(_loadGitHistory());
    }
  }

  void _invalidateGitHistoryAfterMutation() {
    _commitCompareCache.clear();
    _historyLoader.markStale();
    if (_historyCollapsed) {
      return;
    }
    unawaited(_loadGitHistory());
  }

  void _invalidateGitHistoryAfterRepositoryChange() {
    if (_historyLoader.result == null && _historyLoader.inFlight == null) {
      return;
    }
    _commitCompareCache.clear();
    _historyLoader.markStale();
    if (_historyCollapsed) {
      return;
    }
    unawaited(_loadGitHistory());
  }

  Future<void> _refreshGitHistory() async {
    if (_historyCollapsed) {
      setState(() => _historyCollapsed = false);
    }
    await _loadGitHistory();
  }

  Future<void> _loadGitHistory() async {
    _commitCompareCache.clear();
    try {
      await _historyLoader.load();
    } on Object {
      // The loader stores the error and notifies the panel; the wrapper keeps
      // fire-and-forget refreshes from producing an unhandled future error.
    }
  }

  Future<List<GitCommitChangeEntry>> _loadCommitFiles(
    GitHistoryItem item,
  ) async {
    final compare = await _commitCompareFor(item);
    return compare.entries;
  }

  Future<void> _openCommitDiff(GitHistoryItem item) async {
    try {
      final compare = await _commitCompareFor(item);
      if (!mounted) {
        return;
      }
      await widget.onOpenGitCommitDiff(
        scope: .all,
        gitDiffRoot: widget.sourceControlScope.relativeRoot,
        commitOid: compare.summary.commitOid,
        parentOid: compare.summary.parentOid,
        compareRef: compare.summary.compareRef,
        subject: item.subject,
        message: item.message,
      );
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: _messageFor(error), tone: .error);
      }
    }
  }

  Future<void> _openCommitGraph() async {
    try {
      await ref
          .read(workbenchControllerProvider.notifier)
          .openGitHistoryTab(
            workspace: widget.workspace,
            gitDiffRoot: widget.sourceControlScope.relativeRoot,
          );
    } catch (error) {
      if (mounted) {
        AleraToast.show(context, message: _messageFor(error), tone: .error);
      }
    }
  }

  Future<GitCommitCompareResult> _commitCompareFor(GitHistoryItem item) async {
    return _commitCompareCache.compareFor(item.id);
  }

  Future<void> _copyCommitText(String text, String label) async {
    try {
      await Clipboard.setData(ClipboardData(text: text));
      if (!mounted) {
        return;
      }
      AleraToast.show(context, message: '$label copied', tone: .success);
    } catch (_) {
      if (!mounted) {
        return;
      }
      AleraToast.show(context, message: 'Could not copy $label', tone: .error);
    }
  }
}
