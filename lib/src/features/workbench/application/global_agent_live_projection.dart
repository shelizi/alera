import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_context_overview.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

typedef GlobalAgentIdentity = ({
  String sessionId,
  String workspaceId,
  String tabId,
});

typedef _AgentInventory = ({
  List<Project> projects,
  Map<String, List<Workspace>> workspaces,
  Map<String, List<WorkspaceTabRecord>> tabs,
});

_AgentInventory _inventoryOf(WorkbenchState state) => (
  projects: state.projects,
  workspaces: state.workspacesByProject,
  tabs: state.tabsByWorkspace,
);

/// Deterministic dashboard sections. Every Agent appears in exactly one
/// section; a finished/interrupted turn does not appear as Waiting.
enum GlobalAgentDashboardGroup {
  blocked('Blocked'),
  waiting('Waiting'),
  working('Working'),
  interrupted('Interrupted'),
  done('Done');

  GlobalAgentDashboardGroup(this.label);
  final String label;

  static GlobalAgentDashboardGroup forStatus(AgentStatusEntry status) {
    if (status.interrupted == true) return interrupted;
    return switch (status.state) {
      AgentStatusState.blocked => blocked,
      AgentStatusState.waiting => waiting,
      AgentStatusState.working => working,
      AgentStatusState.done => done,
    };
  }
}

class GlobalAgentDashboardSection {
  const GlobalAgentDashboardSection(this.group, this.rows);
  final GlobalAgentDashboardGroup group;
  final List<GlobalAgentLiveRow> rows;
}

/// Group live Agent cards by current Hook state, newest first within each
/// section. Empty sections are omitted, so the viewport stays compact.
List<GlobalAgentDashboardSection> groupGlobalAgentLiveRows(
  Iterable<GlobalAgentLiveRow> rows, {
  bool alreadySorted = false,
}) {
  final grouped = <GlobalAgentDashboardGroup, List<GlobalAgentLiveRow>>{};
  for (final row in rows) {
    grouped
        .putIfAbsent(
          GlobalAgentDashboardGroup.forStatus(row.status),
          () => <GlobalAgentLiveRow>[],
        )
        .add(row);
  }
  return <GlobalAgentDashboardSection>[
    for (final group in GlobalAgentDashboardGroup.values)
      if (grouped[group] case final sectionRows?)
        GlobalAgentDashboardSection(
          group,
          alreadySorted
              ? sectionRows
              : (sectionRows..sort(
                  (a, b) => b.status.updatedAt.compareTo(a.status.updatedAt),
                )),
        ),
  ];
}

/// Flatten status sections for a single-card-per-row Dashboard. A collapsed
/// section still renders its label/count and hides only its Agent cards;
/// no Host data or Session ownership is changed.
List<Object> globalAgentDashboardVisibleItems(
  Iterable<GlobalAgentLiveRow> rows,
  Set<GlobalAgentDashboardGroup> collapsed, {
  bool alreadySorted = false,
}) => <Object>[
  for (final section in groupGlobalAgentLiveRows(
    rows,
    alreadySorted: alreadySorted,
  )) ...<Object>[
    section,
    if (!collapsed.contains(section.group)) ...section.rows,
  ],
];

/// Live, verified workspace-tab projection. Historical provider transcripts
/// are intentionally excluded until the shared Host search API is available.
class GlobalAgentLiveRow {
  GlobalAgentLiveRow({
    required this.project,
    required this.workspace,
    required this.tab,
    required this.status,
    required this.goal,
    required this.progress,
    required this.timeline,
  });

  final String project;
  final String workspace;
  final WorkspaceTabRecord tab;
  final AgentStatusEntry status;
  final String goal;
  final String progress;

  /// Host-validated context events; no raw terminal output or tool inputs.
  final List<AgentContextMilestone> timeline;

  /// Last verified Hook state suggests review, not proven user approval.
  bool get needsReview =>
      status.interrupted != true &&
      (status.state == AgentStatusState.waiting ||
          status.state == AgentStatusState.blocked);

  late final GlobalAgentIdentity identity = (
    sessionId: tab.terminalSessionId,
    workspaceId: status.workspaceId,
    tabId: tab.id,
  );

  late final String _searchText =
      '$project $workspace ${tab.title} ${status.agentType.key} $goal $progress'
          .toLowerCase();

  bool matchesTerms(List<String> terms) =>
      terms.isEmpty || terms.every(_searchText.contains);

  bool matches(String query) => matchesTerms(globalAgentSearchTerms(query));
}

final _searchWhitespace = RegExp(r'\s+');

List<String> globalAgentSearchTerms(String query) {
  final normalized = query.trim().toLowerCase();
  return normalized.isEmpty ? const [] : normalized.split(_searchWhitespace);
}

typedef _RegisteredAgentTab = ({
  String project,
  String workspace,
  WorkspaceTabRecord tab,
  int ordinal,
});

/// Cache scoped to the mounted live Dashboard. Reindex only when registered
/// projects/workspaces/tabs change. A Host delta visits live entries rather
/// than every historical/idle tab, and unchanged rows keep their identity.
class GlobalAgentLiveProjectionCache {
  _AgentInventory? _inventory;
  var _registered = <GlobalAgentIdentity, _RegisteredAgentTab>{};
  var _rows = <GlobalAgentIdentity, GlobalAgentLiveRow>{};
  List<GlobalAgentLiveRow> _sorted = const [];

  List<GlobalAgentLiveRow> project(
    WorkbenchState state,
    Map<String, AgentStatusEntry> statuses,
  ) => _project(_inventoryOf(state), statuses);

