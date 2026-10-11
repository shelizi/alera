import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/workbench/presentation/mobile_git_patch_view.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_picker_sheet.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// Read-only exact-path patch between commits in one registered Git worktree.
class WorkspaceGitRevisionPatchScreen extends ConsumerStatefulWidget {
  const WorkspaceGitRevisionPatchScreen({
    super.key,
    required this.hostId,
    required this.workspaceId,
    required this.baseCommit,
    required this.targetCommit,
    required this.relativePath,
  });

  final String hostId;
  final String workspaceId;
  final String? baseCommit;
  final String targetCommit;
  final String relativePath;

  @override
  ConsumerState<WorkspaceGitRevisionPatchScreen> createState() =>
      _WorkspaceGitRevisionPatchScreenState();
}

class _WorkspaceGitRevisionPatchScreenState
    extends ConsumerState<WorkspaceGitRevisionPatchScreen> {
  static final Logger _logger = Logger('WorkspaceGitRevisionPatchScreen');
  late Future<MobileWorkspaceGitRevisionPatch> _patch;

  @override
  void initState() {
    super.initState();
    _patch = _fetch();
  }

  Future<MobileWorkspaceGitRevisionPatch> _fetch() async {
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      return await client.loadWorkspaceGitRevisionPatch(
        workspaceId: widget.workspaceId,
        baseCommit: widget.baseCommit,
        targetCommit: widget.targetCommit,
        relativePath: widget.relativePath,
      );
    } on Object catch (error, trace) {
      _logger.warning('Could not load historical file patch', error, trace);
      rethrow;
    }
  }

  void _refresh() => setState(() => _patch = _fetch());

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: Text(
        'Diff: ${workspaceFileBaseName(widget.relativePath)}',
        overflow: TextOverflow.ellipsis,
      ),
      actions: <Widget>[
        IconButton(
          tooltip: 'Refresh historical diff',
          icon: const Icon(Icons.refresh),
          onPressed: _refresh,
        ),
      ],
    ),
    body: SafeArea(
      child: FutureBuilder<MobileWorkspaceGitRevisionPatch>(
        future: _patch,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Center(
              child: Padding(
                padding: AleraTokens.contentPadding,
                child: Text('Cannot read Git patch: ${snapshot.error}'),
              ),
            );
          }
          final revision = snapshot.data;
          if (revision == null) {
            return const Center(child: CircularProgressIndicator());
          }
          return Column(
            children: <Widget>[
              Padding(
                padding: AleraTokens.contentPadding,
                child: Text(
                  revision.relativePath,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ),
              Expanded(
                child: MobileGitPatchView(
                  patch: revision.patch,
                  truncated: revision.truncated,
                ),
              ),
            ],
          );
        },
      ),
    ),
  );
}
