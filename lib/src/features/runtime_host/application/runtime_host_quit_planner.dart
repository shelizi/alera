import 'package:alera/src/features/runtime_host/domain/runtime_host_quit_decision.dart';

enum RuntimeHostQuitAction {
  allowClose,
  softStop,
  confirm,
  leaveOpen,
  forceStop,
  cancel,
}

final class RuntimeHostQuitPlan {
  const RuntimeHostQuitPlan._(this.action, {this.commitVisualQuit = false});

  const RuntimeHostQuitPlan.allowClose()
    : this._(RuntimeHostQuitAction.allowClose);

  const RuntimeHostQuitPlan.softStop() : this._(RuntimeHostQuitAction.softStop);

  const RuntimeHostQuitPlan.confirm() : this._(RuntimeHostQuitAction.confirm);

  const RuntimeHostQuitPlan.leaveOpen({bool commitVisualQuit = false})
    : this._(
        RuntimeHostQuitAction.leaveOpen,
        commitVisualQuit: commitVisualQuit,
      );

  const RuntimeHostQuitPlan.forceStop({bool commitVisualQuit = false})
    : this._(
        RuntimeHostQuitAction.forceStop,
        commitVisualQuit: commitVisualQuit,
      );

  const RuntimeHostQuitPlan.cancel() : this._(RuntimeHostQuitAction.cancel);

  final RuntimeHostQuitAction action;
  final bool commitVisualQuit;

  @override
  bool operator ==(Object other) {
    return other is RuntimeHostQuitPlan &&
        other.action == action &&
        other.commitVisualQuit == commitVisualQuit;
  }

  @override
  int get hashCode => Object.hash(action, commitVisualQuit);
}

final class RuntimeHostBusyState {
  const RuntimeHostBusyState({
    this.activeAgents = 0,
    this.activeSessions = 0,
    this.activeJobs = 0,
    this.activePushSubscriptions = 0,
  });

  final int activeAgents;
  final int activeSessions;
  final int activeJobs;
  final int activePushSubscriptions;

  bool get isPushOnly {
    return activePushSubscriptions > 0 &&
        activeAgents == 0 &&
        activeSessions == 0 &&
        activeJobs == 0;
  }
}

final class RuntimeHostQuitPlanner {
  const RuntimeHostQuitPlanner();

  RuntimeHostQuitPlan initialPlan({
    required bool keepRuntimeOpen,
    required bool statusKnown,
    required bool runtimeRunning,
    required bool persistent,
  }) {
    if (keepRuntimeOpen) {
      return const RuntimeHostQuitPlan.allowClose();
    }
    if (statusKnown && !runtimeRunning) {
      return const RuntimeHostQuitPlan.allowClose();
    }
    if (runtimeRunning && persistent) {
      return const RuntimeHostQuitPlan.allowClose();
    }
    return const RuntimeHostQuitPlan.softStop();
  }

  RuntimeHostQuitPlan busyPlan(
    RuntimeHostBusyState busy, {
    RuntimeHostQuitDecision? decision,
  }) {
    if (busy.isPushOnly) {
      return const RuntimeHostQuitPlan.leaveOpen(commitVisualQuit: true);
    }
    return switch (decision) {
      null => const RuntimeHostQuitPlan.confirm(),
      RuntimeHostQuitDecision.cancel => const RuntimeHostQuitPlan.cancel(),
      RuntimeHostQuitDecision.leaveRuntimeOpen =>
        const RuntimeHostQuitPlan.leaveOpen(commitVisualQuit: true),
      RuntimeHostQuitDecision.forceStop => const RuntimeHostQuitPlan.forceStop(
        commitVisualQuit: true,
      ),
    };
  }
}
