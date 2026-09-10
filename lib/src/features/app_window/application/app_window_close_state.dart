enum AppWindowClosePhase { idle, userClosing, quitting, committed, closed }

final class AppWindowCloseStateMachine {
  const AppWindowCloseStateMachine({
    this.phase = AppWindowClosePhase.idle,
    this.quitIntent = false,
  });

  final AppWindowClosePhase phase;
  final bool quitIntent;

  bool get isClosing => phase != AppWindowClosePhase.idle;
  bool get isQuitting => quitIntent && phase != AppWindowClosePhase.idle;
  bool get isCommitted =>
      phase == AppWindowClosePhase.committed ||
      phase == AppWindowClosePhase.closed;
  bool get isClosed => phase == AppWindowClosePhase.closed;

  AppWindowCloseStateMachine beginUserClose() {
    if (phase != AppWindowClosePhase.idle) {
      return this;
    }
    return const AppWindowCloseStateMachine(
      phase: AppWindowClosePhase.userClosing,
    );
  }

  AppWindowCloseStateMachine beginQuit() {
    if (phase != AppWindowClosePhase.idle) {
      return this;
    }
    return const AppWindowCloseStateMachine(
      phase: AppWindowClosePhase.quitting,
      quitIntent: true,
    );
  }

  AppWindowCloseStateMachine cancel() {
    if (phase != AppWindowClosePhase.userClosing &&
        phase != AppWindowClosePhase.quitting) {
      return this;
    }
    return const AppWindowCloseStateMachine();
  }

  AppWindowCloseStateMachine commit() {
    if (phase != AppWindowClosePhase.userClosing &&
        phase != AppWindowClosePhase.quitting) {
      return this;
    }
    return AppWindowCloseStateMachine(
      phase: AppWindowClosePhase.committed,
      quitIntent: quitIntent,
    );
  }

  AppWindowCloseStateMachine complete() {
    if (phase == AppWindowClosePhase.closed) {
      return this;
    }
    if (phase != AppWindowClosePhase.committed) {
      return this;
    }
    return AppWindowCloseStateMachine(
      phase: AppWindowClosePhase.closed,
      quitIntent: quitIntent,
    );
  }
}
