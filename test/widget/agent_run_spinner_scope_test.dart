import 'package:alera/src/features/workbench/presentation/widgets/agent_run_spinner_scope.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Widget _host({required bool spinning}) => Directionality(
  textDirection: TextDirection.ltr,
  child: AgentRunSpinnerScope(
    child: Center(
      child: spinning
          ? const AgentRunSharedSpinner(
              size: 14,
              color: Color(0xFFFFFFFF),
              strokeWidth: 1.5,
            )
          : const SizedBox.shrink(),
    ),
  ),
);

bool _running(WidgetTester tester) =>
    (tester.state(find.byType(AgentRunSpinnerScope)) as dynamic).isRunning
        as bool;

double _progress(WidgetTester tester) {
  final paint = tester.widget<CustomPaint>(
    find.descendant(
      of: find.byType(AgentRunSharedSpinner),
      matching: find.byType(CustomPaint),
    ),
  );
  return (paint.painter! as AgentRunSpinnerPainter).progress.value;
}

void main() {
  testWidgets('an idle scope runs no clock', (tester) async {
    await tester.pumpWidget(_host(spinning: false));

    expect(_running(tester), isFalse);
    expect(tester.binding.hasScheduledFrame, isFalse);
  });

  testWidgets('spinners advance on the paced clock, not every vsync', (
    tester,
  ) async {
    await tester.pumpWidget(_host(spinning: true));
    expect(_running(tester), isTrue);
    // Nothing is waiting on the next vsync between clock steps.
    await tester.pump();
    expect(tester.binding.hasScheduledFrame, isFalse);

    final before = _progress(tester);
    await tester.pump(agentRunSpinnerFrameInterval);
    expect(_progress(tester), isNot(before));

    await tester.pumpWidget(_host(spinning: false));
    expect(_running(tester), isFalse);
  });

  testWidgets('the clock stops while the window is hidden', (tester) async {
    await tester.pumpWidget(_host(spinning: true));
    expect(_running(tester), isTrue);

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.hidden);
    expect(_running(tester), isFalse);

    tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
    expect(_running(tester), isTrue);
  });

  testWidgets('a disabled ticker mode pauses the clock', (tester) async {
    await tester.pumpWidget(
      TickerMode(enabled: false, child: _host(spinning: true)),
    );

    expect(_running(tester), isFalse);
  });
}
