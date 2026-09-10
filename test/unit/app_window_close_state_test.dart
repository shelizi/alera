import 'package:alera/src/features/app_window/application/app_window_close_state.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('AppWindowCloseStateMachine', () {
    test('starts idle', () {
      const state = AppWindowCloseStateMachine();
      expect(state.phase, AppWindowClosePhase.idle);
      expect(state.isClosing, isFalse);
      expect(state.isQuitting, isFalse);
      expect(state.isCommitted, isFalse);
      expect(state.isClosed, isFalse);
    });

    test('user close can begin only from idle', () {
      const state = AppWindowCloseStateMachine();
      final closing = state.beginUserClose();

      expect(closing.phase, AppWindowClosePhase.userClosing);
      expect(closing.isClosing, isTrue);
      expect(closing.beginUserClose(), same(closing));
    });

    test('quit is distinct from ordinary user close', () {
      const state = AppWindowCloseStateMachine();
      final quitting = state.beginQuit();

      expect(quitting.phase, AppWindowClosePhase.quitting);
      expect(quitting.isClosing, isTrue);
      expect(quitting.isQuitting, isTrue);
    });

    test('cancelled close returns to idle', () {
      const state = AppWindowCloseStateMachine();
      final cancelled = state.beginQuit().cancel();

      expect(cancelled.phase, AppWindowClosePhase.idle);
      expect(cancelled.isClosing, isFalse);
      expect(cancelled.isQuitting, isFalse);
    });

    test('committed close suppresses active logging before destruction', () {
      const state = AppWindowCloseStateMachine();
      final committed = state.beginQuit().commit();

      expect(committed.phase, AppWindowClosePhase.committed);
      expect(committed.isClosing, isTrue);
      expect(committed.isCommitted, isTrue);
      expect(committed.isClosed, isFalse);
    });

    test('completed close is terminal', () {
      const state = AppWindowCloseStateMachine();
      final closed = state.beginQuit().commit().complete();

      expect(closed.phase, AppWindowClosePhase.closed);
      expect(closed.isClosing, isTrue);
      expect(closed.isClosed, isTrue);
      expect(closed.cancel(), same(closed));
      expect(closed.beginQuit(), same(closed));
    });
  });
}
