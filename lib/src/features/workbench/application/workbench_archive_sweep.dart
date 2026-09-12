import 'dart:async';

import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/application/workspace_activity_controller.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'workbench_archive_sweep.g.dart';

const Duration workbenchArchiveSweepInterval = Duration(hours: 1);

/// Session-local collapse state for Archived group headers, keyed by
/// `WorkbenchArchivedHeaderRow.key`. Deliberately kept out of
/// [WorkbenchViewPrefs] so it never persists or syncs to other clients.
@Riverpod(keepAlive: true)
class WorkbenchArchivedSectionsCollapse
    extends _$WorkbenchArchivedSectionsCollapse {
  @override
  Set<String> build() => const <String>{};

  void toggle(String key) {
    final next = Set<String>.from(state);
    if (!next.remove(key)) {
      next.add(key);
    }
    state = next;
  }
}

/// Whether [workspace] qualifies for automatic archiving: a non-default,
/// unpinned, non-archived workspace with no open tabs, no unfinished agent
/// run, and no recorded activity within [threshold].
bool workspaceEligibleForAutoArchive({
  required Workspace workspace,
  required WorkbenchState state,
  required Map<String, DateTime> lastActivityByWorkspaceId,
  required Map<String, AgentStatusEntry> agentStatuses,
  required Duration threshold,
  required DateTime now,
}) {
  if (workspace.isMain || workspace.isPinned || workspace.isArchived) {
    return false;
  }
  if (workspace.id == state.activeWorkspaceId) {
    return false;
  }
  if (state.tabsFor(workspace.id).isNotEmpty) {
    return false;
  }
  final hasUnfinishedRun = agentStatuses.values.any(
    (entry) =>
        entry.workspaceId == workspace.id &&
        entry.state != AgentStatusState.done,
  );
  if (hasUnfinishedRun) {
    return false;
  }
  var lastActive = workspace.updatedAt.isAfter(workspace.createdAt)
      ? workspace.updatedAt
      : workspace.createdAt;
  final recorded = lastActivityByWorkspaceId[workspace.id];
  if (recorded != null && recorded.isAfter(lastActive)) {
    lastActive = recorded;
  }
  return now.difference(lastActive) >= threshold;
}

/// Archives idle workspaces once the workbench has bootstrapped and then once
/// per [workbenchArchiveSweepInterval]. The threshold comes from
/// `GeneralSettings.autoArchiveWorkspacesAfterDays`; 0 disables the sweep.
@Riverpod(keepAlive: true)
void workbenchArchiveSweepCoordinator(Ref ref) {
  var inFlight = false;

  Future<void> sweep() async {
    final days = ref
        .read(settingsControllerProvider)
        .general
        .autoArchiveWorkspacesAfterDays;
    if (days <= 0 || inFlight) {
      return;
    }
    final state = ref.read(workbenchControllerProvider);
    if (!state.bootstrapped) {
      return;
    }
    inFlight = true;
    try {
      final activity = ref.read(workspaceActivityControllerProvider);
      final statuses = ref.read(agentStatusControllerProvider);
      final threshold = Duration(days: days);
      final now = DateTime.now().toUtc();
      final controller = ref.read(workbenchControllerProvider.notifier);
      for (final project in state.projects) {
        for (final workspace in state.workspacesFor(project.id)) {
          if (!workspaceEligibleForAutoArchive(
            workspace: workspace,
            state: state,
            lastActivityByWorkspaceId: activity,
            agentStatuses: statuses,
            threshold: threshold,
            now: now,
          )) {
            continue;
          }
          try {
            await controller.archiveWorkspace(workspace);
          } catch (_) {
            // archiveWorkspace already records the failure on state; one bad
            // workspace must not stop the rest of the sweep.
          }
        }
      }
    } finally {
      inFlight = false;
    }
  }

  ref.listen(
    workbenchControllerProvider.select((state) => state.bootstrapped),
    (previous, next) {
      if (next) {
        unawaited(sweep());
      }
    },
    fireImmediately: true,
  );
  final timer = Timer.periodic(
    workbenchArchiveSweepInterval,
    (_) => unawaited(sweep()),
  );
  ref.onDispose(timer.cancel);
}
