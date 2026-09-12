import 'package:alera/src/design_system/forms/alera_dropdown_field.dart';
import 'package:alera/src/features/projects/domain/project.dart';
import 'package:alera/src/features/workbench/application/workspace_graph_repository.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:alera/src/features/workbench/presentation/workspace_graph_dialogs.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'lists a legacy cross-project parent as marked and keeps it clearable',
    (tester) async {
      final alera = _project(id: 'alera', name: 'Alera');
      final orca = _project(id: 'orca', name: 'Orca');
      final child = _workspace(
        id: 'child',
        projectId: alera.id,
        name: 'Feature',
        kind: .linked,
        parentWorkspaceId: 'orca-main',
      );
      final aleraMain = _workspace(
        id: 'alera-main',
        projectId: alera.id,
        name: 'Alera',
      );
      final orcaMain = _workspace(
        id: 'orca-main',
        projectId: orca.id,
        name: 'Orca',
      );
      WorkspaceParentSelection? result;
      var dialogClosed = false;

      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: FilledButton(
                onPressed: () async {
                  result = await showWorkspaceParentDialog(
                    context: context,
                    workspace: child,
                    options: <WorkspaceParentOption>[
                      WorkspaceParentOption(
                        project: alera,
                        workspace: aleraMain,
                      ),
                      WorkspaceParentOption(project: orca, workspace: orcaMain),
                    ],
                    relations: <WorkspaceRelation>[
                      _relation('orca-main', 'child'),
                    ],
                  );
                  dialogClosed = true;
                },
                child: const Text('Open'),
              ),
            ),
          ),
        ),
      );

      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      final field = tester.widget<AleraDropdownField<String?>>(
        find.byWidgetPredicate(
          (widget) =>
              widget is AleraDropdownField<String?> &&
              widget.labelText == 'Parent Workspace',
        ),
      );

      expect(field.value, 'orca-main');
      expect(
        field.entries.map((entry) => (entry.value, entry.label)),
        <(String?, String)>[
          (null, 'No Parent'),
          ('alera-main', 'Alera / Alera - main'),
          ('orca-main', 'Orca / Orca - main (other project)'),
        ],
      );
      expect(field.entries.last.enabled, isTrue);

      field.onChanged(null);
      await tester.pump();
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();

      expect(dialogClosed, isTrue);
      expect(result, isNotNull);
      expect(result!.parentWorkspaceId, isNull);
    },
  );
}

Project _project({required String id, required String name}) {
  final now = DateTime.utc(2026, 5, 25);
  return Project(
    id: id,
    name: name,
    repoPath: '/repo/$id',
    createdAt: now,
    updatedAt: now,
  );
}

Workspace _workspace({
  required String id,
  required String projectId,
  required String name,
  WorkspaceKind kind = .main,
  String? parentWorkspaceId,
}) {
  final now = DateTime.utc(2026, 5, 25);
  return Workspace(
    id: id,
    projectId: projectId,
    name: name,
    branch: 'main',
    path: '/repo/$projectId/$id',
    createdAt: now,
    updatedAt: now,
    kind: kind,
    status: .active,
    parentWorkspaceId: parentWorkspaceId,
  );
}

WorkspaceRelation _relation(String parentId, String childId) {
  return WorkspaceRelation(
    id: '$parentId-$childId',
    parentWorkspaceId: parentId,
    parentInstanceId: 'instance-$parentId',
    childWorkspaceId: childId,
    childInstanceId: 'instance-$childId',
    createdAt: DateTime.utc(2026, 5, 25),
  );
}
