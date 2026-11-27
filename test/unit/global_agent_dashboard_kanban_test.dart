import 'package:alera/src/features/workbench/presentation/global_agent_dashboard_dialog.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_inspector_dialog.dart';

import 'dart:ui' show Size;

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('Kanban status column sizing', () {
    test('five fixed lanes fit a standard wide viewport', () {
      expect(globalAgentDashboardColumnWidth(1600), closeTo(310.4, 0.01));
    });
    test('wide screens retain the readable maximum width', () {
      expect(globalAgentDashboardColumnWidth(2200), 385);
    });
    test('narrow windows scroll rather than crush cards', () {
      expect(globalAgentDashboardColumnWidth(620), 280);
    });
  });

  group('Agent inspector sizing', () {
    test('uses nearly all of a standard desktop window', () {
      expect(
        globalAgentInspectorDefaultSize(const Size(1500, 900)),
        const Size(1436, 836),
      );
    });
    test('scales up past the previous 1490 by 950 dialog cap', () {
      expect(
        globalAgentInspectorDefaultSize(const Size(1920, 1200)),
        const Size(1856, 1136),
      );
    });
    test('caps the dialog on very large desktop windows', () {
      expect(
        globalAgentInspectorDefaultSize(const Size(3440, 2160)),
        const Size(2400, 1400),
      );
    });
  });
  group('Agent inspector Escape routing', () {
    test('Escape key down is reserved for the Agent', () {
      expect(
        isAgentInspectorEscape(
          KeyDownEvent(
            physicalKey: PhysicalKeyboardKey.escape,
            logicalKey: LogicalKeyboardKey.escape,
            timeStamp: Duration.zero,
          ),
        ),
        isTrue,
      );
    });
    test('Escape key up does not send a duplicate ESC byte', () {
      expect(
        isAgentInspectorEscape(
          KeyUpEvent(
            physicalKey: PhysicalKeyboardKey.escape,
            logicalKey: LogicalKeyboardKey.escape,
            timeStamp: Duration.zero,
          ),
        ),
        isFalse,
      );
    });
    test('other keys are not intercepted', () {
      expect(
        isAgentInspectorEscape(
          KeyDownEvent(
            physicalKey: PhysicalKeyboardKey.keyA,
            logicalKey: LogicalKeyboardKey.keyA,
            timeStamp: Duration.zero,
          ),
        ),
        isFalse,
      );
    });
  });
}
