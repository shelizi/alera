import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/runtime/infra/mobile_runtime_client.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// Lists only this workspace's Runtime-owned recovery records. Restoring
/// never replaces an existing path, and there is no permanent-delete action.
class WorkspaceTrashScreen extends ConsumerStatefulWidget {
  const WorkspaceTrashScreen({
    super.key,
    required this.hostId,
    required this.workspaceId,
  });

  final String hostId;
  final String workspaceId;

  @override
  ConsumerState<WorkspaceTrashScreen> createState() =>
      _WorkspaceTrashScreenState();
}

class _WorkspaceTrashScreenState extends ConsumerState<WorkspaceTrashScreen> {
  static final Logger _logger = Logger('WorkspaceTrashScreen');
  late Future<MobileWorkspaceTrashList> _trash;
  String? _restoringId;

  @override
  void initState() {
    super.initState();
    _trash = _load();
  }

  Future<MobileWorkspaceTrashList> _load() async {
    final client = await ref.read(
      hostConnectionControllerProvider(widget.hostId).future,
    );
    return client.listWorkspaceTextTrash(widget.workspaceId);
  }

  void _refresh() => setState(() => _trash = _load());

  Future<void> _restore(MobileWorkspaceTrashEntry item) async {
    if (_restoringId != null) return;
    final accepted = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Restore this text file?'),
        content: Text(
          'Restore ${item.relativePath} to the original folder? An existing file will not be overwritten. The backup is retained if restoration fails.',
        ),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Restore'),
          ),
        ],
      ),
    );
    if (accepted != true || !mounted) return;
    setState(() => _restoringId = item.id);
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      await client.restoreWorkspaceTextFromTrash(
        workspaceId: widget.workspaceId,
        trashId: item.id,
      );
      if (!mounted) return;
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text('Restored ${item.relativePath}')));
      _refresh();
    } on MobileRuntimeConflictException catch (error, trace) {
      _logger.warning('Recovery path already occupied', error, trace);
      if (mounted) {
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(
            content: Text(
              'Restore blocked: a file already exists at this path.',
            ),
          ),
        );
      }
    } on Object catch (error, trace) {
      _logger.warning('Could not restore workspace text', error, trace);
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text('Restore failed: $error')));
      }
    } finally {
      if (mounted) setState(() => _restoringId = null);
    }
  }

  @override
  Widget build(BuildContext context) => Scaffold(
    appBar: AppBar(
      title: const Text('Recoverable files'),
      actions: <Widget>[
        IconButton(
          tooltip: 'Refresh recovered files',
          icon: const Icon(Icons.refresh),
          onPressed: _restoringId == null ? _refresh : null,
        ),
      ],
    ),
    body: SafeArea(
      child: FutureBuilder<MobileWorkspaceTrashList>(
        future: _trash,
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Center(
              child: Padding(
                padding: AleraTokens.contentPadding,
                child: Text('Unable to list recovery files: ${snapshot.error}'),
              ),
            );
          }
          final data = snapshot.data;
          if (data == null) {
            return const Center(child: CircularProgressIndicator());
          }
          return ListView(
            children: <Widget>[
              Padding(
                padding: AleraTokens.contentPadding,
                child: Text(
                  'Stored on the paired host, outside the workspace. No automatic purge or permanent-delete control. Limit ${data.limit} files / ${data.capacityBytes ~/ (1024 * 1024)} MiB.',
                  style: Theme.of(context).textTheme.bodySmall,
                ),
              ),
              if (data.items.isEmpty)
                const ListTile(
                  leading: Icon(Icons.delete_outline),
                  title: Text('No recoverable files'),
                ),
              for (final item in data.items)
                ListTile(
                  key: ValueKey<String>(item.id),
                  leading: const Icon(Icons.restore_from_trash_outlined),
                  title: Text(
                    item.relativePath,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                  subtitle: Text(
                    '${item.sizeBytes} bytes · ${DateTime.fromMillisecondsSinceEpoch(item.deletedAtMs).toLocal()}',
                    maxLines: 2,
                  ),
                  trailing: _restoringId == item.id
                      ? const SizedBox(
                          width: 28,
                          height: 28,
                          child: CircularProgressIndicator(),
                        )
                      : TextButton(
                          onPressed: _restoringId == null
                              ? () => _restore(item)
                              : null,
                          child: const Text('Restore'),
                        ),
                ),
            ],
          );
        },
      ),
    ),
  );
}
