part of 'resource_snapshot_projection_test.dart';

AgentStatusEntry _titleStatus({
  AgentType agent = AgentType.codex,
  String workspaceId = 'w1',
  String tabId = 't1',
  String sessionId = 's1',
}) => AgentStatusEntry(
  terminalSessionId: sessionId,
  workspaceId: workspaceId,
  tabId: tabId,
  agentType: agent,
  state: .working,
  prompt: '',
  updatedAt: _now,
  stateStartedAt: _now,
);

void _registerResourceTitleTests() {
  ResourceSessionRow row({
    String? liveTitle,
    WorkspaceTabRecord? tab,
    AgentStatusEntry? status,
    bool running = true,
  }) => buildResourceTree(
    snapshot: _snapshot(<ResourceSessionSample>[
      _session('s1', workspaceId: 'w1', tabId: 't1', running: running),
    ]),
    projects: <Project>[_project('p1', 'Alera')],
    workspaces: <Workspace>[_workspace('w1', 'Main')],
    tabs: <WorkspaceTabRecord>[
      tab ?? _tab('t1', 'w1', sessionId: 's1', title: 'Original Terminal'),
    ],
    terminalTitles: <String, String>{'s1': ?liveTitle},
    agentStatuses: <String, AgentStatusEntry>{'s1': ?status},
  ).projects.single.workspaces.single.sessions.single;

  test('resource rows use the live agent title and current agent identity', () {
    final codex = row(liveTitle: '  Fix Login  ', status: _titleStatus());
    expect(codex.label, 'Fix Login');
    expect(codex.agentName, 'Codex');
    final claude = row(
      liveTitle: 'Review Login',
      status: _titleStatus(agent: .claude),
    );
    expect(claude.label, 'Review Login');
    expect(claude.agentName, 'Claude Code');
  });

  test('resource rows preserve manual and generated titles', () {
    for (final source in <String>['manual', 'generated']) {
      final tab = _tab('t1', 'w1', sessionId: 's1', title: 'Saved Title')
          .copyWith(
            payload: <String, Object?>{
              workspaceTabTerminalSessionIdPayloadKey: 's1',
              workspaceTabManualTitlePayloadKey: true,
              'agentTitleSource': source,
            },
          );
      expect(row(tab: tab, liveTitle: 'Shell Prompt').label, 'Saved Title');
    }
  });

  test('resource rows fall back to the stored title without a live title', () {
    expect(row().label, 'Original Terminal');
    expect(row(liveTitle: '   ').label, 'Original Terminal');
    expect(row().agentName, isNull);
  });

  test('resource rows reject stale identities and exited agents', () {
    for (final status in <AgentStatusEntry>[
      _titleStatus(workspaceId: 'old-workspace'),
      _titleStatus(tabId: 'old-tab'),
      _titleStatus(sessionId: 'old-session'),
    ]) {
      expect(row(status: status).agentName, isNull);
    }
    expect(row(status: _titleStatus(), running: false).agentName, isNull);
  });
}
