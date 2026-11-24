part of 'agent_status_controller_test.dart';

AgentStatusEntry _snapshotEntry({
  String terminalSessionId = 'session-1',
  AgentStatusState state = AgentStatusState.working,
  required DateTime updatedAt,
}) {
  return AgentStatusEntry(
    terminalSessionId: terminalSessionId,
    workspaceId: 'workspace-1',
    tabId: 'tab-1',
    agentType: .codex,
    state: state,
    prompt: 'Run tests',
    updatedAt: updatedAt,
    stateStartedAt: updatedAt,
  );
}

void _registerAgentStatusSnapshotTests(ProviderContainer Function() container) {
  test('an identical snapshot does not replace the state', () {
    final controller = container().read(agentStatusControllerProvider.notifier);
    final at = DateTime.utc(2026, 5, 26, 2);

    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: at),
    ]);
    final first = controller.state;
    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: at),
    ]);

    // Identity, not just equality: a new map would rebuild every listener.
    expect(identical(controller.state, first), isTrue);
  });

  test('partial snapshot updates reuse equal entries and context maps', () {
    final controller = container().read(agentStatusControllerProvider.notifier);
    final at = DateTime.utc(2026, 5, 26, 2);
    AgentStatusEntry entry(String id) =>
        _snapshotEntry(terminalSessionId: id, updatedAt: at).copyWith(
          contextOverview: {
            'goal': 'Run tests',
            'timeline': [
              {'kind': 'goal', 'text': 'Run tests', 'at': at.toIso8601String()},
            ],
          },
        );
    controller.replaceRuntimeSnapshot([entry('session-1'), entry('session-2')]);
    final old = container().read(agentStatusControllerProvider);
    final updated = entry('session-2')
        .copyWith(state: AgentStatusState.waiting);
    controller.replaceRuntimeSnapshot([entry('session-1'), updated]);
    final next = container().read(agentStatusControllerProvider);
    expect(next, isNot(same(old)));
    expect(next['session-1'], same(old['session-1']));
    expect(
      next['session-1']!.contextOverview,
      same(old['session-1']!.contextOverview),
    );
    expect(next['session-2'], same(updated));
    expect(old['session-2']!.state, AgentStatusState.working);
  });

  test('a stale snapshot does not resurrect a terminal that exited', () {
    // The host keeps a working presence for the life of the PTY when a
    // terminating hook never arrives, which is what left runs spinning
    // forever under orchestration load. The harness clock starts at 01:00,
    // so this snapshot predates the local resolution.
    final controller = container().read(agentStatusControllerProvider.notifier);
    final at = DateTime.utc(2026, 5, 26, 0, 30);
    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: at),
    ]);

    controller.clearExitedTerminal(workspaceId: 'workspace-1', tabId: 'tab-1');
    expect(controller.state, isEmpty);

    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: at),
    ]);

    expect(controller.state, isEmpty);
  });

  test('a genuinely newer snapshot wins over the local resolution', () {
    final controller = container().read(agentStatusControllerProvider.notifier);
    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: .utc(2026, 5, 26, 0, 30)),
    ]);
    controller.clearExitedTerminal(workspaceId: 'workspace-1', tabId: 'tab-1');

    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: .utc(2026, 5, 26, 9)),
    ]);

    expect(controller.state['session-1']!.state, AgentStatusState.working);
  });

  test('a snapshot does not un-clear a locally cleared terminal', () {
    final controller = container().read(agentStatusControllerProvider.notifier);
    final at = DateTime.utc(2026, 5, 26, 0, 30);
    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: at),
    ]);

    controller.clearTerminal('session-1');
    expect(controller.state, isEmpty);

    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: at),
    ]);

    expect(controller.state, isEmpty);
  });

  test('a clear is forgotten once the host stops reporting the session', () {
    final controller = container().read(agentStatusControllerProvider.notifier);
    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: .utc(2026, 5, 26, 0, 30)),
    ]);
    controller.clearTerminal('session-1');

    controller.replaceRuntimeSnapshot(const <AgentStatusEntry>[]);
    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: .utc(2026, 5, 26, 0, 30)),
    ]);

    expect(controller.state['session-1'], isNotNull);
  });

  test(
    'runtime delta updates one background session without dropping others',
    () {
      final controller = container().read(
        agentStatusControllerProvider.notifier,
      );
      final initialAt = DateTime.utc(2026, 5, 26, 0, 30);
      controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
        _snapshotEntry(updatedAt: initialAt),
        _snapshotEntry(terminalSessionId: 'session-2', updatedAt: initialAt),
      ]);

      controller.mergeRuntimeDelta(
        upserts: <AgentStatusEntry>[
          _snapshotEntry(
            state: AgentStatusState.waiting,
            updatedAt: DateTime.utc(2026, 5, 26, 0, 31),
          ),
        ],
      );

      expect(controller.state['session-1']!.state, AgentStatusState.waiting);
      expect(controller.state['session-2'], isNotNull);

      controller.mergeRuntimeDelta(
        removedSessionIds: const <String>['session-1'],
      );

      expect(controller.state['session-1'], isNull);
      expect(controller.state['session-2'], isNotNull);
    },
  );

  test('unrelated sessions in the snapshot are kept', () {
    final controller = container().read(agentStatusControllerProvider.notifier);
    final at = DateTime.utc(2026, 5, 26, 0, 30);

    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: at),
      _snapshotEntry(terminalSessionId: 'session-2', updatedAt: at),
    ]);
    controller.clearTerminal('session-1');
    controller.replaceRuntimeSnapshot(<AgentStatusEntry>[
      _snapshotEntry(updatedAt: at),
      _snapshotEntry(terminalSessionId: 'session-2', updatedAt: at),
    ]);

    expect(controller.state.keys, <String>['session-2']);
  });
}
