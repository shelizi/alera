import 'package:alera/src/shared/infra/files/path_identity.dart';
import 'package:path/path.dart' as p;

/// Converts Windows verbatim/device-prefixed paths into ordinary paths that are
/// suitable for display or clipboard use. Internal filesystem operations may
/// still keep the original canonical path.
String userVisibleWorkspacePath(String path, {p.Context? pathContext}) =>
    withoutWindowsPathPrefix(path, pathContext: pathContext);

/// Returns [filePath] relative to [workspacePath] when the file lives inside
/// the workspace, or null when it does not. A file equal to the workspace
/// root resolves to '.'. Windows verbatim prefixes on either side are ignored.
String? workspaceRelativePath({
  required String workspacePath,
  required String filePath,
  p.Context? pathContext,
}) => relativePathWithin(
  root: workspacePath,
  path: filePath,
  pathContext: pathContext,
);
