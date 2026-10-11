import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_git_revision_patch_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// File changes between two commits reachable from the paired worktree HEAD.
/// When [baseCommit] is null, Git's empty tree is the comparison baseline.
class WorkspaceGitCompareScreen extends ConsumerStatefulWidget {
  const WorkspaceGitCompareScreen({
    super.key,
    required this.hostId,
    required this.workspaceId,
    required this.baseCommit,
    required this.targetCommit,
  });

  final String hostId;
  final String workspaceId;
  final String? baseCommit;
  final String targetCommit;

  @override
  ConsumerState<WorkspaceGitCompareScreen> createState() =>
      _WorkspaceGitCompareScreenState();
}

class _WorkspaceGitCompareScreenState
    extends ConsumerState<WorkspaceGitCompareScreen> {
  static final Logger _logger = Logger('WorkspaceGitCompareScreen');
  late Future<MobileWorkspaceGitComparison> _changes;

  @override
  void initState() {
    super.initState();
    _changes = _fetch();
  }

  Future<MobileWorkspaceGitComparison> _fetch() async {
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      return await client.compareWorkspaceGitCommits(
        workspaceId: widget.workspaceId,
        baseCommit: widget.baseCommit,
        targetCommit: widget.targetCommit,
      );
    } on Object catch (error, trace) {
      _logger.warning('Could not read Git commit changes', error, trace);
      rethrow;
    }
  }

  Future<void> _refresh() async {
    final next = _fetch();
    setState(() => _changes = next);
    try {
      await next;
    } on Object {
      // FutureBuilder shows the error and leaves pull-to-refresh available.
    }
  }

  String _shortSha(String? id) => id == null
      ? '(empty tree)'
      : id.length > 8
      ? id.substring(0, 8)
      : id;

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: const Text('Changed files'),
      actions: <Widget>[
        IconButton(
          tooltip: 'Refresh Git comparison',
          onPressed: _refresh,
          icon: const Icon(Icons.refresh),
        ),
      ],
    ),
    body: SafeArea(
      child: FutureBuilder<MobileWorkspaceGitComparison>(
        future: _changes,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Center(
              child: Padding(
                padding: AleraTokens.contentPadding,
                child: Text('Cannot compare commits: ${snapshot.error}'),
              ),
            );
          }
          final comparison = snapshot.data;
          if (comparison == null) {
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
                    '${_shortSha(comparison.baseCommit)} → ${_shortSha(comparison.targetCommit)}',
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                ),
                if (comparison.truncated)
                  const Padding(
                    padding: AleraTokens.contentPadding,
                    child: Text('Showing the first 200 changed paths.'),
                  ),
                if (comparison.items.isEmpty)
                  const ListTile(
                    leading: Icon(Icons.check_circle_outline),
                    title: Text('No file changes between commits'),
                  ),
                for (final file in comparison.items)
                  ListTile(
                    key: ValueKey<String>(
                      '${file.status}:${file.relativePath}',
                    ),
                    leading: Icon(_icon(file.status)),
                    title: Text(
                      file.relativePath,
                      maxLines: 2,
                      overflow: TextOverflow.ellipsis,
                    ),
                    subtitle: Text(_label(file)),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () => Navigator.of(context).push(
                      MaterialPageRoute<void>(
                        builder: (_) => WorkspaceGitRevisionPatchScreen(
                          hostId: widget.hostId,
                          workspaceId: widget.workspaceId,
                          baseCommit: comparison.baseCommit,
                          targetCommit: comparison.targetCommit,
                          relativePath: file.relativePath,
                        ),
                      ),
                    ),
                  ),
              ],
            ),
          );
        },
      ),
    ),
  );

  IconData _icon(String status) => switch (status) {
    'added' => Icons.add_circle_outline,
    'deleted' => Icons.remove_circle_outline,
    'renamed' => Icons.drive_file_rename_outline,
    _ => Icons.difference_outlined,
  };

  String _label(MobileWorkspaceGitChangedFile entry) =>
      entry.status == 'renamed' && entry.oldPath != null
      ? 'renamed from ${entry.oldPath}'
      : entry.status;
}
