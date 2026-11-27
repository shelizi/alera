import 'dart:convert';
import 'dart:math' as math;

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/global_agent_dashboard_navigation.dart';
import 'package:alera/src/features/workbench/application/global_agent_live_projection.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_launch_dialog.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_history_panel.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_inspector_dialog.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_acknowledgements.dart';
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
  bool pinning,
  bool openingInspector,
  bool doneUnread,
});

/// Open the live inspector as a true modal: only tapping its outside barrier
/// or the explicit Close button dismisses it. The inspector itself routes
/// Escape to the existing Terminal session rather than popping this route.
@visibleForTesting
Future<void> showAgentInspectorModal(BuildContext context, Widget inspector) =>
    showGeneralDialog<void>(
      context: context,
      barrierDismissible: true,
      barrierLabel: 'Close Agent inspector',
      barrierColor: Colors.black54,
      transitionDuration: Duration.zero,
      pageBuilder: (_, _, _) => inspector,
    );

/// Global scope is deliberate: this watches the full Workbench inventory and
/// current Agent map, not the selected project's subset. This page lives in
/// the center workbench; the sidebar and existing Tab state remain mounted.
class GlobalAgentDashboardPage extends ConsumerStatefulWidget {
  const GlobalAgentDashboardPage({super.key});

  @override
  ConsumerState<GlobalAgentDashboardPage> createState() =>
      _GlobalAgentDashboardPageState();
}

/// Always size against every status lane, independent of card counts and
/// filtering. Narrow windows scroll horizontally instead of crushing cards.
@visibleForTesting
double globalAgentDashboardColumnWidth(double viewportWidth) {
  final columns = GlobalAgentDashboardGroup.values.length;
  const gap = 12.0;
  return math.max(
    280.0,
    math.min(385.0, (viewportWidth - gap * (columns - 1)) / columns),
  );
}

