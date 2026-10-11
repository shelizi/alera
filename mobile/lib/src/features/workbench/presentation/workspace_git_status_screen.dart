import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_diff_screen.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_viewer_screen.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_git_history_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// Mobile source control permits guarded per-file staging and read-only history.
/// Commit, Reset and Push remain unavailable on a paired phone.
class const WorkspaceGitStatusScreen({
  super.key,
  required final String hostId,
  required final String workspaceId,
  required final String workspaceName,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceGitStatusScreen> createState() =>
      _WorkspaceGitStatusScreenState();
}

class _WorkspaceGitStatusScreenState
    extends ConsumerState<WorkspaceGitStatusScreen> {
  static final Logger _logger = Logger('WorkspaceGitStatusScreen');
  late Future<MobileWorkspaceGitStatus> _status;
  bool _mutating = false;

  @override
  void initState() {
    super.initState();
    _status = _fetchStatus();
  }

  Future<MobileWorkspaceGitStatus> _fetchStatus() async {
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      return await client.loadWorkspaceGitStatus(widget.workspaceId);
    } on Object catch (error, stackTrace) {
      _logger.warning('Could not load mobile Git status', error, stackTrace);
      rethrow;
    }
  }

  Future<void> _refresh() async {
    final next = _fetchStatus();
    setState(() => _status = next);
    await next;
  }

  void _openFile(MobileWorkspaceGitStatusEntry item) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (_) => item.status == 'added'
            ? WorkspaceFileViewerScreen(
                hostId: widget.hostId,
                workspaceId: widget.workspaceId,
                relativePath: item.relativePath,
              )
            : WorkspaceFileDiffScreen(
                hostId: widget.hostId,
                workspaceId: widget.workspaceId,
                relativePath: item.relativePath,
              ),
      ),
    );
  }

  Future<void> _setStaged(
    MobileWorkspaceGitStatusEntry item,
    bool stage,
  ) async {
    if (_mutating || item.status == 'conflict' || item.status == 'renamed') {
      return;
    }
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(stage ? 'Stage file changes?' : 'Unstage file changes?'),
        content: Text(
          stage
              ? 'Stage the current host version of ${item.relativePath}? The file may have changed since the last refresh.'
              : 'Remove ${item.relativePath} from the Git staging area? Working files will not be changed.',
        ),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.pop(context, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, true),
            child: Text(stage ? 'Stage' : 'Unstage'),
          ),
        ],
      ),
    );
    if (confirmed != true || !mounted) return;
    setState(() => _mutating = true);
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      await client.stageWorkspaceFile(
        workspaceId: widget.workspaceId,
        relativePath: item.relativePath,
        stage: stage,
      );
      if (mounted) await _refresh();
    } on Object catch (error, trace) {
      _logger.warning('Git staging action failed', error, trace);
      if (mounted) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text('Git staging failed: $error')));
      }
    } finally {
      if (mounted) setState(() => _mutating = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final canStage =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceGitStage ==
        true;
    final canHistory =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceGitHistory ==
        true;
    return Scaffold(
      appBar: AppBar(
        title: Text(
          'Source Control: ${widget.workspaceName}',
          overflow: TextOverflow.ellipsis,
        ),
        actions: <Widget>[
          if (canHistory)
            IconButton(
              tooltip: 'Commit History',
              icon: const Icon(Icons.history),
              onPressed: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => WorkspaceGitHistoryScreen(
                    hostId: widget.hostId,
                    workspaceId: widget.workspaceId,
                    workspaceName: widget.workspaceName,
                  ),
                ),
              ),
            ),
          IconButton(
            tooltip: 'Refresh Changes',
            onPressed: _mutating ? null : _refresh,
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: SafeArea(
        child: FutureBuilder<MobileWorkspaceGitStatus>(
          future: _status,
          builder: (context, snapshot) {
            if (snapshot.hasError) {
              return Center(
                child: Padding(
                  padding: AleraTokens.contentPadding,
                  child: Text('Cannot read Git status: ${snapshot.error}'),
                ),
              );
            }
            final status = snapshot.data;
            if (status == null) {
              return const Center(child: CircularProgressIndicator());
            }
            return RefreshIndicator(
              onRefresh: _refresh,
              child: ListView(
                physics: const AlwaysScrollableScrollPhysics(),
                children: <Widget>[
                  if (status.truncated)
                    const Padding(
                      padding: AleraTokens.contentPadding,
                      child: Text('Showing the first 200 changed files.'),
                    ),
                  if (status.items.isEmpty)
                    const ListTile(
                      leading: Icon(Icons.check_circle_outline),
                      title: Text('Working tree clean'),
                      subtitle: Text('No uncommitted changes.'),
                    ),
                  for (final item in status.items)
                    ListTile(
                      leading: Icon(_statusIcon(item.status)),
                      title: Text(
                        item.relativePath,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                      ),
                      subtitle: Text(
                        '${_statusLabel(item.status)}${item.staged ? ' · staged' : ''}${item.unstaged ? ' · unstaged' : ''}',
                      ),
                      trailing:
                          canStage &&
                              !_mutating &&
                              (item.staged || item.unstaged) &&
                              item.status != 'conflict' &&
                              item.status != 'renamed'
                          ? PopupMenuButton<bool>(
                              tooltip: 'Git staging actions',
                              onSelected: (value) => _setStaged(item, value),
                              itemBuilder: (context) => <PopupMenuEntry<bool>>[
                                if (item.unstaged)
                                  const PopupMenuItem<bool>(
                                    value: true,
                                    child: Text('Stage current changes'),
                                  ),
                                if (item.staged)
                                  const PopupMenuItem<bool>(
                                    value: false,
                                    child: Text('Unstage changes'),
                                  ),
                              ],
                            )
                          : const Icon(Icons.chevron_right),
                      onTap: () => _openFile(item),
                    ),
                ],
              ),
            );
          },
        ),
      ),
    );
  }
}

IconData _statusIcon(String status) => switch (status) {
  'added' => Icons.add_circle_outline,
  'deleted' => Icons.remove_circle_outline,
  'conflict' => Icons.warning_amber_outlined,
  'renamed' => Icons.drive_file_rename_outline,
  _ => Icons.edit_note,
};

String _statusLabel(String status) => switch (status) {
  'added' => 'Added or untracked',
  'deleted' => 'Deleted',
  'conflict' => 'Merge conflict',
  'renamed' => 'Renamed',
  _ => 'Modified',
};
