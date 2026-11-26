import 'package:alera/src/features/workbench/presentation/global_agent_dashboard_dialog.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_inspector_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('outside tap closes Agent inspector, Escape does not', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(1200, 900);
    tester.view.devicePixelRatio = 1;
    addTearDown(() {
      tester.view.resetPhysicalSize();
      tester.view.resetDevicePixelRatio();
    });
    var escapeCount = 0;

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () {
                showAgentInspectorModal(
                  context,
                  Shortcuts(
                    shortcuts: const <ShortcutActivator, Intent>{
                      SingleActivator(LogicalKeyboardKey.escape):
                          DoNothingIntent(),
                    },
                    child: PopScope(
                      canPop: true,
                      child: Focus(
                        autofocus: true,
                        onKeyEvent: (node, event) {
                          if (!isAgentInspectorEscape(event)) {
                            return KeyEventResult.ignored;
                          }
                          escapeCount += 1;
                          return KeyEventResult.handled;
                        },
                        child: const Dialog(
                          child: SizedBox(
                            width: 500,
                            height: 320,
                            child: Center(child: Text('Live Agent Terminal')),
                          ),
                        ),
                      ),
                    ),
                  ),
                );
              },
              child: const Text('Inspect'),
            ),
          ),
        ),
      ),
    );

    await tester.tap(find.text('Inspect'));
    await tester.pumpAndSettle();
    expect(find.text('Live Agent Terminal'), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(escapeCount, 1);
    expect(find.text('Live Agent Terminal'), findsOneWidget);

    await tester.tapAt(const Offset(20, 20));
    await tester.pumpAndSettle();
    expect(find.text('Live Agent Terminal'), findsNothing);
    expect(find.text('Inspect'), findsOneWidget);
  });
}
