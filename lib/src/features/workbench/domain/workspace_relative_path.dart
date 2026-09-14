import 'package:path/path.dart' as p;

/// Converts Windows verbatim/device-prefixed paths into ordinary paths that are
/// suitable for display or clipboard use. Internal filesystem operations may
/// still keep the original canonical path.
String userVisibleWorkspacePath(String path) {
  const extendedUncPrefix = r'\\?\UNC\';
  const extendedPrefix = r'\\?\';
  const devicePrefix = r'\\.\';
  if (path.startsWith(extendedUncPrefix)) {
    return r'\\' + path.substring(extendedUncPrefix.length);
  }
  if (path.startsWith(extendedPrefix) || path.startsWith(devicePrefix)) {
    return path.substring(4);
  }
  return path;
}

/// Returns [filePath] relative to [workspacePath] when the file lives inside
/// the workspace, or null when it does not. A file equal to the workspace
/// root resolves to '.'.
String? workspaceRelativePath({
  required String workspacePath,
  required String filePath,
  p.Context? pathContext,
}) {
  final context = pathContext ?? p.context;
  if (!context.isAbsolute(filePath)) {
    return null;
  }
  final normalizedWorkspacePath = context.normalize(workspacePath);
  final normalizedFilePath = context.normalize(filePath);
  if (!context.isWithin(normalizedWorkspacePath, normalizedFilePath) &&
      !context.equals(normalizedWorkspacePath, normalizedFilePath)) {
    return null;
  }
  return context.relative(normalizedFilePath, from: normalizedWorkspacePath);
}
