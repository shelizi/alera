import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// Choose only a directory supplied by the authenticated workspace browser.
/// This deliberately offers no arbitrary filesystem path input or folder
/// creation. The host independently validates the selected directory.
class WorkspaceDirectoryPickerScreen extends ConsumerStatefulWidget {
  const WorkspaceDirectoryPickerScreen({
    super.key,
    required this.hostId,
    required this.workspaceId,
    required this.sourceDirectory,
    required this.fileName,
  });

  final String hostId;
  final String workspaceId;
  final String sourceDirectory;
  final String fileName;

  @override
  ConsumerState<WorkspaceDirectoryPickerScreen> createState() =>
      _WorkspaceDirectoryPickerScreenState();
}

class _WorkspaceDirectoryPickerScreenState
    extends ConsumerState<WorkspaceDirectoryPickerScreen> {
  static final Logger _logger = Logger('WorkspaceDirectoryPicker');
  final List<MobileWorkspaceDirectoryEntry> _folders = [];
  String _directory = '';
  int _generation = 0;
  int _nextOffset = 0;
  bool _more = false;
  bool _loading = false;
  String? _error;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) _load(reset: true);
    });
  }

  String get _parent {
    final index = _directory.lastIndexOf('/');
    return index < 0 ? '' : _directory.substring(0, index);
  }

  void _navigate(String path) {
    _directory = path;
    _load(reset: true);
  }

  Future<void> _load({required bool reset}) async {
    if (!reset && _loading) return;
    final generation = reset ? ++_generation : _generation;
    final directory = _directory;
    final offset = reset ? 0 : _nextOffset;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) {
        _folders.clear();
        _nextOffset = 0;
        _more = false;
      }
    });
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      final page = await client.listWorkspaceDirectory(
        workspaceId: widget.workspaceId,
        relativePath: directory,
        offset: offset,
        limit: 100,
      );
      if (!mounted || generation != _generation) return;
      setState(() {
        _folders.addAll(page.items.where((entry) => entry.isDirectory));
        _nextOffset = page.nextOffset;
        _more = page.hasMore;
      });
    } on Object catch (error, trace) {
      _logger.warning('Destination folder listing failed', error, trace);
      if (mounted && generation == _generation) {
        setState(() => _error = error.toString());
      }
    } finally {
      if (mounted && generation == _generation) {
        setState(() => _loading = false);
      }
    }
  }

  @override
  Widget build(BuildContext context) {
    final destination = _directory.isEmpty
        ? '/${widget.fileName}'
        : '/$_directory/${widget.fileName}';
    final canSelect =
        !_loading && _error == null && _directory != widget.sourceDirectory;
    return Scaffold(
      appBar: AppBar(title: const Text('Choose destination folder')),
      body: SafeArea(
        child: Column(
          children: <Widget>[
            Padding(
              padding: AleraTokens.contentPadding,
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: <Widget>[
                  Text(
                    'Destination: $destination',
                    maxLines: 3,
                    overflow: TextOverflow.ellipsis,
                  ),
                  const SizedBox(height: 8),
                  FilledButton.icon(
                    onPressed: canSelect
                        ? () => Navigator.of(context).pop(_directory)
                        : null,
                    icon: const Icon(Icons.drive_file_move_outline),
                    label: const Text('Select this folder'),
                  ),
                  Text(
                    'Existing folders in this workspace only. The host will never replace an existing file.',
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                ],
              ),
            ),
            const Divider(height: 1),
            Expanded(
              child: ListView(
                children: <Widget>[
                  if (_directory.isNotEmpty)
                    ListTile(
                      leading: const Icon(Icons.drive_folder_upload_outlined),
                      title: const Text('..'),
                      onTap: () => _navigate(_parent),
                    ),
                  for (final folder in _folders)
                    ListTile(
                      key: ValueKey<String>(folder.relativePath),
                      leading: const Icon(Icons.folder_outlined),
                      title: Text(folder.name),
                      trailing: const Icon(Icons.chevron_right),
                      onTap: () => _navigate(folder.relativePath),
                    ),
                  if (_loading)
                    const Padding(
                      padding: AleraTokens.contentPadding,
                      child: Center(child: CircularProgressIndicator()),
                    ),
                  if (_error != null)
                    ListTile(
                      leading: const Icon(Icons.error_outline),
                      title: Text(_error!),
                      trailing: const Icon(Icons.refresh),
                      onTap: () => _load(reset: true),
                    ),
                  if (!_loading && _more)
                    ListTile(
                      leading: const Icon(Icons.expand_more),
                      title: const Text('Load more folders'),
                      onTap: () => _load(reset: false),
                    ),
                  if (!_loading && _error == null && _folders.isEmpty && !_more)
                    const ListTile(title: Text('No subfolders')),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
