import 'dart:io';

import 'package:alera/src/features/shell/presentation/alera_shell_page.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  test(
    'MCP production path resolver checks actual file existence and containment',
    () async {
      final root = await Directory.systemTemp.createTemp('mcp-path-boundary');
      addTearDown(() => root.delete(recursive: true));
      await Directory(p.join(root.path, 'lib')).create();
      await File(p.join(root.path, 'lib', 'main.dart'))
          .writeAsString('void main() {}');
      final workspace = _workspace(root.path);
      expect(
        await mcpResolveExistingWorkspacePath(workspace, 'lib/main.dart'),
        'lib/main.dart',
      );
      await expectLater(
        mcpResolveExistingWorkspacePath(workspace, '../outside.dart'),
        throwsException,
      );
      await expectLater(
        mcpResolveExistingWorkspacePath(workspace, 'lib/missing.dart'),
        throwsException,
      );
      await expectLater(
        mcpResolveExistingWorkspacePath(workspace, 'C:/outside.dart'),
        throwsException,
      );
    },
  );

  test(
    'MCP production path resolver rejects a symlink escaping the Workspace',
    () async {
      final parent = await Directory.systemTemp.createTemp('mcp-path-symlink');
      addTearDown(() => parent.delete(recursive: true));
      final root = await Directory(p.join(parent.path, 'workspace')).create();
      final outside = await File(p.join(parent.path, 'outside.dart'))
          .writeAsString('secret');
      await Link(p.join(root.path, 'escape.dart')).create(outside.path);
      await expectLater(
        mcpResolveExistingWorkspacePath(_workspace(root.path), 'escape.dart'),
        throwsException,
      );
    },
    skip: Platform.isWindows,
  );
}

Workspace _workspace(String path) {
  final now = DateTime.utc(2026, 11, 4);
  return Workspace(
    id: 'workspace-boundary',
    projectId: 'project-boundary',
    name: 'Main',
    path: path,
    kind: .main,
    status: .active,
    createdAt: now,
    updatedAt: now,
  );
}
