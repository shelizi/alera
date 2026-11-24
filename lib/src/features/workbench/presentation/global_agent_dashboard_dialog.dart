import 'dart:convert';

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/global_agent_dashboard_navigation.dart';
import 'package:alera/src/features/workbench/application/global_agent_live_projection.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_launch_dialog.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_history_panel.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

typedef _DashboardCardState = ({
  GlobalAgentLiveRow row,
  bool expanded,
  bool hidden,
  bool historyBusy,
  bool controlsDisabled,
  bool armedInterrupt,
  bool armedTerminate,
});

/// Global scope is deliberate: this watches the full Workbench inventory and
/// current Agent map, not the selected project's subset. This page lives in
/// the center workbench; the sidebar and existing Tab state remain mounted.
class GlobalAgentDashboardPage extends ConsumerStatefulWidget {
  const GlobalAgentDashboardPage({super.key});

  @override
  ConsumerState<GlobalAgentDashboardPage> createState() =>
      _GlobalAgentDashboardPageState();
}

class _GlobalAgentDashboardPageState
    extends ConsumerState<GlobalAgentDashboardPage> {
  String _query = '';
  bool _historyMode = false;
  bool _needsReviewOnly = false;
  final TextEditingController _instruction = TextEditingController();
  ({String sessionId, String workspaceId, String tabId})? _target;
  bool _sending = false;
  bool _controlling = false;
  ({String sessionId, String workspaceId, String tabId, bool terminate})?
  _armedControl;
  String? _sendFeedback;

  Future<void> _controlAgent(GlobalAgentLiveRow row, bool terminate) async {
    if (_controlling || _sending) return;
    final identity = _identity(row);
    final intent = (
      sessionId: identity.sessionId,
      workspaceId: identity.workspaceId,
      tabId: identity.tabId,
      terminate: terminate,
    );
    if (_armedControl != intent) {
      setState(() {
        _armedControl = intent;
        _sendFeedback = terminate
            ? 'Confirm terminate: this ends the Host PTY and may remove its '
                  'retained Terminal history. Hide card instead to keep working.'
            : 'Confirm interrupt: this sends the Agent adapter interrupt bytes '
                  'without terminating its Session.';
      });
      return;
    }
    final valid = globalAgentTargetIsCurrent(
      workbench: ref.read(workbenchControllerProvider),
      statuses: ref.read(agentStatusControllerProvider),
      sessionId: identity.sessionId,
      workspaceId: identity.workspaceId,
      tabId: identity.tabId,
    );
    if (!valid) {
      setState(() {
        _armedControl = null;
        _sendFeedback =
            'Agent identity changed. Refresh status before controlling.';
      });
      return;
    }
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostVerifiedAgentLifecycleClient) {
      setState(() {
        _armedControl = null;
        _sendFeedback =
            'Host does not support verified Agent lifecycle control.';
      });
      return;
    }
    setState(() {
      _controlling = true;
      _armedControl = null;
      _sendFeedback = 'Requesting verified Agent lifecycle action…';
    });
    try {
      await (host as TerminalHostVerifiedAgentLifecycleClient)
          .controlVerifiedAgent(
            sessionId: identity.sessionId,
            workspaceId: identity.workspaceId,
            tabId: identity.tabId,
            terminate: terminate,
          );
      if (!mounted) return;
      setState(() {
        if (terminate && _target == identity) {
          _target = null;
          _instruction.clear();
        }
        _sendFeedback = terminate
            ? 'Host confirmed Session termination. Refresh the Agent list.'
            : 'Host accepted Agent interrupt. Check its next Hook status.';
      });
    } catch (_) {
      if (!mounted) return;
      // Lost ACK may still mean the request was executed. Do not retry.
      setState(
        () => _sendFeedback = 'Lifecycle result unconfirmed. Inspect Host status before retrying.',
      );
    } finally {
      if (mounted) setState(() => _controlling = false);
    }
  }

  final Set<({String sessionId, String workspaceId, String tabId})>
  _hiddenCards = {};
  bool _showHidden = false;
  final Set<({String sessionId, String workspaceId, String tabId})>
  _expandedTimelines = {};

  /// Keep busy sessions readable; collapse whole status sections explicitly.
  final Set<GlobalAgentDashboardGroup> _collapsedGroups = {};

  // Keep only lightweight keyed builders for visited live cards. Reusing
  // their widget instances skips the whole card subtree for unrelated Hook
  // events. Active history is deliberately not cached: its paginated
  // messages and loading state can change independently of the live row.
  final _cardWidgets =
      <GlobalAgentIdentity, ({_DashboardCardState state, Widget widget})>{};

  static Key _itemKey(Object item) => ValueKey<Object>(
    item is GlobalAgentDashboardSection
        ? item.group
        : (item as GlobalAgentLiveRow).identity,
  );

  Widget _cardWidget(GlobalAgentLiveRow row) {
    final identity = row.identity;
    if (_historicalTarget == identity) {
      _cardWidgets.remove(identity);
      return Builder(builder: (context) => _buildAgentCard(context, row));
    }
    bool armed(bool terminate) =>
        _armedControl ==
        (
          sessionId: identity.sessionId,
          workspaceId: identity.workspaceId,
          tabId: identity.tabId,
          terminate: terminate,
        );
    final state = (
      row: row,
      expanded: _expandedTimelines.contains(identity),
      hidden: _hiddenCards.contains(identity),
      historyBusy: _historicalBusy,
      controlsDisabled: _controlling || _sending,
      armedInterrupt: armed(false),
      armedTerminate: armed(true),
    );
    final previous = _cardWidgets[identity];
    if (previous != null && previous.state == state) return previous.widget;
    final widget = Builder(builder: (context) => _buildAgentCard(context, row));
    _cardWidgets[identity] = (state: state, widget: widget);
    return widget;
  }

  ({String sessionId, String workspaceId, String tabId}) _identity(
    GlobalAgentLiveRow row,
  ) => (
    sessionId: row.tab.terminalSessionId,
    workspaceId: row.status.workspaceId,
    tabId: row.tab.id,
  );

  ({String sessionId, String workspaceId, String tabId})? _historicalTarget;
  final List<({String role, String text, String? timestamp})>
  _historicalMessages = [];
  int? _historicalBefore;
  bool _historicalBusy = false;
  String? _historicalStatus;
  int _historicalRequest = 0;

  void _closeHistoricalMessages() {
    _historicalRequest++;
    _historicalTarget = null;
    _historicalMessages.clear();
    _historicalBefore = null;
    _historicalStatus = null;
    _historicalBusy = false;
  }

  Future<void> _readHistoricalMessages(
    GlobalAgentLiveRow row, {
    bool older = false,
  }) async {
    final identity = _identity(row);
    if (_historicalBusy) return;
    if (!globalAgentTargetIsCurrent(
      workbench: ref.read(workbenchControllerProvider),
      statuses: ref.read(agentStatusControllerProvider),
      sessionId: identity.sessionId,
      workspaceId: identity.workspaceId,
      tabId: identity.tabId,
    )) {
      setState(
        () => _sendFeedback =
            'The original Agent identity changed. Refresh before reading.',
      );
      return;
    }
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostAgentHistoryMessagesClient) {
      setState(
        () => _sendFeedback =
            'This Host version does not support restored conversations.',
      );
      return;
    }
    if (older && (_historicalTarget != identity || _historicalBefore == null)) {
      return;
    }
    setState(() {
      if (!older) {
        _closeHistoricalMessages();
        _historicalTarget = identity;
      }
      _historicalBusy = true;
      _historicalStatus =
          'Reading original native Session messages (read only)…';
    });
    final ticket = ++_historicalRequest;
    try {
      final reply = await (host as TerminalHostAgentHistoryMessagesClient)
          .readRestoredAgentMessages(
            workspaceId: identity.workspaceId,
            tabId: identity.tabId,
            before: older ? _historicalBefore : null,
          );
      if (!mounted ||
          ticket != _historicalRequest ||
          _historicalTarget != identity) {
        return;
      }
      if (reply['readOnly'] != true || reply['source'] != 'nativeProvider') {
        throw const FormatException('History reply failed source validation');
      }
      final raw = reply['messages'];
      if (raw is! List || raw.length > 60) {
        throw const FormatException('History reply too large');
      }
      final received = <({String role, String text, String? timestamp})>[];
      for (final item in raw) {
        if (item is! Map) continue;
        final role = item['role'];
        final text = item['text'];
        if (role is! String ||
            !['user', 'assistant'].contains(role) ||
            text is! String ||
            text.runes.length > 8000) {
          throw const FormatException('Invalid history message');
        }
        received.add((
          role: role,
          text: text,
          timestamp: item['timestamp'] is String
              ? (item['timestamp'] as String).substring(
                  0,
                  (item['timestamp'] as String).length > 64
                      ? 64
                      : (item['timestamp'] as String).length,
                )
              : null,
        ));
      }
      final next = reply['nextBefore'];
      final hasMore = reply['hasMore'] == true;
      if (hasMore && (next is! int || next <= 0)) {
        throw const FormatException('Invalid history cursor');
      }
      setState(() {
        if (older) {
          _historicalMessages.insertAll(0, received);
          // Limit retained plaintext even when browsing very long Sessions.
          if (_historicalMessages.length > 360) {
            _historicalMessages.removeRange(360, _historicalMessages.length);
          }
        } else {
          _historicalMessages
            ..clear()
            ..addAll(received);
        }
        _historicalBefore = hasMore ? next as int : null;
        _historicalStatus = _historicalMessages.isEmpty
            ? 'No native user/assistant messages found in this page.'
            : 'Native conversation history · read-only (Terminal remains live)';
      });
    } catch (_) {
      if (!mounted || ticket != _historicalRequest) return;
      setState(
        () => _historicalStatus = 'History unavailable: this saved Claude/Codex Tab has no unique Host-verified original Session source.',
      );
    } finally {
      if (mounted && ticket == _historicalRequest) {
        setState(() => _historicalBusy = false);
      }
    }
  }

  void _toggleHidden(GlobalAgentLiveRow row) {
    final identity = _identity(row);
    setState(() {
      if (!_hiddenCards.add(identity)) _hiddenCards.remove(identity);
      // Hidden cards cannot silently retain input focus as the target.
      if (_target == identity) {
        _target = null;
        _instruction.clear();
      }
      if (_historicalTarget == identity) _closeHistoricalMessages();
      _armedControl = null;
    });
  }

  @override
  void dispose() {
    _instruction.dispose();
    super.dispose();
  }

  Future<void> _submitToAgent() async {
    final target = _target;
    if (target == null || _sending) return;
    final instruction = _instruction.text;
    final bytes = utf8.encode(instruction).length;
    if (instruction.trim().isEmpty ||
        bytes > 4096 ||
        instruction.contains('\u0000')) {
      setState(() => _sendFeedback = 'Enter a prompt up to 4 KiB.');
      return;
    }
    final stillCurrent = globalAgentTargetIsCurrent(
      workbench: ref.read(workbenchControllerProvider),
      statuses: ref.read(agentStatusControllerProvider),
      sessionId: target.sessionId,
      workspaceId: target.workspaceId,
      tabId: target.tabId,
    );
    if (!stillCurrent) {
      setState(() => _sendFeedback = 'Agent target changed. Select it again.');
      return;
    }
    final host = ref.read(terminalHostClientProvider);
    if (host is! TerminalHostVerifiedAgentInputClient) {
      setState(
        () => _sendFeedback =
            'Current Host does not support guarded Agent input.',
      );
      return;
    }
    setState(() {
      _sending = true;
      _sendFeedback = 'Sending to the verified Host Session…';
    });
    try {
      await (host as TerminalHostVerifiedAgentInputClient)
          .submitVerifiedAgentPrompt(
            sessionId: target.sessionId,
            workspaceId: target.workspaceId,
            tabId: target.tabId,
            prompt: instruction,
          );
      if (!mounted) return;
      if (_target == target && _instruction.text == instruction) {
        _instruction.clear();
      }
      setState(
        () => _sendFeedback = _target == target
            ? 'Host acknowledged Agent input.'
            : 'Host acknowledged input to the previously selected Agent.',
      );
    } catch (_) {
      if (!mounted) return;
      // Do not retry automatically: a disconnect after enqueue could have
      // delivered the prompt despite a missing acknowledgement.
      setState(
        () => _sendFeedback =
            'Could not confirm delivery. Refresh status before retrying.',
      );
    } finally {
      if (mounted) setState(() => _sending = false);
    }
  }

  Future<void> _launch() async {
    final created = await showDialog<String>(
      context: context,
      builder: (_) => const GlobalAgentLaunchDialog(),
    );
    if (!mounted || created == null) return;
    setState(
      () => _sendFeedback = 'Agent tab created. New sessions appear after Host/Hook confirmation.',
    );
  }

  Future<void> _focus(GlobalAgentLiveRow row) async {
    final state = ref.read(workbenchControllerProvider);
    final status = ref.read(
      agentStatusControllerProvider,
    )[row.tab.terminalSessionId];
    final current = state.tabsFor(row.status.workspaceId);
    if (status == null ||
        status.workspaceId != row.status.workspaceId ||
        status.tabId != row.tab.id ||
        status.terminalSessionId != row.tab.terminalSessionId ||
        !current.any(
          (tab) =>
              tab.id == row.tab.id &&
              tab.terminalSessionId == row.tab.terminalSessionId,
        )) {
      return;
    }
    await ref
        .read(workbenchControllerProvider.notifier)
        .selectWorkspaceTab(
          workspaceId: status.workspaceId,
          tabId: status.tabId,
        );
    if (mounted) {
      ref.read(globalAgentDashboardNavigationProvider.notifier).close();
    }
  }

  @override
  Widget build(BuildContext context) {
    // History browsing does not subscribe to the high-frequency live map.
    final all = _historyMode
        ? const <GlobalAgentLiveRow>[]
        : ref.watch(globalAgentLiveRowsProvider);
    if (_historyMode) {
      _cardWidgets.clear();
    } else {
      final liveIdentities = all.map(_identity).toSet();
      _hiddenCards.removeWhere(
        (identity) => !liveIdentities.contains(identity),
      );
      _expandedTimelines.removeWhere(
        (identity) => !liveIdentities.contains(identity),
      );
    }
    final searchTerms = globalAgentSearchTerms(_query);
    final needsReviewCount = all.where((row) => row.needsReview).length;
    // A hidden card cannot hide a waiting/blocked Agent from the review queue.
    final rows = all
        .where(
          (row) =>
              _needsReviewOnly ||
              _showHidden ||
              !_hiddenCards.contains(_identity(row)),
        )
        .where((row) => !_needsReviewOnly || row.needsReview)
        .where((row) => row.matchesTerms(searchTerms))
        .toList();
    final items = globalAgentDashboardVisibleItems(
      rows,
      _collapsedGroups,
      alreadySorted: true,
    );
    final indexByKey = <Key, int>{
      for (var i = 0; i < items.length; i++) _itemKey(items[i]): i,
    };
    // Filtered/collapsed cards can release their render snapshots; the
    // projection keeps only their current data while they are live.
    _cardWidgets.removeWhere(
      (identity, _) => !indexByKey.containsKey(ValueKey<Object>(identity)),
    );
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    return Material(
      color: AleraTokens.bg,
      child: SizedBox.expand(
        child: Padding(
          padding: const EdgeInsets.all(AleraTokens.space24),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      'Global Agent Dashboard',
                      style: Theme.of(context).textTheme.titleLarge,
                    ),
                  ),
                  TextButton.icon(
                    onPressed: _launch,
                    icon: const Icon(Icons.add),
                    label: const Text('New Agent'),
                  ),
                  IconButton(
                    tooltip: 'Close',
                    onPressed: () => ref
                        .read(globalAgentDashboardNavigationProvider.notifier)
                        .close(),
                    icon: const Icon(Icons.close),
                  ),
                ],
              ),
              const SizedBox(height: AleraTokens.space8),
              Text(
                'All registered projects and worktrees. Live Host Agents or '
                'opt-in local history previews; semantic search is not yet available.',
                style: muted,
              ),
              const SizedBox(height: 8),
              Row(
                children: [
                  TextButton.icon(
                    onPressed: () => setState(() => _historyMode = false),
                    icon: const Icon(Icons.smart_toy_outlined),
                    label: Text(_historyMode ? 'Live Agents' : '● Live Agents'),
                  ),
                  const SizedBox(width: 8),
                  TextButton.icon(
                    onPressed: () => setState(() => _historyMode = true),
                    icon: const Icon(Icons.history),
                    label: Text(
                      _historyMode ? '● History Search' : 'History Search',
                    ),
                  ),
                ],
              ),
              if (_historyMode)
                const Expanded(child: GlobalAgentHistoryPanel())
              else ...[
                const SizedBox(height: AleraTokens.space16),
                TextField(
                  autofocus: true,
                  decoration: const InputDecoration(
                    prefixIcon: Icon(Icons.search),
                    labelText: 'Search Live Sessions',
                    hintText: 'Project, Agent, overall objective or progress',
                  ),
                  onChanged: (value) => setState(() => _query = value),
                ),
                const SizedBox(height: AleraTokens.space12),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        '${rows.length} of ${all.length} Agent sessions',
                        style: muted,
                      ),
                    ),
                    TextButton(
                      onPressed: () =>
                          setState(() => _showHidden = !_showHidden),
                      child: Text(
                        _showHidden
                            ? 'Show visible only'
                            : 'Show hidden cards (${_hiddenCards.length})',
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: 4),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        'Needs You is a review queue based on last verified Waiting/Blocked Hook state; approval is not inferred.',
                        style: muted,
                      ),
                    ),
                    TextButton.icon(
                      onPressed: () => setState(() {
                        _needsReviewOnly = !_needsReviewOnly;
                        _closeHistoricalMessages();
                        _collapsedGroups.clear();
                        _target = null;
                        _armedControl = null;
                        _sendFeedback =
                            'Filter changed; select an Agent before sending.';
                      }),
                      icon: Icon(
                        _needsReviewOnly
                            ? Icons.filter_alt_off_outlined
                            : Icons.notification_important_outlined,
                      ),
                      label: Text(
                        _needsReviewOnly
                            ? 'Show all Agents'
                            : 'Needs You ($needsReviewCount)',
                      ),
                    ),
                  ],
                ),
                const SizedBox(height: AleraTokens.space8),
                if (_target != null) ...[
                  Text(
                    'Target: ${_target!.workspaceId} / ${_target!.tabId}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: muted,
                  ),
                  const SizedBox(height: 6),
                  Row(
                    children: [
                      Expanded(
                        child: TextField(
                          controller: _instruction,
                          enabled: !_sending,
                          maxLines: 2,
                          minLines: 1,
                          maxLength: 4000,
                          decoration: const InputDecoration(
                            hintText: 'Send an instruction to this Agent without opening its Tab',
                            isDense: true,
                          ),
                        ),
                      ),
                      const SizedBox(width: 8),
                      FilledButton(
                        onPressed: _sending ? null : _submitToAgent,
                        child: const Text('Send'),
                      ),
                    ],
                  ),
                  const SizedBox(height: 8),
                ],
                if (_sendFeedback != null)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 8),
                    child: Text(_sendFeedback!, style: muted),
                  ),
                Expanded(
                  child: rows.isEmpty
                      ? Center(
                          child: Text(
                            all.isEmpty
                                ? 'No verified live Agent sessions in registered workspaces.'
                                : 'No matching live Agent sessions.',
                            style: muted,
                          ),
                        )
                      : ListView.builder(
                          itemCount: items.length,
                          findChildIndexCallback: (key) => indexByKey[key],
                          itemBuilder: (context, index) {
                            final item = items[index];
                            if (item is GlobalAgentDashboardSection) {
                              final collapsed = _collapsedGroups.contains(
                                item.group,
                              );
                              return Padding(
                                key: _itemKey(item),
                                padding: EdgeInsets.only(
                                  bottom: index == items.length - 1
                                      ? 0
                                      : AleraTokens.space8,
                                ),
                                child: InkWell(
                                  onTap: () => setState(() {
                                    if (!_collapsedGroups.add(item.group)) {
                                      _collapsedGroups.remove(item.group);
                                    }
                                    // Collapsing a group must not leave an
                                    // invisible target or armed destructive action.
                                    _target = null;
                                    _instruction.clear();
                                    _closeHistoricalMessages();
                                    _armedControl = null;
                                  }),
                                  child: Padding(
                                    padding: const EdgeInsets.fromLTRB(
                                      4,
                                      12,
                                      4,
                                      4,
                                    ),
                                    child: Row(
                                      children: [
                                        Icon(
                                          collapsed
                                              ? Icons.chevron_right
                                              : Icons.expand_more,
                                        ),
                                        const SizedBox(width: 4),
                                        Text(
                                          item.group.label,
                                          style: Theme.of(context)
                                              .textTheme
                                              .titleMedium,
                                        ),
                                        const SizedBox(width: 8),
                                        Text(
                                          '${item.rows.length} Agents',
                                          style: muted,
                                        ),
                                        const Spacer(),
                                        Text(
                                          collapsed ? 'Expand' : 'Collapse',
                                          style: muted,
                                        ),
                                      ],
                                    ),
                                  ),
                                ),
                              );
                            }
                            return Padding(
                              key: _itemKey(item),
                              padding: EdgeInsets.only(
                                bottom: index == items.length - 1
                                    ? 0
                                    : AleraTokens.space8,
                              ),
                              child: _cardWidget(item as GlobalAgentLiveRow),
                            );
                          },
                        ),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }

  Widget _buildAgentCard(BuildContext context, GlobalAgentLiveRow row) {
    final identity = _identity(row);
    final expanded = _expandedTimelines.contains(identity);
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    return DecoratedBox(
      decoration: BoxDecoration(
        color: AleraTokens.surface,
        border: Border.all(color: AleraTokens.borderSubtle),
        borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
      ),
      child: Padding(
        padding: const EdgeInsets.all(AleraTokens.space16),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Expanded(
                  child: Text(
                    '${row.project} / ${row.workspace}',
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                ),
                Text(
                  '${row.status.agentType.key} - '
                  '${row.status.state.key}',
                  style: muted,
                ),
              ],
            ),
            if (row.needsReview) ...[
              const SizedBox(height: AleraTokens.space4),
              Text(
                'Needs review · ${row.status.state.key} · last reported ${row.status.updatedAt.toLocal()}',
                style: muted,
              ),
            ],
            const SizedBox(height: AleraTokens.space8),
            Text(
              row.goal.isEmpty ? row.tab.title : row.goal,
              maxLines: 3,
              overflow: TextOverflow.ellipsis,
            ),
            if (row.progress.isNotEmpty) ...[
              const SizedBox(height: AleraTokens.space4),
              Text(
                row.progress,
                maxLines: 2,
                overflow: TextOverflow.ellipsis,
                style: muted,
              ),
            ],
            if (row.timeline.isNotEmpty) ...[
              TextButton.icon(
                onPressed: () => setState(() {
                  if (!_expandedTimelines.add(identity)) {
                    _expandedTimelines.remove(identity);
                  }
                }),
                icon: Icon(expanded ? Icons.expand_less : Icons.expand_more),
                label: Text('Timeline (${row.timeline.length})'),
              ),
              if (expanded)
                for (final event in row.timeline)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 4),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          '${event.kind} · ${event.at.toLocal()}',
                          style: muted,
                        ),
                        Text(event.text),
                      ],
                    ),
                  ),
            ],
            Wrap(
              alignment: WrapAlignment.end,
              spacing: 4,
              runSpacing: 0,
              children: [
                TextButton.icon(
                  onPressed: () => setState(() {
                    _target = (
                      sessionId: row.tab.terminalSessionId,
                      workspaceId: row.status.workspaceId,
                      tabId: row.tab.id,
                    );
                    _sendFeedback = null;
                  }),
                  icon: const Icon(Icons.edit_note),
                  label: const Text('Steer'),
                ),
                if (row.status.agentType.key == 'claude' ||
                    row.status.agentType.key == 'codex')
                  TextButton.icon(
                    onPressed: _historicalBusy
                        ? null
                        : () => _readHistoricalMessages(row),
                    icon: const Icon(Icons.menu_book_outlined),
                    label: const Text('Original conversation'),
                  ),
                TextButton.icon(
                  onPressed: () => _focus(row),
                  icon: const Icon(Icons.open_in_new),
                  label: const Text('Open Agent'),
                ),
                TextButton.icon(
                  onPressed: () => _toggleHidden(row),
                  icon: Icon(
                    _hiddenCards.contains(identity)
                        ? Icons.visibility
                        : Icons.visibility_off,
                  ),
                  label: Text(
                    _hiddenCards.contains(identity)
                        ? 'Show card'
                        : 'Hide (keeps running)',
                  ),
                ),
                TextButton.icon(
                  onPressed: _controlling || _sending
                      ? null
                      : () => _controlAgent(row, false),
                  icon: const Icon(Icons.pause_circle_outline),
                  label: Text(
                    _armedControl ==
                            (
                              sessionId: row.tab.terminalSessionId,
                              workspaceId: row.status.workspaceId,
                              tabId: row.tab.id,
                              terminate: false,
                            )
                        ? 'Confirm interrupt'
                        : 'Interrupt',
                  ),
                ),
                TextButton.icon(
                  onPressed: _controlling || _sending
                      ? null
                      : () => _controlAgent(row, true),
                  icon: const Icon(Icons.stop_circle_outlined),
                  label: Text(
                    _armedControl ==
                            (
                              sessionId: row.tab.terminalSessionId,
                              workspaceId: row.status.workspaceId,
                              tabId: row.tab.id,
                              terminate: true,
                            )
                        ? 'Confirm terminate'
                        : 'Terminate',
                  ),
                ),
              ],
            ),
            if (_historicalTarget == identity) ...[
              const Divider(height: 16),
              Row(
                children: [
                  const Icon(Icons.history, size: 16),
                  const SizedBox(width: 8),
                  const Expanded(
                    child: Text(
                      'Original Claude/Codex conversation · read only',
                    ),
                  ),
                  IconButton(
                    tooltip: 'Close original conversation',
                    onPressed: () => setState(_closeHistoricalMessages),
                    icon: const Icon(Icons.close, size: 18),
                  ),
                ],
              ),
              if (_historicalStatus != null)
                Text(_historicalStatus!, style: muted),
              if (_historicalBusy) const LinearProgressIndicator(minHeight: 2),
              if (_historicalMessages.isNotEmpty)
                SizedBox(
                  height: 340,
                  child: ListView.builder(
                    itemCount: _historicalMessages.length,
                    itemBuilder: (context, itemIndex) {
                      final message = _historicalMessages[itemIndex];
                      return Padding(
                        padding: const EdgeInsets.symmetric(vertical: 5),
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            Text(
                              message.role == 'user' ? 'You' : 'Agent',
                              style: Theme.of(context).textTheme.labelMedium,
                            ),
                            if (message.timestamp != null)
                              Text(message.timestamp!, style: muted),
                            SelectableText(message.text),
                          ],
                        ),
                      );
                    },
                  ),
                ),
              if (_historicalBefore != null)
                TextButton.icon(
                  onPressed: _historicalBusy
                      ? null
                      : () => _readHistoricalMessages(row, older: true),
                  icon: const Icon(Icons.expand_less),
                  label: const Text('Load older conversation messages'),
                ),
            ],
          ],
        ),
      ),
    );
  }
}
