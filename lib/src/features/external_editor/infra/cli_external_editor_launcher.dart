import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:alera/src/shared/infra/files/path_identity.dart';
import 'package:alera/src/shared/infra/process/command_path_probe.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:path/path.dart' as p;

typedef ExternalEditorCommandReader = String? Function();
typedef ExternalEditorWorkspaceModeReader =
    ExternalEditorWorkspaceMode Function();
typedef ExternalEditorPathExists = bool Function(String path);
typedef ExternalEditorPathCanonicalizer = String? Function(String path);
typedef ExternalEditorEnvironmentReader = Map<String, String> Function();

/// Launches CLI-based editors described by an [ExternalEditorSpec]. All path
/// validation and workspace-containment checks live here; the spec only
/// contributes command names, fallback install paths, and argument shapes, so
/// new editors plug in without touching this file.
class CliExternalEditorLauncher implements ExternalEditorLauncher {
  CliExternalEditorLauncher({
    required this._spec,
    required this._processRunner,
    ExternalEditorCommandReader? commandReader,
    ExternalEditorWorkspaceModeReader? workspaceModeReader,
    ExternalEditorPathExists? pathExists,
    ExternalEditorPathCanonicalizer? pathCanonicalizer,
    ExternalEditorEnvironmentReader? environmentReader,
    this._isWindows,
  }) : _commandReader = commandReader ?? _noConfiguredCommand,
       _workspaceModeReader = workspaceModeReader ?? _newWindowMode,
       _pathExists = pathExists ?? _pathExistsOnDisk,
       _pathCanonicalizer = pathCanonicalizer ?? _canonicalExistingPathOnDisk,
       _environmentReader = environmentReader ?? _processEnvironment;

  final ExternalEditorSpec _spec;
  final ProcessRunner _processRunner;
  final ExternalEditorCommandReader _commandReader;
  final ExternalEditorWorkspaceModeReader _workspaceModeReader;
  final ExternalEditorPathExists _pathExists;
  final ExternalEditorPathCanonicalizer _pathCanonicalizer;
  final ExternalEditorEnvironmentReader _environmentReader;
  final bool? _isWindows;

  @override
  Future<ExternalEditorLaunchResult> openWorkspace(String workspacePath) async {
    final workspace = _validatedExistingAbsolutePath(workspacePath);
    if (workspace == null) {
      return _invalidTarget('The requested Alera workspace does not exist.');
    }

    final newWindow =
        _spec.supportsWorkspaceWindowMode &&
        _workspaceModeReader() == .newWindow;
    final arguments = <String>[
      ..._spec.workspaceArgs(newWindow: newWindow),
      workspace.normalized,
    ];
    return _start(arguments, workingDirectory: workspace.normalized);
  }

  @override
  Future<ExternalEditorLaunchResult> openFile(
    ExternalEditorOpenRequest request,
  ) async {
    final workspace = _validatedExistingAbsolutePath(request.workspacePath);
    final file = _validatedExistingAbsolutePath(request.filePath);
    if (workspace == null || file == null) {
      return _invalidTarget(
        'The requested ${_spec.shortName} file target does not exist.',
      );
    }

    final context = _pathContextFor(workspace.canonical);
    if (!context.equals(workspace.canonical, file.canonical) &&
        !context.isWithin(workspace.canonical, file.canonical)) {
      return _invalidTarget(
        'The requested ${_spec.shortName} file target is outside the active '
        'Alera workspace.',
      );
    }
    if (request.line != null && request.line! < 1 ||
        request.column != null && request.column! < 1 ||
        request.column != null && request.line == null) {
      return _invalidTarget(
        '${_spec.shortName} line and column values must be positive.',
      );
    }

    return _start(
      _spec.fileArgs(
        file.normalized,
        line: request.line,
        column: request.column,
      ),
      workingDirectory: workspace.normalized,
    );
  }

  @override
  Future<ExternalEditorLaunchResult> openFiles(
    ExternalEditorOpenFilesRequest request,
  ) async {
    final workspace = _validatedExistingAbsolutePath(request.workspacePath);
    if (workspace == null || request.filePaths.isEmpty) {
      return _invalidTarget(
        'The requested ${_spec.shortName} file targets are invalid.',
      );
    }
    final context = _pathContextFor(workspace.canonical);
    final targets = <String>[];
    for (final filePath in request.filePaths) {
      final file = _validatedExistingAbsolutePath(filePath);
      if (file == null ||
          (!context.equals(workspace.canonical, file.canonical) &&
              !context.isWithin(workspace.canonical, file.canonical))) {
        return _invalidTarget(
          'A requested ${_spec.shortName} file target is missing or outside '
          'the active Alera workspace.',
        );
      }
      if (!targets.contains(file.normalized)) {
        targets.add(file.normalized);
      }
    }
    return _start(
      _spec.filesArgs(targets),
      workingDirectory: workspace.normalized,
    );
  }

  @override
  Future<bool> isInstalled() async {
    final override = _nonBlank(_commandReader());
    if (override != null) {
      return _pathExists(override);
    }
    return _resolveExecutable() != null;
  }

