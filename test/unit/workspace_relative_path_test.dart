import 'package:alera/src/features/workbench/domain/workspace_relative_path.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  final posix = p.Context(style: p.Style.posix);
  final windows = p.Context(style: p.Style.windows);

  test('strips Windows verbatim prefixes from user-visible paths', () {
    expect(userVisibleWorkspacePath(r'\\?\C:\repo\alera'), r'C:\repo\alera');
    expect(
      userVisibleWorkspacePath(r'\\?\UNC\server\share\repo'),
      r'\\server\share\repo',
    );
    expect(userVisibleWorkspacePath(r'\\.\C:\repo\alera'), r'C:\repo\alera');
    expect(userVisibleWorkspacePath(r'C:\repo\alera'), r'C:\repo\alera');
    expect(userVisibleWorkspacePath('/repo/alera'), '/repo/alera');
  });

  test('returns null for a non-absolute file path', () {
    expect(
      workspaceRelativePath(
        workspacePath: '/tmp/project',
        filePath: 'packages/app/main.dart',
        pathContext: posix,
      ),
      isNull,
    );
  });

  test('relativizes a file inside the workspace', () {
    expect(
      workspaceRelativePath(
        workspacePath: '/tmp/project',
        filePath: '/tmp/project/packages/app/main.dart',
        pathContext: posix,
      ),
      'packages/app/main.dart',
    );
  });

  test('resolves the workspace root itself to a dot', () {
    expect(
      workspaceRelativePath(
        workspacePath: '/tmp/project',
        filePath: '/tmp/project',
        pathContext: posix,
      ),
      '.',
    );
  });

  test('normalizes redundant segments before comparing', () {
    expect(
      workspaceRelativePath(
        workspacePath: '/tmp/project/',
        filePath: '/tmp/project/packages/../README.md',
        pathContext: posix,
      ),
      'README.md',
    );
  });

  test('keeps files outside the workspace absolute', () {
    expect(
      workspaceRelativePath(
        workspacePath: '/tmp/project',
        filePath: '/tmp/other/main.dart',
        pathContext: posix,
      ),
      isNull,
    );
    expect(
      workspaceRelativePath(
        workspacePath: '/tmp/project',
        filePath: '/tmp/project-other/main.dart',
        pathContext: posix,
      ),
      isNull,
    );
  });

  test('relativizes with Windows separators and case-insensitive roots', () {
    expect(
      workspaceRelativePath(
        workspacePath: r'C:\project',
        filePath: r'c:\project\packages\app\main.dart',
        pathContext: windows,
      ),
      r'packages\app\main.dart',
    );
    expect(
      workspaceRelativePath(
        workspacePath: r'C:\project',
        filePath: r'D:\other\main.dart',
        pathContext: windows,
      ),
      isNull,
    );
  });
}
