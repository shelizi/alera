import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/features/runtime/application/host_connection_controller.dart';
import 'package:alera_mobile/src/features/runtime/domain/mobile_codex_workspace.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_picker_sheet.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_file_viewer_screen.dart';
import 'package:alera_mobile/src/features/workbench/presentation/workspace_text_create_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:logging/logging.dart';

/// Runtime-backed directory navigation. The server scans a single directory,
/// pages at 100 entries and never follows symlinks into another worktree.
class WorkspaceDirectoryBrowserScreen extends ConsumerStatefulWidget {
  const WorkspaceDirectoryBrowserScreen({
    super.key,
    required this.hostId,
    required this.workspaceId,
    required this.workspaceName,
  });

  final String hostId;
  final String workspaceId;
  final String workspaceName;

  @override
  ConsumerState<WorkspaceDirectoryBrowserScreen> createState() =>
      _WorkspaceDirectoryBrowserScreenState();
}

class _WorkspaceDirectoryBrowserScreenState
    extends ConsumerState<WorkspaceDirectoryBrowserScreen> {
  static final Logger _logger = Logger('MobileDirectoryBrowser');
  final List<MobileWorkspaceDirectoryEntry> _items = [];
  String _directory = '';
  int _nextOffset = 0;
  int _generation = 0;
  bool _showHidden = false;
  bool _hasMore = false;
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
    if (_directory.isEmpty) return '';
    final index = _directory.lastIndexOf('/');
    return index < 0 ? '' : _directory.substring(0, index);
  }

  void _navigate(String directory) {
    _directory = directory;
    _load(reset: true);
  }

  Future<void> _openFile(String relativePath) async {
    await Navigator.of(context).push<void>(
      MaterialPageRoute<void>(
        builder: (_) => WorkspaceFileViewerScreen(
          hostId: widget.hostId,
          workspaceId: widget.workspaceId,
          relativePath: relativePath,
        ),
      ),
    );
    if (mounted) await _load(reset: true);
  }

  Future<void> _quickOpen() async {
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      if (!mounted) return;
      final path = await showWorkspaceFilePickerSheet(
        context,
        start: () => client.startWorkspaceQuickOpen(widget.workspaceId),
        search: (session, query) =>
            client.searchWorkspaceQuickOpen(session, query, limit: 100),
        stop: client.stopWorkspaceQuickOpen,
      );
      if (!mounted || path == null) return;
      await _openFile(path);
    } on Object catch (error, trace) {
      _logger.warning('Quick Open failed', error, trace);
      if (mounted) setState(() => _error = error.toString());
    }
  }

  Future<void> _createTextFile() async {
    final path = await Navigator.of(context).push<String>(
      MaterialPageRoute<String>(
        builder: (_) => WorkspaceTextCreateScreen(
          hostId: widget.hostId,
          workspaceId: widget.workspaceId,
          directory: _directory,
        ),
      ),
    );
    if (!mounted || path == null) return;
    await _load(reset: true);
    if (!mounted) return;
    await _openFile(path);
  }

  Future<void> _load({required bool reset}) async {
    if (_loading && !reset) return;
    final generation = reset ? ++_generation : _generation;
    final directory = _directory;
    final offset = reset ? 0 : _nextOffset;
    setState(() {
      _loading = true;
      _error = null;
      if (reset) {
        _items.clear();
        _nextOffset = 0;
        _hasMore = false;
      }
    });
    try {
      final client = await ref.read(
        hostConnectionControllerProvider(widget.hostId).future,
      );
      final result = await client.listWorkspaceDirectory(
        workspaceId: widget.workspaceId,
        relativePath: directory,
        offset: offset,
        limit: 100,
        showHidden: _showHidden,
      );
      if (!mounted || generation != _generation) return;
      setState(() {
        _items.addAll(result.items);
        _nextOffset = result.nextOffset;
        _hasMore = result.hasMore;
      });
    } on Object catch (error, trace) {
      _logger.warning('Workspace directory listing failed', error, trace);
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
    final canCreate =
        ref
            .watch(hostConnectionControllerProvider(widget.hostId))
            .value
            ?.supportsWorkspaceTextCreate ==
        true;
    return Scaffold(
      appBar: AppBar(
        title: Text(
          _directory.isEmpty ? widget.workspaceName : _directory,
          overflow: TextOverflow.ellipsis,
        ),
        leading: _directory.isNotEmpty
            ? IconButton(
                tooltip: 'Parent folder',
                onPressed: () => _navigate(_parent),
                icon: const Icon(Icons.arrow_upward),
              )
            : null,
        actions: <Widget>[
          if (canCreate)
            IconButton(
              tooltip: 'New text file',
              icon: const Icon(Icons.note_add_outlined),
              onPressed: _createTextFile,
            ),
          IconButton(
            tooltip: 'Quick Open File',
            icon: const Icon(Icons.search),
            onPressed: _quickOpen,
          ),
          IconButton(
            tooltip: _showHidden ? 'Hide hidden files' : 'Show hidden files',
            icon: Icon(_showHidden ? Icons.visibility : Icons.visibility_off),
            onPressed: () {
              _showHidden = !_showHidden;
              _load(reset: true);
            },
          ),
          IconButton(
            tooltip: 'Refresh Folder',
            onPressed: () => _load(reset: true),
            icon: const Icon(Icons.refresh),
          ),
        ],
      ),
      body: SafeArea(
        child: RefreshIndicator(
          onRefresh: () => _load(reset: true),
          child: ListView(
            physics: const AlwaysScrollableScrollPhysics(),
            children: <Widget>[
              Padding(
                padding: AleraTokens.contentPadding,
                child: Text(
                  _directory.isEmpty ? '/' : '/$_directory',
                  style: Theme.of(context).textTheme.bodySmall,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (_directory.isNotEmpty)
                ListTile(
                  leading: const Icon(Icons.drive_folder_upload_outlined),
                  title: const Text('..'),
                  onTap: () => _navigate(_parent),
                ),
              for (final entry in _items)
                ListTile(
                  key: ValueKey<String>(entry.relativePath),
                  leading: Icon(
                    entry.isDirectory
                        ? Icons.folder_outlined
                        : Icons.insert_drive_file_outlined,
                  ),
                  title: Text(
                    entry.name,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  subtitle: entry.isDirectory
                      ? null
                      : Text('${entry.sizeBytes} bytes'),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () {
                    if (entry.isDirectory) {
                      _navigate(entry.relativePath);
                    } else {
                      _openFile(entry.relativePath);
                    }
                  },
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
                  onTap: () => _load(reset: _items.isEmpty),
                ),
              if (!_loading && _hasMore)
                ListTile(
                  title: const Text('Load more files'),
                  leading: const Icon(Icons.expand_more),
                  onTap: () => _load(reset: false),
                ),
              if (!_loading && _error == null && _items.isEmpty)
                const ListTile(title: Text('This folder is empty')),
            ],
          ),
        ),
      ),
    );
  }
}
