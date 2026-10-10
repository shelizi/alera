import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_picker_sheet.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// Read-only HEAD-to-worktree diff. Git stays on the paired runtime; only a
/// bounded patch for the requested path is sent over the authenticated socket.
class const WorkspaceFileDiffScreen({
  super.key,
  required final String hostId,
  required final String workspaceId,
  required final String relativePath,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<WorkspaceFileDiffScreen> createState() =>
      _WorkspaceFileDiffScreenState();
}

class _WorkspaceFileDiffScreenState
    extends ConsumerState<WorkspaceFileDiffScreen> {
  static final Logger _logger = Logger('WorkspaceFileDiffScreen');
  late final Future<MobileWorkspaceFileDiff> _diff;

  @override
  void initState() {
    super.initState();
    _diff = _loadDiff();
  }

  Future<MobileWorkspaceFileDiff> _loadDiff() async {
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      return await client.diffWorkspaceFile(
        workspaceId: widget.workspaceId,
        relativePath: widget.relativePath,
      );
    } on Object catch (error, stackTrace) {
      _logger.warning('Could not load mobile Git diff', error, stackTrace);
      rethrow;
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: Text(
        'Changes: ${workspaceFileBaseName(widget.relativePath)}',
        overflow: TextOverflow.ellipsis,
      ),
    ),
    body: SafeArea(
      child: FutureBuilder<MobileWorkspaceFileDiff>(
        future: _diff,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Center(
              child: Text('Unable to load Git diff: ${snapshot.error}'),
            );
          }
          final diff = snapshot.data;
          if (diff == null) {
            return const Center(child: CircularProgressIndicator());
          }
          if (diff.patch.isEmpty) {
            return const Center(child: Text('No tracked changes from HEAD.'));
          }
          return Column(
            children: <Widget>[
              if (diff.truncated)
                const Padding(
                  padding: AleraTokens.contentPadding,
                  child: Text('Patch truncated at 512 KiB.'),
                ),
              Expanded(
                child: SingleChildScrollView(
                  padding: AleraTokens.contentPadding,
                  child: SelectableText(
                    diff.patch,
                    style: Theme.of(context).textTheme.bodySmall
                        ?.copyWith(fontFamily: AleraTokens.monoFontFamily),
                  ),
                ),
              ),
            ],
          );
        },
      ),
    ),
  );
}
