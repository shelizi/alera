import 'dart:io';

import 'package:path/path.dart' as p;

/// Whether [command] resolves to an executable file under the PATH carried by
/// [environment].
///
/// Windows shells resolve bare names through PATHEXT, so `claude` matches the
/// `claude.cmd`/`claude.exe` shims installers actually write; POSIX shells skip
/// files without the execute bit, so a plain exists check is not enough. The
/// [executableExists] override keeps tests off the real filesystem.
bool commandResolvesOnPath(
  String command, {
  required Map<String, String> environment,
  bool? isWindows,
  bool Function(String path)? executableExists,
}) {
  final name = command.trim();
  // Separators make this a path, not a PATH lookup; the caller is expected to
  // hand over bare command names.
  if (name.isEmpty || name.contains('/') || name.contains(r'\')) {
    return false;
  }
  final windows = isWindows ?? Platform.isWindows;
  final exists =
      executableExists ??
      (windows ? _windowsExecutableExists : _posixExecutableExists);
  final pathValue = _pathValue(environment, windows: windows);
  if (pathValue == null || pathValue.isEmpty) {
    return false;
  }
  final join = windows ? p.windows.join : p.posix.join;
  for (final segment in pathValue.split(windows ? ';' : ':')) {
    final directory = _cleanPathSegment(segment, windows: windows);
    if (directory == null) {
      continue;
    }
    for (final candidate in _candidateNames(
      name,
      environment,
      windows: windows,
    )) {
      if (exists(join(directory, candidate))) {
        return true;
      }
    }
  }
  return false;
}

String? _pathValue(Map<String, String> environment, {required bool windows}) {
  if (!windows) {
    return environment['PATH'];
  }
  // Windows environment keys are case-insensitive; the process block usually
  // spells it 'Path'.
  for (final entry in environment.entries) {
    if (entry.key.toLowerCase() == 'path') {
      return entry.value;
    }
  }
  return null;
}

/// Windows PATH entries may be wrapped in double quotes; an empty POSIX
/// segment means "current directory", which is not meaningful here because
/// the probe has no working directory of its own.
String? _cleanPathSegment(String segment, {required bool windows}) {
  var value = segment.trim();
  if (windows &&
      value.length > 1 &&
      value.startsWith('"') &&
      value.endsWith('"')) {
    value = value.substring(1, value.length - 1).trim();
  }
  return value.isEmpty ? null : value;
}

List<String> _candidateNames(
  String command,
  Map<String, String> environment, {
  required bool windows,
}) {
  if (!windows) {
    return <String>[command];
  }
  final pathExt = environment['PATHEXT']?.trim();
  final extensions = (pathExt == null || pathExt.isEmpty)
      ? const <String>['.com', '.exe', '.bat', '.cmd']
      : pathExt
            .split(';')
            .map((ext) => ext.trim().toLowerCase())
            .where((ext) => ext.startsWith('.') && ext.length > 1)
            .toList(growable: false);
  final seen = <String>{command.toLowerCase()};
  return <String>[
    command,
    for (final ext in extensions)
      if (seen.add('$command$ext')) '$command$ext',
  ];
}

bool _windowsExecutableExists(String path) => File(path).existsSync();

bool _posixExecutableExists(String path) {
  final stat = FileStat.statSync(path);
  return stat.type == FileSystemEntityType.file && stat.mode & 0x49 != 0;
}
