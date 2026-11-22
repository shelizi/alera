import 'dart:math' as math;

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/workbench/application/global_agent_dashboard_navigation.dart';
import 'package:alera/src/features/workbench/application/global_agent_live_projection.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_history_panel.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_inspector_dialog.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_launch_dialog.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

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

/// A live status Kanban: one column per present state, not one long list.
/// The actual Terminal is inspected in a separate dismissible modal.
class GlobalAgentDashboardPage extends ConsumerStatefulWidget {
  const GlobalAgentDashboardPage({super.key});

  @override
  ConsumerState<GlobalAgentDashboardPage> createState() =>
      _GlobalAgentDashboardPageState();
}

/// Keep card widths readable. Three distinct statuses produce three columns;
/// narrow windows scroll horizontally instead of crushing the cards.
@visibleForTesting
double globalAgentDashboardColumnWidth(double viewportWidth, int columns) {
  if (columns < 1) return 0;
  const gap = 12.0;
  return math.max(
    280.0,
    math.min(385.0, (viewportWidth - gap * (columns - 1)) / columns),
  );
}

class _GlobalAgentDashboardPageState
    extends ConsumerState<GlobalAgentDashboardPage> {
  final ScrollController _horizontal = ScrollController();
  final Set<({String sessionId, String workspaceId, String tabId})>
  _hiddenCards = {};
  String _query = '';
  bool _historyMode = false;
  bool _showHidden = false;
  bool _needsReviewOnly = false;
  bool _openingInspector = false;
  final Set<String> _pinningTabIds = <String>{};
  String? _feedback;

  ({String sessionId, String workspaceId, String tabId}) _identity(
    GlobalAgentLiveRow row,
  ) => (
    sessionId: row.tab.terminalSessionId,
    workspaceId: row.status.workspaceId,
    tabId: row.tab.id,
  );

  @override
  void dispose() {
    _horizontal.dispose();
    super.dispose();
  }

  Future<void> _launch() async {
    final created = await showDialog<String>(
      context: context,
      builder: (_) => const GlobalAgentLaunchDialog(),
    );
    if (mounted && created != null) {
      setState(
        () => _feedback = 'Agent created. Waiting for verified Host status.',
      );
    }
  }

  void _toggleHidden(GlobalAgentLiveRow row) {
    setState(() {
      if (!_hiddenCards.add(_identity(row))) {
        _hiddenCards.remove(_identity(row));
      }
    });
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
      setState(() => _feedback = 'Agent Session changed. Pin was not saved.');
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
      if (mounted) setState(() => _feedback = 'Could not save this Agent pin.');
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
      setState(() => _feedback = 'Agent Session changed. Refresh the board.');
      return;
    }
    final workspace = workbench.workspacesByProject.values
        .expand((entries) => entries)
        .where((entry) => entry.id == identity.workspaceId)
        .firstOrNull;
    if (workspace == null) {
      setState(() => _feedback = 'The original Worktree is not available.');
      return;
    }
    // Reuse the same session identity and rendering handle. No new Agent tab
    // is created, and the Dashboard stays in place behind the inspector.
    final runtime = ref.read(terminalRuntimeProvider);
    final session = runtime.sessionFor(workspace: workspace, tab: row.tab);
    if (session.terminalSessionId != null &&
        session.terminalSessionId != identity.sessionId) {
      setState(() => _feedback = 'Terminal Session identity changed.');
      return;
    }
    final overlay = ref.read(globalAgentInspectionOpenProvider.notifier);
    setState(() {
      _openingInspector = true;
      _feedback = null;
    });
    try {
      // The background Workspace uses the same GlobalKey-backed terminal
      // renderer. Let Flutter unmount it before mounting the Dialog view.
      overlay.open();
      await WidgetsBinding.instance.endOfFrame;
      if (!mounted) return;
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

  String _elapsed(DateTime at) {
    final elapsed = DateTime.now().difference(at);
    if (elapsed.isNegative || elapsed.inMinutes < 1) return 'Just now';
    if (elapsed.inHours < 1) return '${elapsed.inMinutes}m ago';
    if (elapsed.inDays < 1) return '${elapsed.inHours}h ago';
    return '${elapsed.inDays}d ago';
  }

  Widget _agentCard(GlobalAgentLiveRow row, GlobalAgentDashboardGroup group) {
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);
    final hidden = _hiddenCards.contains(_identity(row));
    final pinned = row.tab.isPinned;
    return Material(
      color: AleraTokens.surface,
      borderRadius: BorderRadius.circular(10),
      child: InkWell(
        key: ValueKey<String>('agent-card-${row.tab.id}'),
        onTap: _openingInspector ? null : () => _inspect(row),
        borderRadius: BorderRadius.circular(10),
        child: Container(
          padding: const EdgeInsets.all(14),
          decoration: BoxDecoration(
            border: Border.all(
              color: row.needsReview
                  ? _groupColor(group).withValues(alpha: 0.7)
                  : AleraTokens.borderSubtle,
            ),
            borderRadius: BorderRadius.circular(10),
          ),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: <Widget>[
              Row(
                children: <Widget>[
                  Expanded(
                    child: Text(
                      row.project,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: Theme.of(context).textTheme.titleSmall,
                    ),
                  ),
                  if (row.needsReview)
                    Tooltip(
                      message: 'Agent may need your attention',
                      child: Icon(
                        Icons.notifications_active_outlined,
                        size: 16,
                        color: _groupColor(group),
                      ),
                    ),
                  IconButton(
                    tooltip: pinned ? 'Unpin Agent' : 'Pin Agent to top',
                    icon: Icon(
                      pinned ? Icons.push_pin : Icons.push_pin_outlined,
                      color: pinned ? _groupColor(group) : null,
                      size: 17,
                    ),
                    iconSize: 17,
                    visualDensity: VisualDensity.compact,
                    onPressed: _pinningTabIds.contains(row.tab.id)
                        ? null
                        : () => _togglePinned(row),
                  ),
                  IconButton(
                    tooltip: hidden ? 'Show Agent card' : 'Hide Agent card',
                    icon: Icon(
                      hidden
                          ? Icons.visibility_outlined
                          : Icons.visibility_off_outlined,
                      size: 16,
                    ),
                    iconSize: 16,
                    visualDensity: VisualDensity.compact,
                    onPressed: () => _toggleHidden(row),
                  ),
                ],
              ),
              Text(
                row.workspace,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: muted,
              ),
              const SizedBox(height: 10),
              Text(
                row.goal.isEmpty ? row.tab.title : row.goal,
                maxLines: 3,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodyMedium,
              ),
              if (row.progress.trim().isNotEmpty) ...<Widget>[
                const SizedBox(height: 7),
                Text(
                  row.progress,
                  maxLines: 2,
                  overflow: TextOverflow.ellipsis,
                  style: muted,
                ),
              ],
              const SizedBox(height: 12),
              Row(
                children: <Widget>[
                  Icon(
                    Icons.smart_toy_outlined,
                    size: 15,
                    color: _groupColor(group),
                  ),
                  const SizedBox(width: 5),
                  Expanded(
                    child: Text(
                      row.status.agentType.key,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: muted,
                    ),
                  ),
                  Text(_elapsed(row.status.updatedAt), style: muted),
                  const SizedBox(width: 6),
                  const Icon(Icons.open_in_full, size: 15),
                ],
              ),
            ],
          ),
        ),
      ),
    );
  }

  Widget _statusColumn(GlobalAgentDashboardSection section, double width) {
    final color = _groupColor(section.group);
    return SizedBox(
      width: width,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: <Widget>[
          Padding(
            padding: const EdgeInsets.fromLTRB(4, 4, 4, 12),
            child: Row(
              children: <Widget>[
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
              ],
            ),
          ),
          Expanded(
            child: ListView.separated(
              key: ValueKey<String>('agent-column-${section.group.name}'),
              itemCount: section.rows.length,
              separatorBuilder: (_, _) => const SizedBox(height: 10),
              itemBuilder: (_, index) =>
                  _agentCard(section.rows[index], section.group),
            ),
          ),
        ],
      ),
    );
  }

  Widget _board(List<GlobalAgentDashboardSection> columns) {
    if (columns.isEmpty) {
      return const Center(child: Text('No matching verified Agent Sessions.'));
    }
    return LayoutBuilder(
      builder: (context, constraints) {
        final columnWidth = globalAgentDashboardColumnWidth(
          constraints.maxWidth,
          columns.length,
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
    final workbench = ref.watch(workbenchControllerProvider);
    final statuses = ref.watch(agentStatusControllerProvider);
    final all = projectGlobalAgentLiveRows(workbench, statuses);
    final identities = all.map(_identity).toSet();
    _hiddenCards.removeWhere((id) => !identities.contains(id));
    final rows = all
        .where(
          (row) =>
              _showHidden ||
              row.needsReview ||
              !_hiddenCards.contains(_identity(row)),
        )
        .where((row) => !_needsReviewOnly || row.needsReview)
        .where((row) => row.matches(_query))
        .toList(growable: false);
    final columns = groupGlobalAgentLiveRows(rows, pinnedFirst: true);
    final needsReviewCount = all.where((row) => row.needsReview).length;
    final muted = Theme.of(context).textTheme.bodySmall
        ?.copyWith(color: AleraTokens.foregroundMuted);

    return Material(
      color: AleraTokens.bg,
      child: Padding(
        padding: const EdgeInsets.all(18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: <Widget>[
            Row(
              children: <Widget>[
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
            Text(
              'Each status has its own column. Select a card to inspect and control its live Agent.',
              style: muted,
            ),
            const SizedBox(height: 10),
            Row(
              children: <Widget>[
                TextButton.icon(
                  onPressed: () => setState(() => _historyMode = false),
                  icon: const Icon(Icons.view_kanban_outlined),
                  label: Text(_historyMode ? 'Live Agents' : '● Live Agents'),
                ),
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
            else ...<Widget>[
              const SizedBox(height: 8),
              TextField(
                autofocus: true,
                decoration: const InputDecoration(
                  prefixIcon: Icon(Icons.search),
                  hintText: 'Search project, Agent, goal or progress',
                  isDense: true,
                ),
                onChanged: (text) => setState(() => _query = text),
              ),
              const SizedBox(height: 10),
              Row(
                children: <Widget>[
                  Expanded(
                    child: Text(
                      '${rows.length} of ${all.length} Agents · ${columns.length} status columns',
                      style: muted,
                    ),
                  ),
                  FilterChip(
                    label: Text('Needs you ($needsReviewCount)'),
                    avatar: const Icon(Icons.notifications_none, size: 18),
                    selected: _needsReviewOnly,
                    onSelected: (selected) =>
                        setState(() => _needsReviewOnly = selected),
                  ),
                  const SizedBox(width: 6),
                  TextButton(
                    onPressed: () => setState(() => _showHidden = !_showHidden),
                    child: Text(
                      _showHidden
                          ? 'Show visible only'
                          : 'Hidden (${_hiddenCards.length})',
                    ),
                  ),
                ],
              ),
              if (_feedback != null)
                Padding(
                  padding: const EdgeInsets.only(bottom: 6),
                  child: Text(_feedback!, style: muted),
                ),
              const SizedBox(height: 8),
              Expanded(child: _board(columns)),
            ],
          ],
        ),
      ),
    );
  }
}
