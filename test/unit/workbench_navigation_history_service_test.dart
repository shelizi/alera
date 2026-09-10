import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workbench_navigation_history_service.dart';
import 'package:alera/src/features/workbench/application/workbench_state.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'resolves back and forward entries to live project/workspace objects',
    () {
      final service = WorkbenchNavigationHistoryService();
      final project = _project('project');
      final main = _workspace('main', project.id);
      final first = _workspace('first', project.id);
      final second = _workspace('second', project.id);
      final state = _state(project, <Workspace>[main, first, second]);

      service.record(project: project, workspace: main);
      service.record(project: project, workspace: first);
      service.record(project: project, workspace: second);

      final back = service.peekBack(state);
      expect(back, isNotNull);
      expect(back!.project, same(project));
      expect(back.workspace, same(first));
      service.commitBack(back);

      final forward = service.peekForward(state);
      expect(forward, isNotNull);
      expect(forward!.workspace, same(second));
      service.commitForward(forward);

      expect(service.canGoBack(state), isTrue);
      expect(service.canGoForward(state), isFalse);
    },
  );

  test('prunes history entries whose workspace no longer exists', () {
    final service = WorkbenchNavigationHistoryService();
    final project = _project('project');
    final main = _workspace('main', project.id);
    final removed = _workspace('removed', project.id);
    final current = _workspace('current', project.id);

    service.record(project: project, workspace: main);
    service.record(project: project, workspace: removed);
    service.record(project: project, workspace: current);

    final state = _state(project, <Workspace>[main, current]);
    final back = service.peekBack(state);

    expect(back, isNotNull);
    expect(back!.workspace, same(main));
  });

  test('prunes the current cursor when its project disappears', () {
    final service = WorkbenchNavigationHistoryService();
    final project = _project('project');
    final workspace = _workspace('workspace', project.id);
    service.record(project: project, workspace: workspace);

    const empty = WorkbenchState();
    service.prune(empty);

    expect(service.canGoBack(empty), isFalse);
    expect(service.canGoForward(empty), isFalse);
  });
}

WorkbenchState _state(Project project, List<Workspace> workspaces) {
  return WorkbenchState(
    projects: <Project>[project],
    workspacesByProject: <String, List<Workspace>>{project.id: workspaces},
  );
}

Project _project(String id) {
  final now = DateTime.utc(2026, 9, 10);
  return Project(
    id: id,
    name: id,
    repoPath: 'C:/$id',
    createdAt: now,
    updatedAt: now,
  );
}

Workspace _workspace(String id, String projectId) {
  final now = DateTime.utc(2026, 9, 10);
  return Workspace(
    id: id,
    projectId: projectId,
    name: id,
    path: 'C:/$id',
    createdAt: now,
    updatedAt: now,
    kind: WorkspaceKind.main,
    status: WorkspaceStatus.active,
  );
}
