import 'package:alera/src/features/runtime_host/application/runtime_host_quit_planner.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_quit_decision.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('RuntimeHostQuitPlanner', () {
    const planner = RuntimeHostQuitPlanner();

    test(
      'keeps an explicitly preserved runtime open without probing shutdown',
      () {
        expect(
          planner.initialPlan(
            keepRuntimeOpen: true,
            statusKnown: true,
            runtimeRunning: true,
            persistent: false,
          ),
          const RuntimeHostQuitPlan.allowClose(),
        );
      },
    );

    test('keeps persistent runtime open', () {
      expect(
        planner.initialPlan(
          keepRuntimeOpen: false,
          statusKnown: true,
          runtimeRunning: true,
          persistent: true,
        ),
        const RuntimeHostQuitPlan.allowClose(),
      );
    });

    test('requests soft shutdown for an app-owned runtime', () {
      expect(
        planner.initialPlan(
          keepRuntimeOpen: false,
          statusKnown: true,
          runtimeRunning: true,
          persistent: false,
        ),
        const RuntimeHostQuitPlan.softStop(),
      );
    });

    test('still requests soft shutdown when probe status is uncertain', () {
      expect(
        planner.initialPlan(
          keepRuntimeOpen: false,
          statusKnown: false,
          runtimeRunning: false,
          persistent: false,
        ),
        const RuntimeHostQuitPlan.softStop(),
      );
    });

    test('does not request shutdown for a known stopped runtime', () {
      expect(
        planner.initialPlan(
          keepRuntimeOpen: false,
          statusKnown: true,
          runtimeRunning: false,
          persistent: false,
        ),
        const RuntimeHostQuitPlan.allowClose(),
      );
    });

    test('auto-leaves a push-only runtime open and commits visual quit', () {
      expect(
        planner.busyPlan(
          const RuntimeHostBusyState(activePushSubscriptions: 1),
        ),
        const RuntimeHostQuitPlan.leaveOpen(commitVisualQuit: true),
      );
    });

    test('asks for a decision when non-push work is busy', () {
      expect(
        planner.busyPlan(const RuntimeHostBusyState(activeSessions: 1)),
        const RuntimeHostQuitPlan.confirm(),
      );
    });

    test('maps user cancel without committing visual quit', () {
      expect(
        planner.busyPlan(
          const RuntimeHostBusyState(activeAgents: 1),
          decision: RuntimeHostQuitDecision.cancel,
        ),
        const RuntimeHostQuitPlan.cancel(),
      );
    });

    test('maps leave-open to a committed visual quit', () {
      expect(
        planner.busyPlan(
          const RuntimeHostBusyState(activeJobs: 1),
          decision: RuntimeHostQuitDecision.leaveRuntimeOpen,
        ),
        const RuntimeHostQuitPlan.leaveOpen(commitVisualQuit: true),
      );
    });

    test('maps force-stop to a committed visual quit before shutdown', () {
      expect(
        planner.busyPlan(
          const RuntimeHostBusyState(activeSessions: 1),
          decision: RuntimeHostQuitDecision.forceStop,
        ),
        const RuntimeHostQuitPlan.forceStop(commitVisualQuit: true),
      );
    });
  });
}
