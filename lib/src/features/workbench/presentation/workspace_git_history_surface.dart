import 'dart:async';
import 'dart:io';
import 'dart:math' as math;

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/feedback/alera_empty_state.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/typography/alera_search_highlighted_text.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workspace_source_control_controller.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/domain/workbench_view_prefs.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_actions.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_history_graph.dart';
import 'package:alera/src/shared/infra/git/git_commit_ops_models.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_exception.dart';
import 'package:alera/src/shared/infra/git/git_history_graph.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

part 'workspace_git_history_commit_row.dart';
part 'workspace_git_history_surface_actions.dart';
part 'workspace_git_history_surface_perspective.dart';
part 'workspace_git_history_surface_ref_actions.dart';

const String _allBranchesPerspective = '__all_branches__';
const String _headPerspective = '__head__';
const String _headRef = 'HEAD';

/// Main-area commit graph tab. Walks every branch tip by default and pages
/// history in with offset pagination, keeping swimlanes continuous across
/// page boundaries by seeding each page with the previous row's lanes.
class const WorkspaceGitHistorySurface({
  super.key,
  required final Workspace workspace,
  required final WorkspaceTabRecord tab,
}) extends ConsumerStatefulWidget {
  static const int pageSize = 200;

  @override
  ConsumerState<WorkspaceGitHistorySurface> createState() =>
      _WorkspaceGitHistorySurfaceState();
}

