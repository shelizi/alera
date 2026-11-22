import 'dart:convert';
import 'dart:math' as math;

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/workbench/application/global_agent_history_retry.dart';
import 'package:alera/src/features/workbench/application/global_agent_dashboard_navigation.dart';
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
  bool _advancedFiltersOpen = false;
  String? _resumeExpandedKey;
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

  /// Search remains on-demand. Changing a scope must never leave actions
  /// from a previous account/Worktree attached to stale results.
  void _invalidateSearchResults() {
    _hits = const <Map<String, Object?>>[];
    _locatedTabs.clear();
    _armedResumeKey = null;
    _resumeExpandedKey = null;
    _feedback = _consented
        ? 'Search changed. Press Search History to refresh results.'
        : 'Search is off until you explicitly enable local indexing.';
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
      _resumeExpandedKey = null;
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
        _resumeExpandedKey = null;
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
      ref.read(globalAgentDashboardNavigationProvider.notifier).close();
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

  Future<void> _confirmClearCache() async {
    if (_busy) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Clear local history index?'),
        content: const Text(
          'This removes only the private local search cache. '
          'Claude and Codex source conversations will not be changed.',
        ),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(context).pop(false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.of(context).pop(true),
            child: const Text('Clear index'),
          ),
        ],
      ),
    );
    if (confirmed == true && mounted) await _clearCache();
  }

  Widget _providerChip(String? value, String label) => ChoiceChip(
    label: Text(label),
    selected: _provider == value,
    onSelected: _busy
        ? null
        : (selected) {
            if (selected) {
              setState(() {
                _provider = value;
                _invalidateSearchResults();
              });
            }
          },
  );

  Widget _searchControls() => LayoutBuilder(
    builder: (context, constraints) {
      final field = TextField(
        controller: _query,
        key: const ValueKey<String>('history-search-query'),
        enabled: !_busy,
        autofocus: true,
        maxLength: 256,
        textInputAction: TextInputAction.search,
        decoration: InputDecoration(
          prefixIcon: const Icon(Icons.manage_search),
          labelText: 'Search historical Claude / Codex Sessions',
          hintText: 'Search by task, code, goal or progress',
          counterText: '',
          isDense: true,
          border: const OutlineInputBorder(),
          suffixIcon: ValueListenableBuilder<TextEditingValue>(
            valueListenable: _query,
            builder: (_, value, _) => value.text.isEmpty
                ? const Icon(Icons.keyboard_return_outlined, size: 18)
                : IconButton(
                    tooltip: 'Clear search text',
                    icon: const Icon(Icons.close),
                    onPressed: _busy ? null : () => _query.clear(),
                  ),
          ),
        ),
        onChanged: (_) => setState(_invalidateSearchResults),
        onSubmitted: (_) => _searchHistory(),
      );
      final button = FilledButton.icon(
        onPressed: _busy || !_consented ? null : _searchHistory,
        icon: const Icon(Icons.search),
        label: const Text('Search History'),
      );
      if (constraints.maxWidth < 620) {
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: <Widget>[field, const SizedBox(height: 8), button],
        );
      }
      return Row(
        children: <Widget>[
          Expanded(child: field),
          const SizedBox(width: 10),
          button,
        ],
      );
    },
  );

  Widget _advancedFilters() => LayoutBuilder(
    builder: (context, constraints) {
      final width = math.min(310.0, constraints.maxWidth);
      Widget field(TextEditingController controller, String label) => SizedBox(
        width: width,
        child: TextField(
          controller: controller,
          enabled: !_busy,
          maxLength: 256,
          decoration: InputDecoration(
            labelText: label,
            counterText: '',
            isDense: true,
            border: const OutlineInputBorder(),
          ),
          onChanged: (_) => setState(_invalidateSearchResults),
          onSubmitted: (_) => _searchHistory(),
        ),
      );
      return Container(
        key: const ValueKey<String>('history-advanced-filters'),
        padding: const EdgeInsets.all(12),
        decoration: BoxDecoration(
          border: Border.all(color: AleraTokens.borderSubtle),
          borderRadius: BorderRadius.circular(8),
        ),
        child: Wrap(
          spacing: 10,
          runSpacing: 8,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: <Widget>[
            field(_project, 'Project ID'),
            field(_workspace, 'Worktree ID'),
            field(_account, 'Source account ID'),
            TextButton.icon(
              onPressed: _busy
                  ? null
                  : () {
                      _project.clear();
                      _workspace.clear();
                      _account.clear();
                      setState(() {
                        _provider = null;
                        _invalidateSearchResults();
                      });
                    },
              icon: const Icon(Icons.filter_alt_off_outlined, size: 18),
              label: const Text('Reset filters'),
            ),
          ],
        ),
      );
    },
  );

  Widget _resultCard(BuildContext context, Map<String, Object?> hit) {
    final session = hit['session'];
    final data = session is Map ? session : const <String, Object?>{};
    String field(String key) => data[key]?.toString() ?? '';
    final sourceKey = _sourceKey(hit);
    final located = _locatedTabs.containsKey(sourceKey);
    final expanded = _resumeExpandedKey == sourceKey;
    final canLocate =
        field('providerAccountId').isNotEmpty &&
        const <String>['claude', 'codex'].contains(field('provider'));
    final goal = field('goal');
    final progress = field('progress');
    final title = field('title').trim().isEmpty
        ? 'Untitled Agent Session'
        : field('title');
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    final resumeKey = jsonEncode(<String>[
      field('provider'),
      field('providerAccountId'),
      field('projectId'),
      field('workspaceId'),
      field('providerSessionId'),
      _resumeProfile.text.trim(),
    ]);
    final matchField = hit['matchField']?.toString() ?? 'history';

    return Container(
      key: ValueKey<String>('history-result-$sourceKey'),
      padding: const EdgeInsets.all(13),
      decoration: BoxDecoration(
        color: AleraTokens.surface,
        border: Border.all(
          color: located
              ? AleraTokens.foregroundMuted
              : AleraTokens.borderSubtle,
        ),
        borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          Row(
            children: <Widget>[
              Icon(
                field('provider') == 'claude'
                    ? Icons.smart_toy_outlined
                    : Icons.code_outlined,
                size: 20,
              ),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  title,
                  style: Theme.of(context).textTheme.titleSmall,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              if (located)
                const Tooltip(
                  message: 'The original Host Tab was found and verified',
                  child: Icon(Icons.verified_outlined, size: 19),
                ),
            ],
          ),
          const SizedBox(height: 6),
          Text(
            '${field('provider').toUpperCase()} · ${field('providerAccountId').isEmpty ? 'Unknown account' : field('providerAccountId')}',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: muted,
          ),
          Text(
            '${field('projectId')} / ${field('workspaceId')}',
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
            style: muted,
          ),
          if (goal.isNotEmpty) ...<Widget>[
            const SizedBox(height: 8),
            Text(goal, maxLines: 3, overflow: TextOverflow.ellipsis),
          ],
          if (progress.isNotEmpty) ...<Widget>[
            const SizedBox(height: 5),
            Text(
              progress,
              maxLines: 2,
              overflow: TextOverflow.ellipsis,
              style: muted,
            ),
          ],
          const SizedBox(height: 7),
          Text(
            canLocate
                ? 'Matched: $matchField · Preview only (no Agent started)'
                : 'Preview only · Exact source account unavailable',
            style: muted,
          ),
          if (canLocate) ...<Widget>[
            const SizedBox(height: 7),
            Wrap(
              spacing: 8,
              runSpacing: 4,
              crossAxisAlignment: WrapCrossAlignment.center,
              children: <Widget>[
                if (!located)
                  OutlinedButton.icon(
                    onPressed: _busy || !_consented
                        ? null
                        : () => _locateExisting(hit),
                    icon: const Icon(Icons.find_in_page_outlined, size: 18),
                    label: const Text('Find existing Tab'),
                  )
                else
                  FilledButton.icon(
                    onPressed: _busy || !_consented
                        ? null
                        : () => _openLocatedExisting(hit),
                    icon: const Icon(Icons.open_in_new, size: 18),
                    label: const Text('Open verified Tab'),
                  ),
                TextButton.icon(
                  onPressed: _busy || !_consented
                      ? null
                      : () => setState(() {
                          _resumeExpandedKey = expanded ? null : sourceKey;
                          _armedResumeKey = null;
                          if (!expanded) _resumeProfile.clear();
                        }),
                  icon: Icon(
                    expanded ? Icons.keyboard_arrow_up : Icons.history_outlined,
                    size: 18,
                  ),
                  label: Text(expanded ? 'Hide Resume' : 'Resume Session…'),
                ),
              ],
            ),
            if (expanded) ...<Widget>[
              const SizedBox(height: 8),
              Container(
                key: const ValueKey<String>('history-resume-details'),
                padding: const EdgeInsets.all(12),
                decoration: BoxDecoration(
                  border: Border.all(color: AleraTokens.borderSubtle),
                  borderRadius: BorderRadius.circular(8),
                ),
                child: Column(
                  crossAxisAlignment: CrossAxisAlignment.stretch,
                  children: <Widget>[
                    const Text(
                      'Resume creates a new Tab only after identity verification. '
                      'Find an existing Tab first to avoid duplicates.',
                    ),
                    const SizedBox(height: 8),
                    TextField(
                      controller: _resumeProfile,
                      enabled: !_busy,
                      decoration: const InputDecoration(
                        labelText: 'Official Claude / Codex Profile ID',
                        hintText: 'Exact Profile ID from Settings',
                        isDense: true,
                        border: OutlineInputBorder(),
                      ),
                      onChanged: (_) => setState(() => _armedResumeKey = null),
                    ),
                    const SizedBox(height: 8),
                    Align(
                      alignment: Alignment.centerRight,
                      child: OutlinedButton.icon(
                        onPressed: _busy || !_consented
                            ? null
                            : () => _resumeExact(hit),
                        icon: const Icon(Icons.play_arrow_outlined, size: 18),
                        label: Text(
                          _armedResumeKey == resumeKey
                              ? 'Confirm exact Resume'
                              : 'Resume exact Session…',
                        ),
                      ),
                    ),
                  ],
                ),
              ),
            ],
          ],
        ],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    final activeFilters = <String>[
      if (_project.text.trim().isNotEmpty) 'Project',
      if (_workspace.text.trim().isNotEmpty) 'Worktree',
      if (_account.text.trim().isNotEmpty) 'Account',
    ].length;

    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        _searchControls(),
        const SizedBox(height: 9),
        Wrap(
          spacing: 8,
          runSpacing: 4,
          crossAxisAlignment: WrapCrossAlignment.center,
          children: <Widget>[
            _providerChip(null, 'All'),
            _providerChip('claude', 'Claude'),
            _providerChip('codex', 'Codex'),
            TextButton.icon(
              onPressed: () =>
                  setState(() => _advancedFiltersOpen = !_advancedFiltersOpen),
              icon: Icon(
                _advancedFiltersOpen ? Icons.expand_less : Icons.tune_outlined,
                size: 18,
              ),
              label: Text(
                activeFilters == 0
                    ? 'Advanced filters'
                    : 'Advanced filters ($activeFilters)',
              ),
            ),
          ],
        ),
        if (_advancedFiltersOpen) ...<Widget>[
          const SizedBox(height: 8),
          _advancedFilters(),
        ],
        CheckboxListTile(
          dense: true,
          contentPadding: EdgeInsets.zero,
          controlAffinity: ListTileControlAffinity.leading,
          value: _consented,
          onChanged: _busy
              ? null
              : (value) => setState(() {
                  _consented = value == true;
                  if (!_consented) {
                    _hits = const <Map<String, Object?>>[];
                    _armedResumeKey = null;
                    _resumeExpandedKey = null;
                    _locatedTabs.clear();
                  }
                }),
          title: const Text(
            'Allow local indexing of limited user-message previews',
          ),
          subtitle: const Text(
            'Opt in per view · private local SQLite (not encrypted) · no cloud upload',
            maxLines: 2,
          ),
        ),
        if (_busy) const LinearProgressIndicator(minHeight: 2),
        Padding(
          padding: const EdgeInsets.symmetric(vertical: 6),
          child: Text(_feedback, style: muted),
        ),
        Row(
          children: <Widget>[
            Expanded(
              child: Text(
                '${_hits.length} history results',
                style: Theme.of(context).textTheme.titleSmall,
              ),
            ),
            TextButton.icon(
              onPressed: _busy ? null : _confirmClearCache,
              icon: const Icon(Icons.delete_outline, size: 18),
              label: const Text('Clear local index'),
            ),
          ],
        ),
        const Divider(height: 12),
        Expanded(
          child: _hits.isEmpty
              ? Center(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: <Widget>[
                      const Icon(Icons.history_toggle_off_outlined, size: 34),
                      const SizedBox(height: 12),
                      Text(
                        !_consented
                            ? 'Enable local preview indexing to search history.'
                            : 'Search to find a past Claude or Codex Session.',
                        textAlign: TextAlign.center,
                        style: muted,
                      ),
                    ],
                  ),
                )
              : ListView.separated(
                  key: const ValueKey<String>('history-results-list'),
                  itemCount: _hits.length,
                  separatorBuilder: (_, _) => const SizedBox(height: 10),
                  itemBuilder: (context, index) =>
                      _resultCard(context, _hits[index]),
                ),
        ),
      ],
    );
  }
}
