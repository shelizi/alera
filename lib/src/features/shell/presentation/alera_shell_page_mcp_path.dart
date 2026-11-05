part of 'alera_shell_page.dart';

/// Production validation for built-in and explicit external MCP file opens.
/// Real disk and symlink resolution stays async; the tests replace the provider
/// rather than weakening this security boundary.
Future<String> mcpResolveExistingWorkspacePath(
  Workspace workspace,
  Object? rawPath,
) async {
  final candidate = rawPath is String
      ? normalizeWorkspaceRelativePath(rawPath)
      : null;
  if (candidate == null ||
      candidate.startsWith('/') ||
      candidate.contains(':')) {
    throw const _McpUiFailure('invalid_path');
  }
  final file = File(
    p.joinAll(<String>[workspace.path, ...candidate.split('/')]),
  );
  if (!await file.exists()) {
    throw const _McpUiFailure('file_missing');
  }
  final root = await Directory(workspace.path).resolveSymbolicLinks();
  final resolved = await file.resolveSymbolicLinks();
  if (!p.isWithin(root, resolved)) {
    throw const _McpUiFailure('invalid_path');
  }
  return candidate;
}
