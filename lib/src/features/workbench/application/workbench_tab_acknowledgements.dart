import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'workbench_tab_acknowledgements.g.dart';

/// Session-local acknowledgement of the exact completion epoch a user viewed.
///
/// Kept as a provider rather than widget state so the sidebar row builder can
/// bucket unacked completions next to the tab strip that marks them read.
/// Deliberately not persisted: a done status restored on the next launch reads
/// as unread again.
@Riverpod(keepAlive: true)
class WorkbenchTabCompletionAcknowledgementsController
    extends _$WorkbenchTabCompletionAcknowledgementsController {
  @override
  Map<String, DateTime> build() => <String, DateTime>{};

  /// Records that the user saw this exact completion. Only a done status can
  /// be acknowledged; repeat calls with the same epoch are no-ops.
  void acknowledge(AgentStatusEntry? status) {
    if (status?.state != AgentStatusState.done) {
      return;
    }
    if (state[status!.terminalSessionId] == status.stateStartedAt) {
      return;
    }
    state = <String, DateTime>{
      ...state,
      status.terminalSessionId: status.stateStartedAt,
    };
  }

  /// Drops acknowledgements for terminal sessions that no longer exist.
  void retainTerminalSessions(Set<String> terminalSessionIds) {
    if (state.keys.every(terminalSessionIds.contains)) {
      return;
    }
    state = <String, DateTime>{
      for (final entry in state.entries)
        if (terminalSessionIds.contains(entry.key)) entry.key: entry.value,
    };
  }
}

/// Whether this exact completion epoch (`stateStartedAt`) has been viewed.
/// Only a done status can be acknowledged.
bool isCompletionAcknowledged(
  Map<String, DateTime> acknowledged,
  AgentStatusEntry? status,
) {
  if (status?.state != AgentStatusState.done) {
    return false;
  }
  return acknowledged[status!.terminalSessionId] == status.stateStartedAt;
}
