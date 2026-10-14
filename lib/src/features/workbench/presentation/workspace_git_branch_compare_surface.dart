import 'dart:async';
import 'dart:math' as math;

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_source_control_scope.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/workspace_git_diff_surface.dart';
import 'package:alera/src/shared/infra/git/git_diff_models.dart';
import 'package:alera/src/shared/infra/git/git_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// A read-only, snapshot-pinned branch comparison with lazy per-file diffs.
class const WorkspaceGitBranchCompareSurface({
  super.key,
  required this.workspace,
  required this.tab,
}) extends ConsumerStatefulWidget {
  final Workspace workspace;
  final WorkspaceTabRecord tab;

  @override
  ConsumerState<WorkspaceGitBranchCompareSurface> createState() =>
      _WorkspaceGitBranchCompareSurfaceState();
}

class _WorkspaceGitBranchCompareSurfaceState
    extends ConsumerState<WorkspaceGitBranchCompareSurface> {
  GitCommitCompareResult? _snapshot;
  _BranchCompareNode? _root;
  String? _selectedPath;
  String _query = '';
  GitChangeStatus? _statusFilter;
  final Set<String> _collapsed = <String>{};
  int _generation = 0;
  bool _loading = true;
  bool _updating = false;
  String? _error;

  String get _repositoryPath {
    final root = normalizeSourceControlRootRelativePath(widget.tab.gitDiffRoot);
    if (root == null) return widget.workspace.path;
    return sourceControlRootAbsolutePath(
      workspacePath: widget.workspace.path,
      relativeRoot: root,
    );
  }

  @override
  void initState() {
    super.initState();
    unawaited(_loadPinnedSnapshot());
  }

  @override
  void didUpdateWidget(covariant WorkspaceGitBranchCompareSurface oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.workspace.path != widget.workspace.path ||
        oldWidget.tab.gitDiffRoot != widget.tab.gitDiffRoot ||
        oldWidget.tab.gitDiffCommitOid != widget.tab.gitDiffCommitOid ||
        oldWidget.tab.gitDiffParentOid != widget.tab.gitDiffParentOid) {
      unawaited(_loadPinnedSnapshot());
    }
  }

  @override
  void dispose() {
    _generation += 1;
    super.dispose();
  }

  Future<void> _loadPinnedSnapshot() async {
    final generation = ++_generation;
    if (mounted)
      setState(() {
        _loading = true;
        _error = null;
      });
    try {
      final head = widget.tab.gitDiffCommitOid;
      final base = widget.tab.gitDiffParentOid;
      if (head == null || base == null) {
        throw StateError('This branch comparison has no saved Git snapshot.');
      }
      final snapshot = await ref
          .read(gitBackendProvider)
          .compareRange(path: _repositoryPath, baseRef: base, headRef: head);
      if (snapshot.summary.status != GitCommitCompareStatus.ready) {
        throw StateError(
          snapshot.summary.errorMessage ?? 'Could not compare branches.',
        );
      }
      if (!mounted || generation != _generation) return;
      _acceptSnapshot(snapshot);
    } catch (error) {
      if (!mounted || generation != _generation) return;
      setState(() {
        _loading = false;
        _error = error.toString();
      });
    }
  }

  void _acceptSnapshot(GitCommitCompareResult snapshot) {
    final paths = snapshot.entries.map((entry) => entry.path).toSet();
    setState(() {
      _snapshot = snapshot;
      _root = _buildBranchCompareTree(snapshot.entries);
      _selectedPath = paths.contains(_selectedPath)
          ? _selectedPath
          : snapshot.entries.firstOrNull?.path;
      _loading = false;
      _updating = false;
      _error = null;
    });
  }

  Future<void> _updateSnapshot() async {
    final baseRef = widget.tab.gitBranchCompareBaseRef;
    final headRef = widget.tab.gitBranchCompareHeadRef;
    if (_updating || baseRef == null || headRef == null) return;
    setState(() => _updating = true);
    try {
      final compare = await ref
          .read(gitBackendProvider)
          .compareRange(
            path: _repositoryPath,
            baseRef: baseRef,
            headRef: headRef,
          );
      final parentOid = compare.summary.parentOid;
      if (compare.summary.status != GitCommitCompareStatus.ready ||
          parentOid == null) {
        throw StateError(
          compare.summary.errorMessage ?? 'Could not update comparison.',
        );
      }
      if (!mounted) return;
      await ref
          .read(workbenchControllerProvider.notifier)
          .updateGitBranchCompareSnapshot(
            tabId: widget.tab.id,
            headOid: compare.summary.commitOid,
            mergeBaseOid: parentOid,
          );
      if (!mounted) return;
      ++_generation;
      _acceptSnapshot(compare);
    } catch (error) {
      if (mounted)
        setState(() {
          _updating = false;
          _error = error.toString();
        });
    }
  }

  List<_BranchCompareNode> _visibleNodes() {
    final root = _root;
    final snapshot = _snapshot;
    if (root == null || snapshot == null) return const [];
    final included = <String>{};
    for (final entry in snapshot.entries) {
      if (_statusFilter != null && entry.status != _statusFilter) continue;
      if (_query.isNotEmpty &&
          !entry.path.toLowerCase().contains(_query.toLowerCase())) {
        continue;
      }
      included.add(entry.path);
      final parts = entry.path.split('/');
      var ancestor = '';
      for (var i = 0; i < parts.length - 1; i++) {
        ancestor = ancestor.isEmpty ? parts[i] : '$ancestor/${parts[i]}';
        included.add(ancestor);
      }
    }
    final result = <_BranchCompareNode>[];
    void visit(_BranchCompareNode parent) {
      final children = parent.children.values.toList()
        ..sort((a, b) {
          if (a.isFolder != b.isFolder) return a.isFolder ? -1 : 1;
          return a.name.toLowerCase().compareTo(b.name.toLowerCase());
        });
      for (final node in children) {
        if (!included.contains(node.path)) continue;
        result.add(node);
        if (node.isFolder && !_collapsed.contains(node.path)) visit(node);
      }
    }

    visit(root);
    return result;
  }

  WorkspaceTabRecord _fileTab(GitCommitChangeEntry entry) {
    final snapshot = _snapshot!;
    return widget.tab.copyWith(
      title: entry.path,
      payload: <String, Object?>{
        ...widget.tab.payload,
        workspaceTabGitDiffSourcePayloadKey: WorkspaceGitDiffSource.commit.key,
        workspaceTabGitDiffScopePayloadKey: WorkspaceGitDiffScope.file.key,
        workspaceTabGitDiffCommitOidPayloadKey: snapshot.summary.commitOid,
        workspaceTabGitDiffParentOidPayloadKey: snapshot.summary.parentOid,
        workspaceTabGitDiffCompareRefPayloadKey:
            '${widget.tab.gitBranchCompareBaseRef}...${widget.tab.gitBranchCompareHeadRef}',
        workspaceTabFilePathPayloadKey: entry.path,
        workspaceTabGitDiffOldPathPayloadKey: entry.oldPath,
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    final snapshot = _snapshot;
    final entries = snapshot?.entries ?? const <GitCommitChangeEntry>[];
    final visible = _visibleNodes();
    final selected = entries
        .where((entry) => entry.path == _selectedPath)
        .firstOrNull;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Padding(
          padding: const EdgeInsets.all(AleraTokens.space8),
          child: Row(
            children: <Widget>[
              const Icon(Icons.compare_arrows, size: 18),
              const SizedBox(width: AleraTokens.space8),
              Expanded(
                child: Text(
                  '${widget.tab.gitBranchCompareBaseRef ?? '?'} ... ${widget.tab.gitBranchCompareHeadRef ?? '?'}',
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              Text(
                '${entries.length} files',
                style: Theme.of(context).textTheme.bodySmall,
              ),
              const SizedBox(width: AleraTokens.space8),
              OutlinedButton.icon(
                onPressed: _updating ? null : _updateSnapshot,
                icon: _updating
                    ? const SizedBox.square(
                        dimension: 16,
                        child: CircularProgressIndicator(strokeWidth: 2),
                      )
                    : const Icon(Icons.refresh, size: 16),
                label: const Text('Update Comparison'),
              ),
            ],
          ),
        ),
        const Divider(height: 1, color: AleraTokens.borderSubtle),
        if (_error != null)
          Padding(
            padding: const EdgeInsets.all(AleraTokens.space8),
            child: Text(
              _error!,
              style: const TextStyle(color: AleraTokens.error),
            ),
          ),
        if (_loading)
          const Expanded(child: Center(child: CircularProgressIndicator()))
        else if (snapshot == null)
          const Expanded(child: Center(child: Text('Comparison unavailable.')))
        else if (entries.isEmpty)
          const Expanded(
            child: Center(
              child: Text(
                'No changes from the merge base to the compare branch.',
              ),
            ),
          )
        else
          Expanded(
            child: LayoutBuilder(
              builder: (context, constraints) {
                final tree = _buildNavigator(visible);
                final details = selected == null
                    ? const Center(child: Text('Select a changed file.'))
                    : WorkspaceGitDiffSurface(
                        key: ValueKey<String>(
                          '${snapshot.summary.commitOid}:${snapshot.summary.parentOid}:${selected.path}',
                        ),
                        workspace: widget.workspace,
                        tab: _fileTab(selected),
                      );
                if (constraints.maxWidth < 620) {
                  return Column(
                    children: <Widget>[
                      SizedBox(
                        height: math.min(220, constraints.maxHeight * 0.45),
                        child: tree,
                      ),
                      const Divider(height: 1, color: AleraTokens.borderSubtle),
                      Expanded(child: details),
                    ],
                  );
                }
                return Row(
                  children: <Widget>[
                    SizedBox(
                      width: math.min(310, constraints.maxWidth * 0.36),
                      child: tree,
                    ),
                    const VerticalDivider(
                      width: 1,
                      color: AleraTokens.borderSubtle,
                    ),
                    Expanded(child: details),
                  ],
                );
              },
            ),
          ),
      ],
    );
  }

  Widget _buildNavigator(List<_BranchCompareNode> visible) {
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Padding(
          padding: const EdgeInsets.all(AleraTokens.space8),
          child: TextField(
            key: const ValueKey<String>('branch-compare-file-filter'),
            decoration: const InputDecoration(
              isDense: true,
              hintText: 'Filter changed files',
              prefixIcon: Icon(Icons.search, size: 18),
              border: OutlineInputBorder(),
            ),
            onChanged: (value) => setState(() => _query = value),
          ),
        ),
        SingleChildScrollView(
          scrollDirection: Axis.horizontal,
          child: Row(
            children: <Widget>[
              for (final status in <GitChangeStatus?>[
                null,
                .added,
                .modified,
                .deleted,
                .renamed,
                .copied,
              ])
                Padding(
                  padding: const EdgeInsets.only(left: AleraTokens.space4),
                  child: ChoiceChip(
                    label: Text(status?.badge ?? 'All'),
                    selected: status == _statusFilter,
                    onSelected: (_) => setState(() => _statusFilter = status),
                    tooltip: status?.name ?? 'All changed files',
                    visualDensity: VisualDensity.compact,
                  ),
                ),
            ],
          ),
        ),
        const Divider(height: 1, color: AleraTokens.borderSubtle),
        Expanded(
          child: ListView.builder(
            itemCount: visible.length,
            itemBuilder: (context, index) {
              final node = visible[index];
              final isSelected = !node.isFolder && node.path == _selectedPath;
              return InkWell(
                key: ValueKey<String>('branch-compare-row-${node.path}'),
                onTap: () => setState(() {
                  if (node.isFolder) {
                    if (!_collapsed.add(node.path))
                      _collapsed.remove(node.path);
                  } else {
                    _selectedPath = node.path;
                  }
                }),
                child: Container(
                  color: isSelected ? AleraTokens.surfaceVariant : null,
                  padding: EdgeInsets.only(
                    left: AleraTokens.space4 + node.depth * AleraTokens.space12,
                    right: AleraTokens.space8,
                  ),
                  height: AleraTokens.sidebarHeaderHeight * 0.73,
                  child: Row(
                    children: <Widget>[
                      Icon(
                        node.isFolder
                            ? _collapsed.contains(node.path)
                                  ? Icons.chevron_right
                                  : Icons.keyboard_arrow_down
                            : Icons.insert_drive_file_outlined,
                        size: 16,
                        color: AleraTokens.foregroundMuted,
                      ),
                      const SizedBox(width: AleraTokens.space4),
                      Expanded(
                        child: Text(
                          node.name,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: theme.textTheme.bodySmall,
                        ),
                      ),
                      if (node.entry case final entry?)
                        Text(
                          entry.status.badge,
                          style: theme.textTheme.labelSmall?.copyWith(
                            color: switch (entry.status) {
                              GitChangeStatus.added => AleraTokens.success,
                              GitChangeStatus.deleted => AleraTokens.error,
                              GitChangeStatus.modified => AleraTokens.warning,
                              GitChangeStatus.renamed ||
                              GitChangeStatus.copied ||
                              GitChangeStatus.untracked =>
                                AleraTokens.foregroundMuted,
                            },
                          ),
                        ),
                    ],
                  ),
                ),
              );
            },
          ),
        ),
      ],
    );
  }
}

class _BranchCompareNode {
  _BranchCompareNode({
    required this.path,
    required this.name,
    required this.depth,
    this.entry,
  });

  final String path;
  final String name;
  final int depth;
  final GitCommitChangeEntry? entry;
  final Map<String, _BranchCompareNode> children =
      <String, _BranchCompareNode>{};
  bool get isFolder => entry == null;
}

_BranchCompareNode _buildBranchCompareTree(List<GitCommitChangeEntry> entries) {
  final root = _BranchCompareNode(path: '', name: '', depth: -1);
  for (final entry in entries) {
    final segments = entry.path.split('/');
    var node = root;
    var path = '';
    for (var i = 0; i < segments.length; i++) {
      final segment = segments[i];
      path = path.isEmpty ? segment : '$path/$segment';
      node = node.children.putIfAbsent(
        segment,
        () => _BranchCompareNode(
          path: path,
          name: segment,
          depth: i,
          entry: i == segments.length - 1 ? entry : null,
        ),
      );
    }
  }
  return root;
}
