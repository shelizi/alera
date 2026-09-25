import 'package:path/path.dart' as p;

// The single owner of how Alera spells, compares and relativizes filesystem
// paths across sources. The Rust runtime used to store canonical Windows roots
// in the verbatim form `\\?\E:\...` (from `std::fs::canonicalize`), while file
// URIs, pickers, git, language servers and the OS file manager report the
// plain, often lowercase, `e:\...` form. package:path treats those as
// unrelated roots, and `cmd.exe` refuses a verbatim working directory and runs
// in `C:\Windows` instead. Every comparison between a stored root and a path
// from another source, and every hand-off of a stored root to another process,
// goes through this file; `dart_path_identity_conformance_test.dart` rejects
// prefix handling anywhere else.
//
// On POSIX a leading `\\?\` is ordinary filename text, so nothing here rewrites
// a path unless the context (the host's by default) is Windows-style.

const String _extendedUncPrefix = r'\\?\UNC\';
const String _extendedPrefix = r'\\?\';
const String _devicePrefix = r'\\.\';
final RegExp _driveRoot = RegExp(r'^[A-Za-z]:[\\/]');

/// Returns the plain drive or UNC spelling of a Windows verbatim (`\\?\X:\`,
/// `\\?\UNC\`) or device (`\\.\X:\`) path. A prefixed path with no plain
/// equivalent, such as `\\?\Volume{...}\`, is returned unchanged.
String withoutWindowsPathPrefix(String path, {p.Context? pathContext}) {
  final context = pathContext ?? p.context;
  if (context.style != p.Style.windows) {
    return path;
  }
  if (path.startsWith(_extendedUncPrefix)) {
    return r'\\' + path.substring(_extendedUncPrefix.length);
  }
  for (final prefix in const <String>[_extendedPrefix, _devicePrefix]) {
    if (path.startsWith(prefix)) {
      final unprefixed = path.substring(prefix.length);
      return _driveRoot.hasMatch(unprefixed) ? unprefixed : path;
    }
  }
  return path;
}

/// Whether [path] still carries a Windows verbatim or device prefix after
/// [withoutWindowsPathPrefix], i.e. it has no plain spelling.
bool hasUnresolvedWindowsPathPrefix(String path, {p.Context? pathContext}) {
  final context = pathContext ?? p.context;
  if (context.style != p.Style.windows) {
    return false;
  }
  final plain = withoutWindowsPathPrefix(path, pathContext: context);
  return plain.startsWith(_extendedPrefix) || plain.startsWith(_devicePrefix);
}

/// Returns [path] normalized so it compares equal to other spellings of the
/// same location. Use it for map keys and equality; hand
/// [withoutWindowsPathPrefix] to other processes instead, since normalizing
/// can change what a user typed.
String comparablePath(String path, {p.Context? pathContext}) {
  final context = pathContext ?? p.context;
  return context.normalize(
    withoutWindowsPathPrefix(path, pathContext: context),
  );
}

/// Whether [left] and [right] name the same location, ignoring Windows prefix
/// spelling (and, on Windows, ASCII case, as package:path does).
bool isSamePath(String left, String right, {p.Context? pathContext}) {
  final context = pathContext ?? p.context;
  return context.equals(
    comparablePath(left, pathContext: context),
    comparablePath(right, pathContext: context),
  );
}

/// Whether [path] is [root] itself or lives inside it.
bool isPathWithinOrSame(String root, String path, {p.Context? pathContext}) {
  final context = pathContext ?? p.context;
  final comparableRoot = comparablePath(root, pathContext: context);
  final comparable = comparablePath(path, pathContext: context);
  return context.equals(comparableRoot, comparable) ||
      context.isWithin(comparableRoot, comparable);
}

/// Returns [path] relative to [root] when it is [root] itself (`.`) or lives
/// inside it, or null otherwise. Relative inputs are rejected rather than
/// resolved against the process working directory.
String? relativePathWithin({
  required String root,
  required String path,
  p.Context? pathContext,
}) {
  final context = pathContext ?? p.context;
  if (!context.isAbsolute(path)) {
    return null;
  }
  final comparableRoot = comparablePath(root, pathContext: context);
  final comparable = comparablePath(path, pathContext: context);
  if (!context.equals(comparableRoot, comparable) &&
      !context.isWithin(comparableRoot, comparable)) {
    return null;
  }
  return context.relative(comparable, from: comparableRoot);
}
