import 'package:alera/src/features/workbench/domain/workbench_layout.dart';

abstract interface class WorkbenchLayoutRepository {
  Future<WorkbenchLayout?> findWorkbenchLayout(String workspaceId);

  Future<WorkbenchLayout> upsertWorkbenchLayout(WorkbenchLayout layout);
}