class _GlobalAgentDashboardPageState
    extends ConsumerState<GlobalAgentDashboardPage> {
  final ScrollController _horizontal = ScrollController();
  final Set<String> _pinningTabIds = <String>{};
  bool _openingInspector = false;
  String _query = '';
  final TextEditingController _searchController = TextEditingController();

  bool _isDoneUnread(GlobalAgentLiveRow row) =>
      GlobalAgentDashboardGroup.forStatus(row.status) ==
          GlobalAgentDashboardGroup.done &&
      !isCompletionAcknowledged(
        ref.read(workbenchTabCompletionAcknowledgementsControllerProvider),
        row.status,
      );

  void _setDoneRead(GlobalAgentLiveRow row, bool read) {
    final current = ref.read(
      agentStatusControllerProvider,
    )[row.identity.sessionId];
    if (current == null ||
        current.workspaceId != row.identity.workspaceId ||
        current.tabId != row.identity.tabId ||
        GlobalAgentDashboardGroup.forStatus(current) !=
            GlobalAgentDashboardGroup.done ||
        current.stateStartedAt != row.status.stateStartedAt)
      return;
    final acknowledgements = ref.read(
      workbenchTabCompletionAcknowledgementsControllerProvider.notifier,
    );
    if (read) {
      acknowledgements.acknowledge(current);
    } else {
      acknowledgements.markUnread(current);
    }
  }

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
      pinning: _pinningTabIds.contains(row.tab.id),
      openingInspector: _openingInspector,
      doneUnread: _isDoneUnread(row),
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
    _horizontal.dispose();
    _searchController.dispose();
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
      if (_isDoneUnread(row)) _setDoneRead(row, true);
      ref.read(globalAgentDashboardNavigationProvider.notifier).close();
    }
  }

  Future<void> _togglePinned(GlobalAgentLiveRow row) async {
    final identity = _identity(row);
    if (_pinningTabIds.contains(identity.tabId)) return;
    if (!globalAgentTargetIsCurrent(
      workbench: ref.read(workbenchControllerProvider),
      statuses: ref.read(agentStatusControllerProvider),
      sessionId: identity.sessionId,
      workspaceId: identity.workspaceId,
      tabId: identity.tabId,
    )) {
      setState(
        () => _sendFeedback = 'Agent Session changed. Pin was not saved.',
      );
      return;
    }
    setState(() => _pinningTabIds.add(identity.tabId));
    try {
      // Reuse existing persisted Tab pin state so pins survive app restarts,
      // stay in sync with the Tab strip and never mutate/stop the Agent.
      await ref
          .read(workbenchControllerProvider.notifier)
          .setWorkspaceTabPinned(
            tabId: identity.tabId,
            pinned: !row.tab.isPinned,
          );
    } on Object {
      if (mounted)
        setState(() => _sendFeedback = 'Could not save this Agent pin.');
    } finally {
      if (mounted) setState(() => _pinningTabIds.remove(identity.tabId));
    }
  }

  Future<void> _inspect(GlobalAgentLiveRow row) async {
    if (_openingInspector) return;
    final workbench = ref.read(workbenchControllerProvider);
    final statuses = ref.read(agentStatusControllerProvider);
    final identity = _identity(row);
    if (!globalAgentTargetIsCurrent(
      workbench: workbench,
      statuses: statuses,
      sessionId: identity.sessionId,
      workspaceId: identity.workspaceId,
      tabId: identity.tabId,
    )) {
      setState(
        () => _sendFeedback = 'Agent Session changed. Refresh the board.',
      );
      return;
    }
    final workspace = workbench.workspacesByProject.values
        .expand((entries) => entries)
        .where((entry) => entry.id == identity.workspaceId)
        .firstOrNull;
    if (workspace == null) {
      setState(() => _sendFeedback = 'The original Worktree is not available.');
      return;
    }
    // Reuse the same session identity and rendering handle. No new Agent tab
    // is created, and the Dashboard stays in place behind the inspector.
    final runtime = ref.read(terminalRuntimeProvider);
    final session = runtime.sessionFor(workspace: workspace, tab: row.tab);
    if (session.terminalSessionId != identity.sessionId) {
      setState(() => _sendFeedback = 'Terminal Session identity changed.');
      return;
    }
    final overlay = ref.read(globalAgentInspectionOpenProvider.notifier);
    setState(() {
      _openingInspector = true;
      _sendFeedback = null;
    });
    try {
      // Release only the target Terminal renderer before mounting its modal.
      overlay.open(identity.sessionId);
      await WidgetsBinding.instance.endOfFrame;
      if (!mounted) return;
      if (_isDoneUnread(row)) _setDoneRead(row, true);
      await showAgentInspectorModal(
        context,
        GlobalAgentInspectorDialog(row: row, session: session),
      );
    } finally {
      // A zero-duration route and one final frame allow its TerminalView to
      // unmount before the original workbench's renderer is restored.
      await WidgetsBinding.instance.endOfFrame;
      overlay.close();
      if (mounted) setState(() => _openingInspector = false);
    }
  }

  static Color _groupColor(GlobalAgentDashboardGroup group) => switch (group) {
    GlobalAgentDashboardGroup.blocked => const Color(0xFFE57575),
    GlobalAgentDashboardGroup.waiting => const Color(0xFFE7B35C),
    GlobalAgentDashboardGroup.working => const Color(0xFF63C7A4),
    GlobalAgentDashboardGroup.interrupted => const Color(0xFFA99BE5),
    GlobalAgentDashboardGroup.done => const Color(0xFF89A2B7),
  };

  static IconData _groupIcon(GlobalAgentDashboardGroup group) =>
      switch (group) {
        GlobalAgentDashboardGroup.blocked => Icons.error_outline,
        GlobalAgentDashboardGroup.waiting => Icons.hourglass_top,
        GlobalAgentDashboardGroup.working => Icons.bolt_outlined,
        GlobalAgentDashboardGroup.interrupted => Icons.pause_circle_outline,
        GlobalAgentDashboardGroup.done => Icons.check_circle_outline,
      };

  Widget _statusColumn(GlobalAgentDashboardSection section, double width) {
    final color = _groupColor(section.group);
    final collapsed = _collapsedGroups.contains(section.group);
    final indexByKey = <Key, int>{
      for (var i = 0; i < section.rows.length; i++)
        ValueKey<Object>(section.rows[i].identity): i,
    };
    return SizedBox(
      key: ValueKey(section.group),
      width: width,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          InkWell(
            onTap: () => setState(() {
              if (!_collapsedGroups.add(section.group)) {
                _collapsedGroups.remove(section.group);
              }
              _target = null;
              _instruction.clear();
              _closeHistoricalMessages();
              _armedControl = null;
            }),
            child: Padding(
              padding: const EdgeInsets.fromLTRB(4, 4, 4, 12),
              child: Row(
                children: [
                  Icon(_groupIcon(section.group), color: color, size: 20),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Text(
                      section.group.label,
                      style: Theme.of(context).textTheme.titleMedium,
                    ),
                  ),
                  Text(
                    '${section.rows.length}',
                    style: TextStyle(color: color, fontWeight: FontWeight.w700),
                  ),
                  Icon(collapsed ? Icons.chevron_right : Icons.expand_more),
                ],
              ),
            ),
          ),
          if (!collapsed)
            Expanded(
              child: ListView.builder(
                key: PageStorageKey<String>(
                  'agent-column-${section.group.name}',
                ),
                primary: false,
                itemCount: section.rows.length,
                findChildIndexCallback: (key) => indexByKey[key],
                itemBuilder: (_, index) {
                  final row = section.rows[index];
                  return Padding(
                    key: ValueKey<Object>(row.identity),
                    padding: const EdgeInsets.only(bottom: AleraTokens.space8),
                    child: _cardWidget(row),
                  );
                },
              ),
            ),
        ],
      ),
    );
  }

  Widget _board(List<GlobalAgentDashboardSection> columns) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final columnWidth = globalAgentDashboardColumnWidth(
          constraints.maxWidth,
        );
        return Scrollbar(
          controller: _horizontal,
          child: SingleChildScrollView(
            controller: _horizontal,
            scrollDirection: Axis.horizontal,
            child: SizedBox(
              height: constraints.maxHeight,
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: <Widget>[
                  for (var i = 0; i < columns.length; i++) ...<Widget>[
                    if (i > 0) const SizedBox(width: 12),
                    _statusColumn(columns[i], columnWidth),
                  ],
                ],
              ),
            ),
          ),
        );
      },
    );
  }

  @override
  Widget build(BuildContext context) {
    // The sidebar and tab strip use this same completion receipt provider.
    // Rebuild and invalidate only changed cards when either surface reads a run.
    if (!_historyMode) {
      ref.watch(workbenchTabCompletionAcknowledgementsControllerProvider);
    }
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
    final columns = groupGlobalAgentLiveRows(
      rows,
      alreadySorted: true,
      pinnedFirst: true,
      includeEmpty: true,
    );
    for (final section in columns) {
      if (section.group == GlobalAgentDashboardGroup.done) {
        section.rows.sort((a, b) {
          final unread = (_isDoneUnread(b) ? 1 : 0).compareTo(
            _isDoneUnread(a) ? 1 : 0,
          );
          if (unread != 0) return unread;
          final completed = b.status.stateStartedAt.compareTo(
            a.status.stateStartedAt,
          );
          return completed != 0 ? completed : a.tab.id.compareTo(b.tab.id);
        });
      }
    }
    final visible = {
      for (final section in columns)
        if (!_collapsedGroups.contains(section.group))
          for (final row in section.rows) row.identity,
    };
    _cardWidgets.removeWhere((identity, _) => !visible.contains(identity));
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    return Material(
      color: AleraTokens.bg,
      child: SizedBox.expand(
        child: Padding(
          padding: const EdgeInsets.all(AleraTokens.space8),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              Row(
                children: [
                  Expanded(
                    child: LayoutBuilder(
                      builder: (context, constraints) {
                        final searchWidth = (constraints.maxWidth * .30)
                            .clamp(120.0, 280.0)
                            .toDouble();
                        return SingleChildScrollView(
                          scrollDirection: Axis.horizontal,
                          child: Row(
                            mainAxisSize: MainAxisSize.min,
                            children: [
                              Tooltip(
                                message:
                                    '${rows.length} of ${all.length} Agent sessions',
                                child: Text(
                                  'Agents',
                                  style: Theme.of(context).textTheme.titleSmall,
                                ),
                              ),
                              const SizedBox(width: 8),
                              if (!_historyMode)
                                SizedBox(
                                  width: searchWidth,
                                  height: 34,
                                  child: TextField(
                                    controller: _searchController,
                                    decoration: InputDecoration(
                                      prefixIcon: const Icon(
                                        Icons.search,
                                        size: 16,
                                      ),
                                      hintText: 'Search Agents',
                                      isDense: true,
                                      contentPadding:
                                          const EdgeInsets.symmetric(
                                            horizontal: 6,
                                          ),
                                      suffixIcon: _query.isEmpty
                                          ? null
                                          : IconButton(
                                              tooltip: 'Clear search',
                                              icon: const Icon(
                                                Icons.close,
                                                size: 14,
                                              ),
                                              onPressed: () => setState(() {
                                                _searchController.clear();
                                                _query = '';
                                              }),
                                            ),
                                    ),
                                    onChanged: (value) =>
                                        setState(() => _query = value),
                                  ),
                                ),
                              const SizedBox(width: 4),
                              if (!_historyMode) ...[
                                Tooltip(
                                  message: 'Waiting / blocked Agents requiring review (not inferred approval)',
                                  child: FilterChip(
                                    visualDensity: VisualDensity.compact,
                                    showCheckmark: false,
                                    selected: _needsReviewOnly,
                                    label: Text(
                                      'Needs You ($needsReviewCount)',
                                    ),
                                    onSelected: (selected) => setState(() {
                                      _needsReviewOnly = selected;
                                      _closeHistoricalMessages();
                                      _collapsedGroups.clear();
                                      _target = null;
                                      _armedControl = null;
                                      _sendFeedback = null;
                                    }),
                                  ),
                                ),
                                if (_hiddenCards.isNotEmpty || _showHidden) ...[
                                  const SizedBox(width: 4),
                                  Tooltip(
                                    message: _showHidden
                                        ? 'Show visible only'
                                        : 'Show hidden cards',
                                    child: FilterChip(
                                      visualDensity: VisualDensity.compact,
                                      showCheckmark: false,
                                      selected: _showHidden,
                                      avatar: Icon(
                                        _showHidden
                                            ? Icons.visibility
                                            : Icons.visibility_off,
                                        size: 15,
                                      ),
                                      label: Text('${_hiddenCards.length}'),
                                      onSelected: (selected) => setState(
                                        () => _showHidden = selected,
                                      ),
                                    ),
                                  ),
                                ],
                              ],
                              IconButton(
                                visualDensity: VisualDensity.compact,
                                tooltip: _historyMode
                                    ? 'Live Agents'
                                    : 'History Search',
                                icon: Icon(
                                  _historyMode
                                      ? Icons.view_kanban_outlined
                                      : Icons.history,
                                  size: 19,
                                ),
                                onPressed: () => setState(
                                  () => _historyMode = !_historyMode,
                                ),
                              ),
                            ],
                          ),
                        );
                      },
                    ),
                  ),
                  TextButton.icon(
                    style: TextButton.styleFrom(
                      visualDensity: VisualDensity.compact,
                    ),
                    onPressed: _launch,
                    icon: const Icon(Icons.add, size: 18),
                    label: const Text('Agent'),
                  ),
                  IconButton(
                    tooltip: 'Close',
                    visualDensity: VisualDensity.compact,
                    onPressed: () => ref
                        .read(globalAgentDashboardNavigationProvider.notifier)
                        .close(),
                    icon: const Icon(Icons.close, size: 18),
                  ),
                ],
              ),
              const SizedBox(height: 4),
              if (_historyMode)
                const Expanded(child: GlobalAgentHistoryPanel())
              else ...[
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
                      : _board(columns),
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
    final done =
        GlobalAgentDashboardGroup.forStatus(row.status) ==
        GlobalAgentDashboardGroup.done;
    final doneUnread = _isDoneUnread(row);
    bool armed(bool terminate) =>
        _armedControl ==
        (
          sessionId: identity.sessionId,
          workspaceId: identity.workspaceId,
          tabId: identity.tabId,
          terminate: terminate,
        );
    final armedInterrupt = armed(false);
    final armedTerminate = armed(true);
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    return Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: _openingInspector ? null : () => _inspect(row),
        borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
        child: DecoratedBox(
          decoration: BoxDecoration(
            color: doneUnread
                ? Color.alphaBlend(
                    AleraTokens.success.withValues(alpha: .15),
                    AleraTokens.surface,
                  )
                : AleraTokens.surface,
            border: Border.all(
              color: doneUnread
                  ? AleraTokens.success.withValues(alpha: .6)
                  : AleraTokens.borderSubtle,
            ),
            borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
          ),
          child: Padding(
            padding: const EdgeInsets.all(AleraTokens.space12),
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
                    IconButton(
                      tooltip: row.tab.isPinned
                          ? 'Unpin Agent'
                          : 'Pin Agent to top',
                      icon: Icon(
                        row.tab.isPinned
                            ? Icons.push_pin
                            : Icons.push_pin_outlined,
                      ),
                      iconSize: 17,
                      visualDensity: VisualDensity.compact,
                      onPressed: _pinningTabIds.contains(row.tab.id)
                          ? null
                          : () => _togglePinned(row),
                    ),
                    IconButton(
                      tooltip: 'Inspect live Agent',
                      icon: const Icon(Icons.open_in_full, size: 17),
                      visualDensity: VisualDensity.compact,
                      onPressed: _openingInspector ? null : () => _inspect(row),
                    ),
                  ],
                ),
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        '${row.status.agentType.key} · ${row.status.state.key}',
                        style: muted,
                      ),
                    ),
                    if (done)
                      Text(
                        doneUnread ? '● Unread' : 'Read',
                        style: muted?.copyWith(
                          color: doneUnread
                              ? AleraTokens.success
                              : AleraTokens.foregroundMuted,
                        ),
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
                  maxLines: 6,
                  overflow: TextOverflow.ellipsis,
                ),
                if (row.progress.isNotEmpty) ...[
                  const SizedBox(height: AleraTokens.space4),
                  Text(
                    row.progress,
                    maxLines: 8,
                    overflow: TextOverflow.ellipsis,
                    style: Theme.of(context).textTheme.bodyMedium
                        ?.copyWith(color: AleraTokens.foregroundMuted),
                  ),
                ],
                if (row.timeline.isNotEmpty) ...[
                  TextButton.icon(
                    onPressed: () => setState(() {
                      if (!_expandedTimelines.add(identity)) {
                        _expandedTimelines.remove(identity);
                      }
                    }),
                    icon: Icon(
                      expanded ? Icons.expand_less : Icons.expand_more,
                    ),
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
                            Text(
                              event.kind == 'report'
                                  ? (event.summary.isNotEmpty
                                        ? event.summary
                                        : event.text)
                                  : event.text,
                            ),
                          ],
                        ),
                      ),
                ],
                Row(
                  mainAxisAlignment: MainAxisAlignment.end,
                  children: [
                    IconButton(
                      tooltip: 'Steer',
                      visualDensity: VisualDensity.compact,
                      icon: const Icon(Icons.edit_note, size: 19),
                      onPressed: () => setState(() {
                        _target = identity;
                        _sendFeedback = null;
                      }),
                    ),
                    IconButton(
                      tooltip: 'Open Agent',
                      visualDensity: VisualDensity.compact,
                      icon: const Icon(Icons.open_in_new, size: 18),
                      onPressed: () => _focus(row),
                    ),
                    PopupMenuButton<String>(
                      tooltip: 'More actions',
                      icon: const Icon(Icons.more_horiz, size: 20),
                      onSelected: (action) {
                        switch (action) {
                          case 'read':
                            _setDoneRead(row, doneUnread);
                            break;
                          case 'history':
                            _readHistoricalMessages(row);
                            break;
                          case 'hide':
                            _toggleHidden(row);
                            break;
                          case 'interrupt':
                            if (!armedInterrupt) _controlAgent(row, false);
                            break;
                          case 'terminate':
                            if (!armedTerminate) _controlAgent(row, true);
                            break;
                        }
                      },
                      itemBuilder: (_) => [
                        if (done)
                          PopupMenuItem(
                            value: 'read',
                            child: Text(
                              doneUnread ? 'Mark as read' : 'Mark as unread',
                            ),
                          ),
                        if (row.status.agentType.key == 'claude' ||
                            row.status.agentType.key == 'codex')
                          PopupMenuItem(
                            value: 'history',
                            enabled: !_historicalBusy,
                            child: const Text('Original conversation'),
                          ),
                        PopupMenuItem(
                          value: 'hide',
                          child: Text(
                            _hiddenCards.contains(identity)
                                ? 'Show card'
                                : 'Hide (keeps running)',
                          ),
                        ),
                        const PopupMenuDivider(),
                        PopupMenuItem(
                          value: 'interrupt',
                          enabled: !_controlling && !_sending,
                          child: const Text('Interrupt'),
                        ),
                        PopupMenuItem(
                          value: 'terminate',
                          enabled: !_controlling && !_sending,
                          child: const Text('Terminate'),
                        ),
                      ],
                    ),
                  ],
                ),
                if (armedInterrupt || armedTerminate)
                  Row(
                    children: [
                      Expanded(
                        child: TextButton(
                          onPressed: _controlling || _sending
                              ? null
                              : () => _controlAgent(row, armedTerminate),
                          child: Text(
                            armedTerminate
                                ? 'Confirm terminate'
                                : 'Confirm interrupt',
                          ),
                        ),
                      ),
                      IconButton(
                        tooltip: 'Cancel confirmation',
                        icon: const Icon(Icons.close, size: 18),
                        onPressed: () => setState(() {
                          _armedControl = null;
                          _sendFeedback = null;
                        }),
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
                  if (_historicalBusy)
                    const LinearProgressIndicator(minHeight: 2),
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
                                  style: Theme.of(context)
                                      .textTheme
                                      .labelMedium,
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
        ),
      ),
    );
  }
}
