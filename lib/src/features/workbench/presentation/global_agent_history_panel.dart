import 'dart:convert';

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/workbench/application/global_agent_history_retry.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:uuid/uuid.dart';

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
  final _resumeProfile = TextEditingController();
  bool _consented = false;
  bool _busy = false;
  String? _provider;
  String? _armedResumeKey;
  final _resumeMutations = ExactHistoryResumeRetryTracker();

  /// Only verified Host-backed Tab identities can be offered for navigation.
  final Map<String, ({String workspaceId, String tabId})> _locatedTabs = {};
  String _feedback =
      'Search is off until you explicitly enable local indexing.';
  List<Map<String, Object?>> _hits = const [];

  @override
  void dispose() {
    _query.dispose();
    _project.dispose();
    _workspace.dispose();
    _account.dispose();
    _resumeProfile.dispose();
    super.dispose();
  }

  String? _optional(TextEditingController controller) {
    final value = controller.text.trim();
    return value.isEmpty ? null : value;
  }

  String _sourceKey(Map<String, Object?> hit) {
    final source = hit['session'];
    if (source is! Map) return '';
    return jsonEncode([
      for (final field in [
        'provider',
        'providerAccountId',
        'projectId',
        'workspaceId',
        'providerSessionId',
      ])
        source[field]?.toString() ?? '',
    ]);
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
      _armedResumeKey = null;
      _locatedTabs.clear();
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
        _armedResumeKey = null;
        _locatedTabs.clear();
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

  /// A deliberate read-only Host lookup, useful after the UI was restarted
  /// and its in-memory mutation UUID is no longer available.
  Future<void> _locateExisting(Map<String, Object?> hit) async {
    if (_busy || !_consented) return;
    final source = hit['session'];
    if (source is! Map) return;
    String field(String key) => source[key]?.toString() ?? '';
    final provider = field('provider');
    final account = field('providerAccountId');
    final project = field('projectId');
    final workspace = field('workspaceId');
    final nativeId = field('providerSessionId');
    final sourceKey = _sourceKey(hit);
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostAgentHistoryClient ||
        account.isEmpty ||
        !['claude', 'codex'].contains(provider)) {
      setState(
        () => _feedback = 'Verified source account or Host lookup unavailable.',
      );
      return;
    }
    setState(() {
      _busy = true;
      _armedResumeKey = null;
      _feedback = 'Looking for an existing verified Tab; no Agent will start…';
    });
    try {
      final reply = await (host as TerminalHostAgentHistoryClient)
          .locateExactAgentHistory(
            provider: provider,
            accountId: account,
            projectId: project,
            workspaceId: workspace,
            providerSessionId: nativeId,
          );
      if (!mounted) return;
      if (reply['noAgentStarted'] != true) {
        throw const FormatException('Unverified Host lookup response');
      }
      final tabId = reply['tabId']?.toString();
      final matchedWorkspace = reply['workspaceId']?.toString();
      setState(() {
        final found =
            reply['found'] == true &&
            tabId != null &&
            tabId.isNotEmpty &&
            tabId.length <= 256 &&
            matchedWorkspace == workspace;
        if (found) {
          _locatedTabs[sourceKey] = (workspaceId: workspace, tabId: tabId);
        } else {
          _locatedTabs.remove(sourceKey);
        }
        _feedback = found
            ? 'Existing Tab $tabId found in Worktree $workspace. '
                  'Select Open verified Tab to navigate; no Agent was started.'
            : reply['found'] == false
            ? 'No verified existing Tab found. You can explicitly Resume '
                  'after confirming the source account.'
            : 'Host lookup returned an unexpected identity; no action taken.';
      });
    } catch (_) {
      if (!mounted) return;
      setState(
        () => _feedback = 'Cannot confirm an existing Tab on this Host. No Agent was started.',
      );
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  /// Navigation is a separate, explicit action from read-only Find.
  /// Recheck the Host tuple to prevent a stale search result selecting
  /// a different saved Tab after the account or Worktree changes.
  Future<void> _openLocatedExisting(Map<String, Object?> hit) async {
    if (_busy || !_consented) return;
    final source = hit['session'];
    if (source is! Map) return;
    final key = _sourceKey(hit);
    final located = _locatedTabs[key];
    final host = ref.read(terminalHostClientProvider);
    if (located == null || host is! TerminalHostAgentHistoryClient) return;
    String field(String name) => source[name]?.toString() ?? '';
    setState(() {
      _busy = true;
      _feedback = 'Rechecking the saved Tab before navigation…';
    });
    try {
      final reply = await (host as TerminalHostAgentHistoryClient)
          .locateExactAgentHistory(
            provider: field('provider'),
            accountId: field('providerAccountId'),
            projectId: field('projectId'),
            workspaceId: field('workspaceId'),
            providerSessionId: field('providerSessionId'),
          );
      if (reply['noAgentStarted'] != true ||
          reply['found'] != true ||
          reply['tabId'] != located.tabId ||
          reply['workspaceId'] != located.workspaceId) {
        throw const FormatException('Saved Tab identity changed');
      }
      if (!mounted) return;
      await ref
          .read(workbenchControllerProvider.notifier)
          .openPersistedWorkspaceTab(
            workspaceId: located.workspaceId,
            tabId: located.tabId,
          );
      if (!mounted) return;
      final state = ref.read(workbenchControllerProvider);
      if (state.activeWorkspaceId != located.workspaceId ||
          state.activeWorkspaceTab?.id != located.tabId) {
        throw StateError('Saved Tab could not be selected');
      }
      setState(() {
        _feedback =
            'Opened verified Tab in the original Worktree. '
            'No duplicate Agent Tab was created.';
      });
    } catch (_) {
      if (!mounted) return;
      setState(() {
        _locatedTabs.remove(key);
        _feedback =
            'Saved Tab changed or could not be opened. '
            'Find it again; no duplicate Resume was attempted.';
      });
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _resumeExact(Map<String, Object?> hit) async {
    if (_busy || !_consented) return;
    final source = hit['session'];
    if (source is! Map) return;
    String field(String key) => source[key]?.toString() ?? '';
    final provider = field('provider');
    final account = field('providerAccountId');
    final project = field('projectId');
    final workspace = field('workspaceId');
    final nativeId = field('providerSessionId');
    final profile = _resumeProfile.text.trim();
    if (account.isEmpty || !['claude', 'codex'].contains(provider)) {
      setState(
        () => _feedback =
            'Exact Resume requires a known Claude/Codex source account.',
      );
      return;
    }
    if (profile.isEmpty) {
      setState(
        () => _feedback = 'Enter the matching official Claude/Codex Profile ID from Settings.',
      );
      return;
    }
    // JSON-encode the complete tuple rather than joining untrusted Host
    // fields with a delimiter that may itself appear in an account label.
    final identity = jsonEncode([
      provider,
      account,
      project,
      workspace,
      nativeId,
      profile,
    ]);
    final resumeIdentity = (
      provider: provider,
      accountId: account,
      projectId: project,
      workspaceId: workspace,
      providerSessionId: nativeId,
      profileId: profile,
    );
    if (_armedResumeKey != identity) {
      final previousOutcomeUnknown = _resumeMutations.hasUnconfirmed(
        resumeIdentity,
      );
      setState(() {
        _armedResumeKey = identity;
        _feedback = previousOutcomeUnknown
            ? 'Previous Resume acknowledgement was unconfirmed. Inspect Live '
                  'Agents first; then confirm this SAME result again to resend '
                  'the ORIGINAL request ID. No duplicate Agent is created.'
            : 'Alera will select the configured $provider account "$account" '
                  'and verify this Session again. Confirm exact Resume on '
                  'the SAME result to create a Tab. No old prompt is replayed; '
                  'the provider may reject an incompatible login.';
      });
      return;
    }
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostAgentHistoryClient) return;
    setState(() {
      _armedResumeKey = null;
      _busy = true;
      _feedback = 'Rechecking the native source before exact Resume…';
    });
    // Keep the same UUID if the Host accepted a previous attempt but the ACK
    // was lost. A new UUID would be rejected as a duplicate session owner.
    final mutationId = _resumeMutations.reserve(
      resumeIdentity,
      () => const Uuid().v4(),
    );
    try {
      final reply = await (host as TerminalHostAgentHistoryClient)
          .resumeExactAgentHistory(
            provider: provider,
            accountId: account,
            projectId: project,
            workspaceId: workspace,
            providerSessionId: nativeId,
            profileId: profile,
            clientMutationId: mutationId,
          );
      if (!mounted) return;
      final tab = reply['tab'];
      final tabId = tab is Map ? tab['id']?.toString() : null;
      if (tabId != null) _resumeMutations.acknowledge(resumeIdentity);
      setState(
        () => _feedback = tabId == null
            ? 'Host reply did not confirm a Tab. Inspect Live Agents.'
            : 'Exact Resume Tab $tabId created. Open it from Live Agents. '
                  'Do not retry while the CLI login or PTY is connecting.',
      );
    } catch (_) {
      if (!mounted) return;
      setState(
        () => _feedback =
            'Exact Resume failed or its acknowledgement was ambiguous. '
            'Inspect Live Agents first; a deliberate retry on this SAME '
            'result will reuse the original request ID.',
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
        TextField(
          controller: _resumeProfile,
          decoration: const InputDecoration(
            labelText: 'Official Claude/Codex Profile ID (exact Resume)',
            helperText:
                'Host uses the source account directory automatically. '
                'Only official CLI Profiles, no custom wrappers.',
          ),
          onChanged: (_) => setState(() => _armedResumeKey = null),
        ),
        CheckboxListTile(
          dense: true,
          contentPadding: EdgeInsets.zero,
          value: _consented,
          onChanged: _busy
              ? null
              : (value) => setState(() {
                  _consented = value == true;
                  if (!_consented) {
                    _hits = const [];
                    _armedResumeKey = null;
                    _locatedTabs.clear();
                  }
                }),
          title: const Text(
            'Allow local indexing of limited user-message previews',
          ),
          subtitle: const Text(
            'Explicit opt-in per view. Stored in a private local '
            'SQLite cache (not encrypted). Manual Resume selects the '
            'verified source account. No cloud index upload.',
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
                    final resumeKey = jsonEncode([
                      field('provider'),
                      field('providerAccountId'),
                      field('projectId'),
                      field('workspaceId'),
                      field('providerSessionId'),
                      _resumeProfile.text.trim(),
                    ]);
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
                            field('providerAccountId').isNotEmpty
                                ? 'Matched ${hit['matchField']} · Select Resume to '
                                      'verify the source account, UUID, and Worktree'
                                : 'Preview only · source account unavailable',
                            style: muted,
                          ),
                          if (field('providerAccountId').isNotEmpty)
                            Align(
                              alignment: Alignment.centerRight,
                              child: Wrap(
                                spacing: 8,
                                children: [
                                  TextButton.icon(
                                    onPressed: _busy || !_consented
                                        ? null
                                        : () => _locateExisting(hit),
                                    icon: const Icon(
                                      Icons.find_in_page_outlined,
                                    ),
                                    label: const Text('Find existing Tab'),
                                  ),
                                  if (_locatedTabs.containsKey(_sourceKey(hit)))
                                    TextButton.icon(
                                      onPressed: _busy || !_consented
                                          ? null
                                          : () => _openLocatedExisting(hit),
                                      icon: const Icon(Icons.open_in_new),
                                      label: const Text('Open verified Tab'),
                                    ),
                                  TextButton.icon(
                                    onPressed: _busy || !_consented
                                        ? null
                                        : () => _resumeExact(hit),
                                    icon: const Icon(Icons.history),
                                    label: Text(
                                      _armedResumeKey == resumeKey
                                          ? 'Confirm exact Resume'
                                          : 'Resume exact Session…',
                                    ),
                                  ),
                                ],
                              ),
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