  @override
  Future<ExternalEditorAvailability> checkAvailability() async {
    final command = _nonBlank(_commandReader()) ?? _resolveExecutable();
    if (command == null) {
      return ExternalEditorAvailability(
        available: false,
        message: '${_spec.displayName} was not found. Check Settings > Editor.',
      );
    }
    try {
      final output = await _processRunner.run(command, _spec.versionArgs);
      if (output.exitCode == 0) {
        return ExternalEditorAvailability(
          available: true,
          version: _spec.parseVersion(output.stdout),
        );
      }
      return ExternalEditorAvailability(
        available: false,
        message: '${_spec.displayName} was found but its version check failed.',
      );
    } on Object {
      return ExternalEditorAvailability(
        available: false,
        message:
            '${_spec.displayName} could not be started. Check Settings > Editor.',
      );
    }
  }

  Future<ExternalEditorLaunchResult> _start(
    List<String> arguments, {
    required String workingDirectory,
  }) async {
    try {
      final process = await _processRunner.start(
        _resolvedCommand(),
        arguments,
        workingDirectory: workingDirectory,
      );
      process.stdout.listen((_) {}, onError: (Object _) {});
      process.stderr.listen((_) {}, onError: (Object _) {});
      unawaited(process.exitCode.catchError((Object _) => -1));
      return ExternalEditorLaunchResultFactories.opened;
    } on ProcessException {
      return externalEditorLaunchFailure(
        .unavailable,
        'Could not start ${_spec.displayName}. Configure the executable in '
        'Settings > Editor.',
      );
    } on Object {
      return externalEditorLaunchFailure(
        .failed,
        'Could not open the requested path in ${_spec.displayName}. Check '
        'Settings > Editor and try again.',
      );
    }
  }

  /// The configured override when set, otherwise the first resolvable
  /// candidate, otherwise the first candidate as a last-ditch spawn target.
  String _resolvedCommand() =>
      _nonBlank(_commandReader()) ??
      _resolveExecutable() ??
      _spec.commandCandidates.first;

  /// Resolves the executable without consulting the configured override:
  /// PATH candidates first (PATHEXT-aware on Windows), then fallback install
  /// paths with environment-variable expansion.
  String? _resolveExecutable() {
    final environment = _environmentReader();
    for (final candidate in _spec.commandCandidates) {
      if (commandResolvesOnPath(
        candidate,
        environment: environment,
        isWindows: _isWindows,
        executableExists: _pathExists,
      )) {
        return candidate;
      }
    }
    for (final path in _spec.fallbackPaths) {
      final expanded = _expandEnvironmentVars(path, environment);
      if (_pathExists(expanded)) {
        return expanded;
      }
    }
    return null;
  }

  ({String normalized, String canonical})? _validatedExistingAbsolutePath(
    String value,
  ) {
    final externalPath = _externalProcessPath(value);
    if (externalPath == null) return null;
    final context = _pathContextFor(externalPath);
    if (!context.isAbsolute(externalPath)) return null;
    final normalized = context.normalize(externalPath);
    if (!_pathExists(normalized)) return null;
    final canonical = _pathCanonicalizer(normalized);
    if (canonical == null) return null;
    final canonicalContext = _pathContextFor(canonical);
    if (!canonicalContext.isAbsolute(canonical)) return null;
    return (
      normalized: normalized,
      canonical: canonicalContext.normalize(canonical),
    );
  }

  ExternalEditorLaunchResult _invalidTarget(String message) =>
      externalEditorLaunchFailure(.invalidTarget, message);
}

String? _noConfiguredCommand() => null;

Map<String, String> _processEnvironment() => Platform.environment;

bool _pathExistsOnDisk(String path) =>
    FileSystemEntity.typeSync(path) != FileSystemEntityType.notFound;

String? _canonicalExistingPathOnDisk(String path) {
  try {
    if (Directory(path).existsSync()) {
      return Directory(path).resolveSymbolicLinksSync();
    }
    if (File(path).existsSync()) {
      return File(path).resolveSymbolicLinksSync();
    }
    if (Link(path).existsSync()) {
      return Link(path).resolveSymbolicLinksSync();
    }
  } on FileSystemException {
    return null;
  }
  return null;
}

ExternalEditorWorkspaceMode _newWindowMode() => .newWindow;

String? _nonBlank(String? value) {
  final trimmed = value?.trim();
  return trimmed == null || trimmed.isEmpty ? null : trimmed;
}

/// Expands `%VAR%` (Windows style) and `$VAR`/`${VAR}` tokens against
/// [environment]. Variable names match case-insensitively so `%localappdata%`
/// resolves under a `LocalAppData` key.
String _expandEnvironmentVars(String value, Map<String, String> environment) {
  return value.replaceAllMapped(
    RegExp(r'%([A-Za-z_][A-Za-z0-9_]*)%|\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?'),
    (match) {
      final name = (match.group(1) ?? match.group(2))!.toLowerCase();
      for (final entry in environment.entries) {
        if (entry.key.toLowerCase() == name) {
          return entry.value;
        }
      }
      return match.group(0)!;
    },
  );
}

// Editors are launched with a Windows-style path whenever the value is one,
// whatever the host, so prefixes follow Windows rules. A prefixed path with no
// plain spelling cannot be handed to an editor CLI and is rejected.
String? _externalProcessPath(String value) {
  final plain = withoutWindowsPathPrefix(value, pathContext: p.windows);
  return hasUnresolvedWindowsPathPrefix(plain, pathContext: p.windows)
      ? null
      : plain;
}

p.Context _pathContextFor(String value) =>
    RegExp(r'^[A-Za-z]:[\\/]').hasMatch(value) || value.startsWith(r'\\')
    ? p.Context(style: .windows)
    : p.Context(style: .posix);