  void _reindex(_AgentInventory inventory) {
    final registered = <GlobalAgentIdentity, _RegisteredAgentTab>{};
    var ordinal = 0;
    for (final project in inventory.projects) {
      for (final workspace
          in inventory.workspaces[project.id] ?? const <Workspace>[]) {
        if (!workspace.isActive) continue;
        for (final tab
            in inventory.tabs[workspace.id] ?? const <WorkspaceTabRecord>[]) {
          if (tab.kind != WorkspaceTabKind.terminal) continue;
          registered[(
            sessionId: tab.terminalSessionId,
            workspaceId: workspace.id,
            tabId: tab.id,
          )] = (
            project: project.name,
            workspace: workspace.name,
            tab: tab,
            ordinal: ordinal++,
          );
        }
      }
    }
    _registered = registered;
    _inventory = inventory;
  }

  List<GlobalAgentLiveRow> _project(
    _AgentInventory inventory,
    Map<String, AgentStatusEntry> statuses,
  ) {
    final inventoryChanged = _inventory != inventory;
    if (inventoryChanged) _reindex(inventory);
    final next = <GlobalAgentIdentity, GlobalAgentLiveRow>{};
    final changed = <GlobalAgentLiveRow>[];
    for (final item in statuses.entries) {
      final status = item.value;
      if (item.key != status.terminalSessionId) continue;
      final identity = (
        sessionId: item.key,
        workspaceId: status.workspaceId,
        tabId: status.tabId,
      );
      final registered = _registered[identity];
      if (registered == null) continue;
      final old = _rows[identity];
      if (old != null &&
          identical(old.status, status) &&
          identical(old.tab, registered.tab) &&
          old.project == registered.project &&
          old.workspace == registered.workspace) {
        next[identity] = old;
        continue;
      }
      final overview = AgentContextOverview.fromRaw(status.contextOverview);
      final row = GlobalAgentLiveRow(
        project: registered.project,
        workspace: registered.workspace,
        tab: registered.tab,
        status: status,
        goal: overview.goal.isNotEmpty ? overview.goal : status.prompt,
        progress: overview.displayProgress.isNotEmpty
            ? overview.displayProgress
            : (status.lastAssistantMessage ?? ''),
        timeline: overview.timeline,
      );
      next[identity] = row;
      changed.add(row);
    }

    if (!inventoryChanged && changed.isEmpty && next.length == _rows.length) {
      return _sorted;
    }
    // Inventory ordinal is a stable tie-break for equal Host timestamps.
    int compare(GlobalAgentLiveRow a, GlobalAgentLiveRow b) {
      final order = b.status.updatedAt.compareTo(a.status.updatedAt);
      return order != 0
          ? order
          : _registered[a.identity]!.ordinal.compareTo(
              _registered[b.identity]!.ordinal,
            );
    }

    final List<GlobalAgentLiveRow> sorted;
    if (inventoryChanged || changed.length == next.length) {
      // A new inventory can change every row's tie-break ordinal.
      sorted = next.values.toList()..sort(compare);
    } else {
      // Retained rows already have the correct relative order. Sort only
      // the changed rows and merge them, dropping old/removed instances.
      // A typical one-Agent delta therefore needs a linear merge, not
      // another full sort of every live card.
      changed.sort(compare);
      sorted = <GlobalAgentLiveRow>[];
      var index = 0;
      for (final row in _sorted) {
        if (!identical(next[row.identity], row)) continue;
        while (index < changed.length && compare(changed[index], row) < 0) {
          sorted.add(changed[index++]);
        }
        sorted.add(row);
      }
      while (index < changed.length) {
        sorted.add(changed[index++]);
      }
    }
    _rows = next;
    // Inventory can change without changing any registered live rows.
    if (sorted.length == _sorted.length) {
      var same = true;
      for (var i = 0; i < sorted.length; i++) {
        if (!identical(sorted[i], _sorted[i])) {
          same = false;
          break;
        }
      }
      if (same) return _sorted;
    }
    return _sorted = List<GlobalAgentLiveRow>.unmodifiable(sorted);
  }
}

List<GlobalAgentLiveRow> projectGlobalAgentLiveRows(
  WorkbenchState state,
  Map<String, AgentStatusEntry> statuses,
) => GlobalAgentLiveProjectionCache().project(state, statuses);

final _globalAgentLiveCacheProvider =
    Provider.autoDispose<GlobalAgentLiveProjectionCache>(
      (ref) => GlobalAgentLiveProjectionCache(),
    );

final globalAgentLiveRowsProvider =
    Provider.autoDispose<List<GlobalAgentLiveRow>>(
      (ref) => ref
          .watch(_globalAgentLiveCacheProvider)
          ._project(
            ref.watch(workbenchControllerProvider.select(_inventoryOf)),
            ref.watch(agentStatusControllerProvider),
          ),
    );

/// Preflight for Dashboard commands. The Host still validates the identity
/// atomically immediately before PTY input, preventing transfer races.
bool globalAgentTargetIsCurrent({
  required WorkbenchState workbench,
  required Map<String, AgentStatusEntry> statuses,
  required String sessionId,
  required String workspaceId,
  required String tabId,
}) {
  final entry = statuses[sessionId];
  if (entry == null ||
      entry.workspaceId != workspaceId ||
      entry.tabId != tabId ||
      entry.terminalSessionId != sessionId) {
    return false;
  }
  final registered = workbench.workspacesByProject.values.any(
    (workspaces) => workspaces.any(
      (workspace) => workspace.id == workspaceId && workspace.isActive,
    ),
  );
  return registered &&
      workbench
          .tabsFor(workspaceId)
          .any(
            (tab) =>
                tab.kind == WorkspaceTabKind.terminal &&
                tab.id == tabId &&
                tab.terminalSessionId == sessionId,
          );
}