class _WorkspaceGitHistorySurfaceState
    extends ConsumerState<WorkspaceGitHistorySurface> {
  final ScrollController _verticalController = ScrollController();
  final ScrollController _horizontalController = ScrollController();
  final Map<String, GitCommitCompareResult> _compareCache =
      <String, GitCommitCompareResult>{};
  String? _compareAnchorId;

  List<GitHistoryItem> _items = const <GitHistoryItem>[];
  List<GitHistoryItemViewModel> _viewModels = const <GitHistoryItemViewModel>[];
  GitHistoryProjectionContinuation _projectionContinuation =
      const GitHistoryProjectionContinuation();
  Map<String, GitHistoryGraphColorId?> _colorMap =
      const <String, GitHistoryGraphColorId?>{};
  GitHistoryItemRef? _currentRef;
  GitHistoryItemRef? _remoteRef;
  GitHistoryItemRef? _baseRef;
  bool _hasIncomingChanges = false;
  bool _hasOutgoingChanges = false;
  String? _mergeBase;
  late bool _allBranches = widget.tab.gitHistoryAllBranches;
  late String? _selectedRef = widget.tab.gitHistorySelectedRef;
  List<String> _branches = const <String>[];
  String? _currentBranch;
  bool _loading = false;
  bool _loadingMore = false;
  bool _hasMore = false;
  bool _pageFailed = false;
  String? _error;
  int _generation = 0;

  void _setSurfaceState(VoidCallback fn) => setState(fn);

  @override
  void initState() {
    super.initState();
    _verticalController.addListener(_onScroll);
    unawaited(_loadBranchPerspectives());
    _reload();
  }

  @override
  void dispose() {
    _generation += 1;
    _verticalController.dispose();
    _horizontalController.dispose();
    super.dispose();
  }

  WorkspaceSourceControlScope get _sourceControlScope {
    final root = normalizeSourceControlRootRelativePath(widget.tab.gitDiffRoot);
    if (root == null) {
      return WorkspaceSourceControlScope(
        workspaceId: widget.workspace.id,
        workspacePath: widget.workspace.path,
        path: widget.workspace.path,
      );
    }
    return WorkspaceSourceControlScope(
      workspaceId: widget.workspace.id,
      workspacePath: widget.workspace.path,
      path: sourceControlRootAbsolutePath(
        workspacePath: widget.workspace.path,
        relativeRoot: root,
      ),
      relativeRoot: root,
    );
  }

  void _onScroll() {
    if (!_verticalController.hasClients ||
        !_hasMore ||
        _loading ||
        _loadingMore ||
        _pageFailed) {
      return;
    }
    final position = _verticalController.position;
    if (position.pixels >= position.maxScrollExtent - 160) {
      unawaited(_loadMore());
    }
  }

  Future<void> _reload() async {
    final generation = ++_generation;
    setState(() {
      _items = const <GitHistoryItem>[];
      _viewModels = const <GitHistoryItemViewModel>[];
      _projectionContinuation = const GitHistoryProjectionContinuation();
      _compareAnchorId = null;
      _hasMore = false;
      _pageFailed = false;
      _error = null;
      _loading = true;
      _hasIncomingChanges = false;
      _hasOutgoingChanges = false;
      _mergeBase = null;
    });
    try {
      final result = await ref
          .read(gitBackendProvider)
          .history(
            _sourceControlScope.path,
            limit: WorkspaceGitHistorySurface.pageSize,
            baseRef: _allBranches ? null : _selectedRef,
            includeAllRefs: _allBranches,
          );
      if (!mounted || generation != _generation) {
        return;
      }
      setState(() {
        _items = result.items;
        _hasMore = result.hasMore;
        _loading = false;
        _currentRef = result.currentRef;
        _remoteRef = result.remoteRef;
        _baseRef = _selectedRef == null ? result.baseRef : null;
        _hasIncomingChanges = _selectedRef == null && result.hasIncomingChanges;
        _hasOutgoingChanges = _selectedRef == null && result.hasOutgoingChanges;
        _mergeBase = result.mergeBase;
        _colorMap = buildDefaultGitHistoryColorMap(
          currentRef: result.currentRef,
          remoteRef: result.remoteRef,
          baseRef: _selectedRef == null ? result.baseRef : null,
        );
        final projection = _buildProjection(result.items);
        _viewModels = projection.viewModels;
        _projectionContinuation = projection.continuation;
      });
      // A short first page may not fill the viewport, which means the scroll
      // listener never fires, so pull the next page eagerly instead.
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && generation == _generation) {
          _onScroll();
        }
      });
    } catch (error) {
      if (!mounted || generation != _generation) {
        return;
      }
      setState(() {
        _loading = false;
        _error = error.toString();
      });
    }
  }

  Future<void> _loadMore() async {
    if (_loadingMore || !_hasMore) {
      return;
    }
    final generation = _generation;
    setState(() {
      _loadingMore = true;
      _pageFailed = false;
    });
    try {
      final result = await ref
          .read(gitBackendProvider)
          .history(
            _sourceControlScope.path,
            limit: WorkspaceGitHistorySurface.pageSize,
            baseRef: _allBranches ? null : _selectedRef,
            includeAllRefs: _allBranches,
            offset: _items.length,
          );
      if (!mounted || generation != _generation) {
        return;
      }
      final projection = _buildProjection(
        result.items,
        continuation: _projectionContinuation,
      );
      setState(() {
        _items = <GitHistoryItem>[..._items, ...result.items];
        _viewModels = <GitHistoryItemViewModel>[
          ..._viewModels,
          ...projection.viewModels,
        ];
        _projectionContinuation = projection.continuation;
        _hasMore = result.hasMore;
        _loadingMore = false;
      });
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted && generation == _generation) {
          _onScroll();
        }
      });
    } catch (_) {
      if (!mounted || generation != _generation) {
        return;
      }
      setState(() {
        _loadingMore = false;
        _pageFailed = true;
      });
    }
  }

  GitHistoryProjectionPage _buildProjection(
    List<GitHistoryItem> items, {
    GitHistoryProjectionContinuation continuation =
        const GitHistoryProjectionContinuation(),
  }) {
    return buildGitHistoryProjectionPage(
      items,
      colorMap: _colorMap,
      currentRef: _currentRef,
      remoteRef: _remoteRef,
      baseRef: _baseRef,
      addIncomingChanges: _hasIncomingChanges,
      addOutgoingChanges: _hasOutgoingChanges,
      mergeBase: _mergeBase,
      continuation: continuation,
    );
  }

  @override
  Widget build(BuildContext context) {
    return DecoratedBox(
      decoration: const BoxDecoration(color: AleraTokens.surface),
      child: Column(
        children: <Widget>[
          _buildToolbar(context),
          const Divider(height: 1, color: AleraTokens.borderSubtle),
          Expanded(child: _buildBody(context)),
        ],
      ),
    );
  }

  Widget _buildToolbar(BuildContext context) {
    final theme = Theme.of(context);
    return SizedBox(
      height: AleraTokens.sidebarHeaderHeight,
      child: Padding(
        padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space12),
        child: Row(
          children: <Widget>[
            const Icon(
              AleraIcons.gitGraph,
              size: 16,
              color: AleraTokens.foregroundMuted,
            ),
            const SizedBox(width: AleraTokens.space8),
            Text(
              'Commits',
              style: theme.textTheme.titleSmall?.copyWith(
                fontWeight: .w600,
                color: AleraTokens.foreground,
              ),
            ),
            if (_items.isNotEmpty) ...<Widget>[
              const SizedBox(width: AleraTokens.space8),
              Text(
                _hasMore ? '${_items.length}+' : '${_items.length}',
                style: theme.textTheme.labelSmall?.copyWith(
                  color: AleraTokens.foregroundFaint,
                ),
              ),
            ],
            const SizedBox(width: AleraTokens.space16),
            _buildBranchPerspectiveMenu(theme),
            const Spacer(),
            if (_loading || _loadingMore)
              const Padding(
                padding: EdgeInsets.only(right: AleraTokens.space8),
                child: SizedBox(
                  width: 12,
                  height: 12,
                  child: CircularProgressIndicator(strokeWidth: 2),
                ),
              ),
            AleraIconButton(
              tooltip: 'Refresh Commits',
              icon: AleraIcons.refresh,
              onPressed: _loading ? null : () => unawaited(_reload()),
            ),
          ],
        ),
      ),
    );
  }

  Widget _buildBody(BuildContext context) {
    if (_error != null && _items.isEmpty) {
      return AleraEmptyState(message: _error!);
    }
    if (_viewModels.isEmpty) {
      if (_loading) {
        return const Center(
          child: SizedBox(
            width: 16,
            height: 16,
            child: CircularProgressIndicator(strokeWidth: 2),
          ),
        );
      }
      return const AleraEmptyState(message: 'No commits yet');
    }
    return LayoutBuilder(
      builder: (context, constraints) {
        final graphWidth = _viewModels.fold<double>(
          0,
          (width, viewModel) =>
              math.max(width, GitHistoryGraph.widthFor(viewModel)),
        );
        const double minContentWidth = 320;
        final contentWidth = math.max(
          constraints.maxWidth,
          graphWidth + minContentWidth,
        );
        return Scrollbar(
          controller: _horizontalController,
          child: SingleChildScrollView(
            controller: _horizontalController,
            scrollDirection: .horizontal,
            child: SizedBox(
              width: contentWidth,
              child: Scrollbar(
                controller: _verticalController,
                child: ListView.builder(
                  controller: _verticalController,
                  itemCount:
                      _viewModels.length +
                      ((_loadingMore || _pageFailed) ? 1 : 0),
                  itemBuilder: (context, index) {
                    if (index >= _viewModels.length) {
                      return _buildFooter();
                    }
                    final viewModel = _viewModels[index];
                    final boundary = _isBoundary(viewModel);
                    return _CommitGraphRow(
                      viewModel: viewModel,
                      graphWidth: graphWidth,
                      isCompareAnchor:
                          _compareAnchorId == viewModel.historyItem.id,
                      onTap: () =>
                          unawaited(_handleCommitTap(viewModel.historyItem)),
                      onOpenActions: boundary
                          ? (position) => unawaited(_openBoundaryMenu(position))
                          : (position) => unawaited(
                              _openCommitMenu(viewModel.historyItem, position),
                            ),
                      onOpenRefActions: boundary
                          ? null
                          : (itemRef, position) =>
                                unawaited(_openRefMenu(itemRef, position)),
                    );
                  },
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  static bool _isBoundary(GitHistoryItemViewModel viewModel) {
    return viewModel.kind == GitHistoryItemViewModelKind.incomingChanges ||
        viewModel.kind == GitHistoryItemViewModelKind.outgoingChanges;
  }

  Widget _buildFooter() {
    if (_pageFailed) {
      return SizedBox(
        height: 32,
        child: Center(
          child: TextButton(
            onPressed: () => unawaited(_loadMore()),
            child: Text(
              'Could not load more commits. Retry',
              style: Theme.of(context).textTheme.labelSmall
                  ?.copyWith(color: AleraTokens.accent),
            ),
          ),
        ),
      );
    }
    return const SizedBox(
      height: 32,
      child: Center(
        child: SizedBox(
          width: 14,
          height: 14,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
      ),
    );
  }

  static String _relativeTime(DateTime? timestamp) {
    if (timestamp == null) {
      return '';
    }
    final difference = DateTime.now().difference(timestamp.toLocal());
    if (difference.isNegative) {
      return 'just now';
    }
    if (difference.inMinutes < 1) {
      return 'just now';
    }
    if (difference.inHours < 1) {
      return '${difference.inMinutes}m ago';
    }
    if (difference.inDays < 1) {
      return '${difference.inHours}h ago';
    }
    if (difference.inDays < 30) {
      return '${difference.inDays}d ago';
    }
    final local = timestamp.toLocal();
    final month = local.month.toString().padLeft(2, '0');
    final day = local.day.toString().padLeft(2, '0');
    return '${local.year}-$month-$day';
  }
}
