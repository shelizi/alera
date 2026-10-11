import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_git_compare_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

enum _GitHistoryAction { details, changes, selectBase, compareWithBase }

/// Read-only, bounded commit history for the selected runtime worktree HEAD.
/// History never exposes arbitrary Git ref lookups or shell commands.
class WorkspaceGitHistoryScreen extends ConsumerStatefulWidget {
  const WorkspaceGitHistoryScreen({
    super.key,
    required this.hostId,
    required this.workspaceId,
    required this.workspaceName,
  });

  final String hostId;
  final String workspaceId;
  final String workspaceName;

  @override
  ConsumerState<WorkspaceGitHistoryScreen> createState() =>
      _WorkspaceGitHistoryScreenState();
}

class _WorkspaceGitHistoryScreenState
    extends ConsumerState<WorkspaceGitHistoryScreen> {
  static final Logger _logger = Logger('WorkspaceGitHistoryScreen');
  late Future<MobileWorkspaceGitHistory> _history;
  int _limit = 60;
  String? _comparisonBase;

  @override
  void initState() {
    super.initState();
    _history = _fetch();
  }

  Future<MobileWorkspaceGitHistory> _fetch() async {
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      return await client.loadWorkspaceGitHistory(
        widget.workspaceId,
        limit: _limit,
      );
    } on Object catch (error, trace) {
      _logger.warning('Could not load mobile Git history', error, trace);
      rethrow;
    }
  }

  Future<void> _refresh() async {
    final next = _fetch();
    setState(() => _history = next);
    try {
      await next;
    } on Object {
      // FutureBuilder displays the error and keeps RefreshIndicator usable.
    }
  }

  void _showDetails(MobileWorkspaceGitCommit commit) {
    final sha = commit.id;
    showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(commit.summary.isEmpty ? '(no subject)' : commit.summary),
        content: SizedBox(
          width: 560,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: <Widget>[
                SelectableText(commit.message),
                const SizedBox(height: 12),
                SelectableText('Commit: $sha'),
                Text('Author: ${commit.author}'),
                Text('Date: ${_commitDate(commit.timestamp)}'),
                if (commit.parents.isNotEmpty)
                  SelectableText('Parents: ${commit.parents.join(', ')}'),
              ],
            ),
          ),
        ),
        actions: <Widget>[
          if (ref
                  .read(hostConnectionControllerProvider(widget.hostId))
                  .value
                  ?.supportsWorkspaceGitCompare ==
              true)
            TextButton(
              onPressed: () {
                Navigator.of(context).pop();
                _openComparison(
                  commit.parents.isEmpty ? null : commit.parents.first,
                  commit.id,
                );
              },
              child: const Text('Changes'),
            ),
          TextButton.icon(
            icon: const Icon(Icons.copy_outlined),
            label: const Text('Copy SHA'),
            onPressed: () {
              Clipboard.setData(ClipboardData(text: sha));
              Navigator.of(context).pop();
            },
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }

  void _openComparison(String? base, String target) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => WorkspaceGitCompareScreen(
          hostId: widget.hostId,
          workspaceId: widget.workspaceId,
          baseCommit: base,
          targetCommit: target,
        ),
      ),
    );
  }

  void _handleHistoryAction(
    _GitHistoryAction action,
    MobileWorkspaceGitCommit commit,
  ) {
    switch (action) {
      case _GitHistoryAction.details:
        _showDetails(commit);
      case _GitHistoryAction.changes:
        _openComparison(
          commit.parents.isEmpty ? null : commit.parents.first,
          commit.id,
        );
      case _GitHistoryAction.selectBase:
        setState(() => _comparisonBase = commit.id);
      case _GitHistoryAction.compareWithBase:
        final base = _comparisonBase;
        if (base != null) _openComparison(base, commit.id);
    }
  }

  @override
  Widget build(BuildContext context) {
    final canCompare =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceGitCompare ==
        true;
    return Scaffold(
      appBar: AppBar(
        title: Text(
          'History: ${widget.workspaceName}',
          overflow: TextOverflow.ellipsis,
        ),
        actions: <Widget>[
          IconButton(
            tooltip: 'Refresh Git History',
            icon: const Icon(Icons.refresh),
            onPressed: _refresh,
          ),
        ],
      ),
      body: SafeArea(
        child: FutureBuilder<MobileWorkspaceGitHistory>(
          future: _history,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return Center(
                child: Padding(
                  padding: AleraTokens.contentPadding,
                  child: Text(
                    'Could not read commit history: ${snapshot.error}',
                  ),
                ),
              );
            }
            final history = snapshot.data;
            if (history == null) {
              return const Center(child: CircularProgressIndicator());
            }
            return RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(),
                children: <Widget>[
                  Padding(
                    padding: AleraTokens.contentPadding,
                    child: Text(
                      'Branch: ${history.branch} · latest ${history.commits.length} commits',
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                  ),
                  if (canCompare && _comparisonBase != null)
                    ListTile(
                      leading: const Icon(Icons.compare_arrows),
                      title: Text(
                        'Compare from ${_shortSha(_comparisonBase!)}',
                      ),
                      subtitle: const Text(
                        'Choose another commit and select Compare.',
                      ),
                      trailing: TextButton(
                        onPressed: () => setState(() => _comparisonBase = null),
                        child: const Text('Clear'),
                      ),
                    ),
                  if (history.commits.isEmpty)
                    const ListTile(
                      leading: Icon(Icons.history),
                      title: Text('No commits'),
                    ),
                  for (final commit in history.commits)
                    ListTile(
                      key: ValueKey<String>(commit.id),
                      leading: const Icon(Icons.commit),
                      title: Text(
                        commit.summary.isEmpty
                            ? '(no subject)'
                            : commit.summary,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                      subtitle: Text(
                        '${_shortSha(commit.id)} · ${commit.author}\n${_commitDate(commit.timestamp)}',
                      ),
                      isThreeLine: true,
                      trailing: canCompare
                          ? PopupMenuButton<_GitHistoryAction>(
                              tooltip: 'Git commit actions',
                              onSelected: (action) =>
                                  _handleHistoryAction(action, commit),
                              itemBuilder: (_) =>
                                  <PopupMenuEntry<_GitHistoryAction>>[
                                    const PopupMenuItem(
                                      value: _GitHistoryAction.details,
                                      child: Text('Commit details'),
                                    ),
                                    const PopupMenuItem(
                                      value: _GitHistoryAction.changes,
                                      child: Text('Changes in this commit'),
                                    ),
                                    const PopupMenuItem(
                                      value: _GitHistoryAction.selectBase,
                                      child: Text('Select as comparison base'),
                                    ),
                                    if (_comparisonBase != null &&
                                        _comparisonBase != commit.id)
                                      const PopupMenuItem(
                                        value:
                                            _GitHistoryAction.compareWithBase,
                                        child: Text(
                                          'Compare with selected base',
                                        ),
                                      ),
                                  ],
                            )
                          : const Icon(Icons.chevron_right),
                      onTap: () => _showDetails(commit),
                      onLongPress: canCompare
                          ? () => setState(() => _comparisonBase = commit.id)
                          : null,
                    ),
                  if (_limit < 100 && history.commits.length >= _limit)
                    ListTile(
                      leading: const Icon(Icons.expand_more),
                      title: const Text('Show up to 100 commits'),
                      onTap: () {
                        _limit = 100;
                        _refresh();
                      },
                    ),
                  if (history.commits.length == 100)
                    const Padding(
                      padding: AleraTokens.contentPadding,
                      child: Text('Showing the latest 100 commits.'),
                    ),
                ],
              ),
            );
          },
        ),
      ),
    );
  }

  String _shortSha(String sha) => sha.length > 8 ? sha.substring(0, 8) : sha;

  String _commitDate(int seconds) =>
      DateTime.fromMillisecondsSinceEpoch(seconds * 1000).toLocal().toString();
}
