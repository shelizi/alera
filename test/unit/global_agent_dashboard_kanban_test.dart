import 'package:alera/src/features/workbench/presentation/global_agent_dashboard_dialog.dart';
import 'package:alera/src/features/workbench/presentation/global_agent_inspector_dialog.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('Kanban status column sizing', () {
    test('there is no column when there are no statuses', () {
      expect(globalAgentDashboardColumnWidth(1100, 0), 0);
    });
    test('one status does not make a giant full-width card', () {
      expect(globalAgentDashboardColumnWidth(1200, 1), 385);
    });
    test('three statuses produce three readable columns', () {
      expect(globalAgentDashboardColumnWidth(1100, 3), closeTo(358.666, 0.01));
    });
    test('narrow windows scroll rather than crush cards', () {
      expect(globalAgentDashboardColumnWidth(620, 3), 280);
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
