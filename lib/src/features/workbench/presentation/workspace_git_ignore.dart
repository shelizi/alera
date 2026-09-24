import 'dart:io';

String workspaceGitIgnorePattern(String path, {required bool isDirectory}) {
  final normalized = path
      .replaceAll('\\', '/')
      .split('/')
      .where((segment) => segment.isNotEmpty)
      .join('/');
  final escaped = normalized.replaceAllMapped(
    RegExp(r'[\\*?\[\]#! ]'),
    (match) => '\\${match.group(0)}',
  );
  return '/$escaped${isDirectory ? '/' : ''}';
}

Future<bool> addWorkspacePathToGitIgnore({
  required String rootPath,
  required String path,
  required bool isDirectory,
}) async {
  final pattern = workspaceGitIgnorePattern(path, isDirectory: isDirectory);
  final file = File('$rootPath${Platform.pathSeparator}.gitignore');
  return appendWorkspaceGitIgnorePattern(file, pattern);
}

Future<bool> appendWorkspaceGitIgnorePattern(File file, String pattern) async {
  if (!await file.exists()) {
    await file.writeAsString('$pattern\n', flush: true);
    return true;
  }

  final content = await file.readAsString();
  final alreadyPresent = content
      .split(RegExp(r'\r?\n'))
      .any((line) => line == pattern);
  if (alreadyPresent) {
    return false;
  }

  final newline = content.contains('\r\n') ? '\r\n' : '\n';
  final separator =
      content.isEmpty || content.endsWith('\n') || content.endsWith('\r')
      ? ''
      : newline;
  await file.writeAsString(
    '$separator$pattern$newline',
    mode: FileMode.append,
    flush: true,
  );
  return true;
}
