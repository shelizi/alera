import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/resource_manager/application/resource_manager_providers.dart';
import 'package:alera/src/features/resource_manager/domain/resource_snapshot.dart';
import 'package:alera/src/features/resource_manager/domain/resource_tree.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

class _Workbench(final WorkbenchState initial) extends WorkbenchController {
  @override
  WorkbenchState build() => initial;
}

class _TitleNotifier extends ValueNotifier<String> {
  _TitleNotifier() : super('Original Terminal');

  bool get observed => hasListeners;
}

class _TitleSession extends TerminalSessionHandle {
  @override
  final _TitleNotifier titleListenable = _TitleNotifier();
  @override
  String get displayTitle => titleListenable.value;
  @override
  String get tabId => 't1';
  @override
  String get workspaceId => 'w1';
  @override
  String get terminalSessionId => 's1';
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
  @override
  void dispose() {
    titleListenable.dispose();
    super.dispose();
  }
}

class _Runtime(final _TitleSession session) implements TerminalRuntime {
  @override
  TerminalSessionHandle? peekSession(String tabId) =>
      tabId == 't1' ? session : null;
  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

void main() {
  test('open resource tree follows titles and agent identity and releases listeners', () async {
    final now = DateTime.utc(2026, 10, 11);
    final session = _TitleSession();
    addTearDown(session.dispose);
    final snapshot = ResourceSnapshot.fromJson(<String, Object?>{
      'warming': false,
      'sessions': <Object?>[
        <String, Object?>{
          'sessionId': 's1',
          'workspaceId': 'w1',
          'tabId': 't1',
          'running': true,
        },
      ],
    });
    final state = WorkbenchState(
      projects: <Project>[
        Project(
          id: 'p1',
          name: 'Alera',
          repoPath: '/repo',
          createdAt: now,
          updatedAt: now,
        ),
      ],
      workspacesByProject: <String, List<Workspace>>{
        'p1': <Workspace>[
          Workspace(
            id: 'w1',
            projectId: 'p1',
            name: 'Main',
            path: '/repo',
            createdAt: now,
            updatedAt: now,
            kind: .linked,
            status: .active,
            hostId: 'local',
          ),
        ],
      },
      tabsByWorkspace: <String, List<WorkspaceTabRecord>>{
        'w1': <WorkspaceTabRecord>[
          WorkspaceTabRecord(
            id: 't1',
            workspaceId: 'w1',
            title: 'Original Terminal',
            createdAt: now,
            updatedAt: now,
            payload: const <String, Object?>{
              workspaceTabTerminalSessionIdPayloadKey: 's1',
            },
          ),
        ],
      },
    );
    final container = ProviderContainer(
      overrides: [
        resourceSnapshotProvider.overrideWith((ref) async => snapshot),
        workbenchControllerProvider.overrideWith(() => _Workbench(state)),
        terminalRuntimeProvider.overrideWith((ref) => _Runtime(session)),
      ],
    );
    addTearDown(container.dispose);
    await container.read(resourceSnapshotProvider.future);
    container.read(resourcePanelOpenProvider.notifier).set(open: true);
    final subscription = container.listen(
      resourceTreeProvider(.name),
      (_, _) {},
    );
    addTearDown(subscription.close);
    await container.pump();
    ResourceSessionRow row() => container
        .read(resourceTreeProvider(.name))
        .projects
        .single
        .workspaces
        .single
        .sessions
        .single;
    expect(row().label, 'Original Terminal');
    expect(session.titleListenable.observed, isTrue);
    session.titleListenable.value = 'Fix Login';
    await container.pump();
    expect(row().label, 'Fix Login');
    final status = AgentStatusEntry(
      terminalSessionId: 's1',
      workspaceId: 'w1',
      tabId: 't1',
      agentType: .codex,
      state: .working,
      prompt: '',
      updatedAt: now,
      stateStartedAt: now,
    );
    final statuses = container.read(agentStatusControllerProvider.notifier);
    statuses.mergeRuntimeDelta(upserts: <AgentStatusEntry>[status]);
    await container.pump();
    expect(row().agentName, 'Codex');
    final before = container.read(resourceTreeProvider(.name));
    statuses.mergeRuntimeDelta(
      upserts: <AgentStatusEntry>[status.copyWith(toolName: 'Read')],
    );
    await container.pump();
    expect(
      identical(container.read(resourceTreeProvider(.name)), before),
      isTrue,
    );
    statuses.mergeRuntimeDelta(
      upserts: <AgentStatusEntry>[status.copyWith(agentType: .claude)],
    );
    await container.pump();
    expect(row().agentName, 'Claude Code');
    container.read(resourcePanelOpenProvider.notifier).set(open: false);
    await container.pump();
    expect(session.titleListenable.observed, isFalse);
    session.titleListenable.value = 'Hidden Title';
    await container.pump();
    expect(row().label, 'Fix Login');
    container.read(resourcePanelOpenProvider.notifier).set(open: true);
    await container.pump();
    expect(row().label, 'Hidden Title');
    subscription.close();
    await container.pump();
    expect(session.titleListenable.observed, isFalse);
  });
}
