import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// A deliberate, local-only historical search surface. Never index while
/// typing or automatically opt users into storing transcript text.
class GlobalAgentHistoryPanel extends ConsumerStatefulWidget {
  const GlobalAgentHistoryPanel({super.key});

  @override
  ConsumerState<GlobalAgentHistoryPanel> createState() =>
      _GlobalAgentHistoryPanelState();
}

class _GlobalAgentHistoryPanelState
    extends ConsumerState<GlobalAgentHistoryPanel> {
  final _query = TextEditingController();
  final _project = TextEditingController();
  final _workspace = TextEditingController();
  final _account = TextEditingController();
  bool _consented = false;
  bool _busy = false;
  String? _provider;
  String _feedback =
      'Search is off until you explicitly enable local indexing.';
  List<Map<String, Object?>> _hits = const [];

  @override
  void dispose() {
    _query.dispose();
    _project.dispose();
    _workspace.dispose();
    _account.dispose();
    super.dispose();
  }

  String? _optional(TextEditingController controller) {
    final value = controller.text.trim();
    return value.isEmpty ? null : value;
  }

  Future<void> _searchHistory() async {
    if (_busy || !_consented) return;
    final text = _query.text.trim();
    if (text.isEmpty || text.runes.length > 256) {
      setState(() => _feedback = 'Enter 1–256 characters to search.');
      return;
    }
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostAgentHistoryClient) {
      setState(
        () =>
            _feedback = 'This Host client does not support historical search.',
      );
      return;
    }
    setState(() {
      _busy = true;
      _feedback = 'Searching verified workspaces; no Agent will be started…';
    });
    try {
      final result = await (host as TerminalHostAgentHistoryClient)
          .searchAgentHistory(
            text: text,
            projectId: _optional(_project),
            workspaceId: _optional(_workspace),
            provider: _provider,
            accountId: _optional(_account),
          );
      if (!mounted) return;
      final rawHits = result['hits'];
      final hits = rawHits is List
          ? rawHits
                .whereType<Map>()
                .map(
                  (value) => value.map(
                    (key, value) => MapEntry(key.toString(), value),
                  ),
                )
                .toList()
          : <Map<String, Object?>>[];
      final truncated = result['scanTruncated'] == true;
      setState(() {
        _hits = hits;
        _feedback =
            '${hits.length} preview-only match(es) found.'
            '${truncated ? ' Scan budget reached; results may be incomplete.' : ''}';
      });
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _hits = const [];
        _feedback =
            'History search failed or Host is unavailable. '
            'No Agent was started.';
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _clearCache() async {
    if (_busy) return;
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostAgentHistoryClient) {
      setState(() => _feedback = 'This Host does not support cache clearing.');
      return;
    }
    setState(() => _busy = true);
    try {
      await (host as TerminalHostAgentHistoryClient)
          .clearAgentHistorySearchCache();
      if (!mounted) return;
      setState(() {
        _hits = const [];
        _consented = false;
        _feedback = 'Local search index cleared. Native Claude/Codex files were not changed.';
      });
    } catch (_) {
      if (!mounted) return;
      setState(
        () =>
            _feedback = 'Could not confirm cache deletion. Check Host status.',
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        TextField(
          controller: _query,
          autofocus: true,
          maxLength: 256,
          decoration: const InputDecoration(
            prefixIcon: Icon(Icons.manage_search),
            labelText: 'Historical Claude / Codex Sessions',
            hintText: 'Windows 注音, IMM API, git diff…',
          ),
          onSubmitted: (_) => _searchHistory(),
        ),
        Row(
          children: [
            Expanded(
              child: DropdownButtonFormField<String>(
                initialValue: _provider,
                decoration: const InputDecoration(labelText: 'Provider'),
                items: const [
                  DropdownMenuItem(value: null, child: Text('All Providers')),
                  DropdownMenuItem(value: 'claude', child: Text('Claude')),
                  DropdownMenuItem(value: 'codex', child: Text('Codex')),
                ],
                onChanged: _busy
                    ? null
                    : (value) => setState(() => _provider = value),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                controller: _account,
                decoration: const InputDecoration(
                  labelText: 'Account ID (optional)',
                ),
              ),
            ),
          ],
        ),
        const SizedBox(height: 8),
        Row(
          children: [
            Expanded(
              child: TextField(
                controller: _project,
                decoration: const InputDecoration(
                  labelText: 'Project ID (optional)',
                ),
              ),
            ),
            const SizedBox(width: 8),
            Expanded(
              child: TextField(
                controller: _workspace,
                decoration: const InputDecoration(
                  labelText: 'Worktree ID (optional)',
                ),
              ),
            ),
          ],
        ),
        CheckboxListTile(
          dense: true,
          contentPadding: EdgeInsets.zero,
          value: _consented,
          onChanged: _busy
              ? null
              : (value) => setState(() {
                  _consented = value == true;
                  if (!_consented) _hits = const [];
                }),
          title: const Text(
            'Allow local indexing of limited user-message previews',
          ),
          subtitle: const Text(
            'Explicit opt-in per view. Stored in a private local '
            'SQLite cache (not encrypted). No cloud upload or automatic resume.',
          ),
        ),
        Row(
          children: [
            FilledButton.icon(
              onPressed: _busy || !_consented ? null : _searchHistory,
              icon: const Icon(Icons.search),
              label: Text(_busy ? 'Working…' : 'Search History'),
            ),
            const SizedBox(width: 12),
            TextButton.icon(
              onPressed: _busy ? null : _clearCache,
              icon: const Icon(Icons.delete_outline),
              label: const Text('Clear local index'),
            ),
          ],
        ),
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 6),
          child: Text(_feedback, style: muted),
        ),
        const Divider(),
        Expanded(
          child: _hits.isEmpty
              ? Center(
                  child: Text(
                    'No historical previews to display.',
                    style: muted,
                  ),
                )
              : ListView.separated(
                  itemCount: _hits.length,
                  separatorBuilder: (_, _) => const SizedBox(height: 8),
                  itemBuilder: (_, index) {
                    final hit = _hits[index];
                    final session = hit['session'];
                    final data = session is Map
                        ? session
                        : const <String, Object?>{};
                    String field(String key) => data[key]?.toString() ?? '';
                    final goal = field('goal');
                    return Container(
                      decoration: BoxDecoration(
                        color: AleraTokens.surface,
                        border: Border.all(color: AleraTokens.borderSubtle),
                        borderRadius: BorderRadius.circular(
                          AleraTokens.radiusMd,
                        ),
                      ),
                      padding: const EdgeInsets.all(12),
                      child: Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          Text(
                            field('title'),
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: Theme.of(context).textTheme.titleSmall,
                          ),
                          const SizedBox(height: 4),
                          Text(
                            '${field('provider')} · ${field('providerAccountId')} · '
                            '${field('projectId')} / ${field('workspaceId')}',
                            maxLines: 2,
                            overflow: TextOverflow.ellipsis,
                            style: muted,
                          ),
                          if (goal.isNotEmpty)
                            Text(
                              goal,
                              maxLines: 3,
                              overflow: TextOverflow.ellipsis,
                            ),
                          Text(
                            'Preview only · matched ${hit['matchField']} · '
                            'Resume requires separately verified native identity',
                            style: muted,
                          ),
                        ],
                      ),
                    );
                  },
                ),
        ),
      ],
    );
  }
}
