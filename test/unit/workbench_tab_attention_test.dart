import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_acknowledgements.dart';
import 'package:alera/src/features/workbench/application/workbench_tab_attention.dart';
import 'package:alera/src/features/workbench/presentation/workbench_tab_attention_presentation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final now = DateTime.utc(2026, 7, 9);

  AgentStatusEntry entry(AgentStatusState state, {DateTime? stateStartedAt}) {
    return AgentStatusEntry(
      terminalSessionId: 's1',
      workspaceId: 'w1',
      tabId: 't1',
      agentType: .claude,
      state: state,
      prompt: '',
      updatedAt: now,
      stateStartedAt: stateStartedAt ?? now,
    );
  }

  group('workbenchTabAttention', () {
    test('waiting and blocked always need attention', () {
      expect(
        workbenchTabAttention(
          status: entry(.waiting),
          completionAcknowledged: false,
        ),
        WorkbenchTabAttention.agentWaiting,
      );
      expect(
        workbenchTabAttention(
          status: entry(.blocked),
          completionAcknowledged: false,
        ),
        WorkbenchTabAttention.agentBlocked,
      );
    });

    test('done needs attention until its completion is acknowledged', () {
      expect(
        workbenchTabAttention(
          status: entry(.done),
          completionAcknowledged: false,
        ),
        WorkbenchTabAttention.agentDoneUnacked,
      );
      expect(
        workbenchTabAttention(
          status: entry(.done),
          completionAcknowledged: true,
        ),
        WorkbenchTabAttention.none,
      );
    });

    test('working has no extra attention', () {
      expect(
        workbenchTabAttention(
          status: entry(.working),
          completionAcknowledged: false,
        ),
        WorkbenchTabAttention.none,
      );
    });
  });

  group('workbenchTabAttentionDotColor', () {
    test('uses warning for unacked done', () {
      expect(
        workbenchTabAttentionDotColor(
          status: entry(.done),
          completionAcknowledged: false,
        ),
        AleraTokens.warning,
      );
    });
  });

  group('WorkbenchTabCompletionAcknowledgementsController', () {
    late ProviderContainer container;

    Map<String, DateTime> acknowledged() {
      return container.read(
        workbenchTabCompletionAcknowledgementsControllerProvider,
      );
    }

    WorkbenchTabCompletionAcknowledgementsController controller() {
      return container.read(
        workbenchTabCompletionAcknowledgementsControllerProvider.notifier,
      );
    }

    setUp(() {
      container = ProviderContainer();
      addTearDown(container.dispose);
    });

    test('keeps a viewed completion acknowledged across tab switches', () {
      final completion = entry(.done);

      expect(isCompletionAcknowledged(acknowledged(), completion), isFalse);
      controller().acknowledge(completion);
      expect(isCompletionAcknowledged(acknowledged(), completion), isTrue);
      expect(isCompletionAcknowledged(acknowledged(), completion), isTrue);
    });

    test('a later completion epoch requires acknowledgement again', () {
      final first = entry(.done);
      controller().acknowledge(first);
      final later = entry(
        .done,
        stateStartedAt: now.add(const Duration(minutes: 1)),
      );

      expect(isCompletionAcknowledged(acknowledged(), later), isFalse);
    });

    test('ignores statuses that are not done', () {
      controller().acknowledge(entry(.working));
      controller().acknowledge(entry(.waiting));
      controller().acknowledge(null);

      expect(acknowledged(), isEmpty);
    });

    test('retains acknowledgements until the session is actually removed', () {
      final completion = entry(.done);
      controller().acknowledge(completion);

      controller().retainTerminalSessions(<String>{'s1', 's2'});
      expect(isCompletionAcknowledged(acknowledged(), completion), isTrue);

      controller().retainTerminalSessions(<String>{'s2'});
      expect(isCompletionAcknowledged(acknowledged(), completion), isFalse);
    });
  });
}
