import 'dart:async';
import 'dart:io';

import 'package:alera/src/features/external_editor/domain/external_editor_launch_result.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:path/path.dart' as p;

typedef ExternalEditorCommandReader = String? Function();
typedef ExternalEditorWorkspaceModeReader =
    ExternalEditorWorkspaceMode Function();
typedef ExternalEditorPathExists = bool Function(String path);
typedef ExternalEditorPathCanonicalizer = String? Function(String path);

class ZedExternalEditorLauncher implements ExternalEditorLauncher {
  ZedExternalEditorLauncher({
    required ProcessRunner processRunner,
    ExternalEditorCommandReader? commandReader,
    ExternalEditorWorkspaceModeReader? workspaceModeReader,
    ExternalEditorPathExists? pathExists,
    ExternalEditorPathCanonicalizer? pathCanonicalizer,
  }) : _processRunner = processRunner,
       _commandReader = commandReader ?? _noConfiguredCommand,
       _workspaceModeReader = workspaceModeReader ?? _newWindowMode,
       _pathExists = pathExists ?? _pathExistsOnDisk,
       _pathCanonicalizer = pathCanonicalizer ?? _canonicalExistingPathOnDisk;

  final ProcessRunner _processRunner;
  final ExternalEditorCommandReader _commandReader;
  final ExternalEditorWorkspaceModeReader _workspaceModeReader;
  final ExternalEditorPathExists _pathExists;
  final ExternalEditorPathCanonicalizer _pathCanonicalizer;

  @override
  Future<ExternalEditorLaunchResult> openWorkspace(String workspacePath) async {
    final workspace = _validatedExistingAbsolutePath(workspacePath);
    if (workspace == null) {
      return _invalidTarget('The requested Alera workspace does not exist.');
    }

    final arguments = <String>[
      if (_workspaceModeReader() == .newWindow) '--new',
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
      return _invalidTarget('The requested Zed file target does not exist.');
    }

    final context = _pathContextFor(workspace.canonical);
    if (!context.equals(workspace.canonical, file.canonical) &&
        !context.isWithin(workspace.canonical, file.canonical)) {
      return _invalidTarget(
        'The requested Zed file target is outside the active Alera workspace.',
      );
    }
    if (request.line != null && request.line! < 1 ||
        request.column != null && request.column! < 1 ||
        request.column != null && request.line == null) {
      return _invalidTarget('Zed line and column values must be positive.');
    }

    final target = switch ((request.line, request.column)) {
      (final int line, final int column) => '${file.normalized}:$line:$column',
      (final int line, null) => '${file.normalized}:$line',
      _ => file.normalized,
    };
    return _start(<String>[target], workingDirectory: workspace.normalized);
  }

  @override
  Future<ExternalEditorLaunchResult> openFiles(
    ExternalEditorOpenFilesRequest request,
  ) async {
    final workspace = _validatedExistingAbsolutePath(request.workspacePath);
    if (workspace == null || request.filePaths.isEmpty) {
      return _invalidTarget('The requested Zed file targets are invalid.');
    }
    final context = _pathContextFor(workspace.canonical);
    final targets = <String>[];
    for (final filePath in request.filePaths) {
      final file = _validatedExistingAbsolutePath(filePath);
      if (file == null ||
          (!context.equals(workspace.canonical, file.canonical) &&
              !context.isWithin(workspace.canonical, file.canonical))) {
        return _invalidTarget(
          'A requested Zed file target is missing or outside the active Alera workspace.',
        );
      }
      if (!targets.contains(file.normalized)) {
        targets.add(file.normalized);
      }
    }
    return _start(targets, workingDirectory: workspace.normalized);
  }

  @override
  Future<ExternalEditorAvailability> checkAvailability() async {
    try {
      final output = await _processRunner.run(
        _resolvedCommand(),
        const <String>['--version'],
      );
      if (output.exitCode == 0) {
        final version = output.stdout.trim();
        return ExternalEditorAvailability(
          available: true,
          version: version.isEmpty ? null : version,
        );
      }
      return const ExternalEditorAvailability(
        available: false,
        message: 'Zed was found but its version check failed.',
      );
    } on Object {
      return const ExternalEditorAvailability(
        available: false,
        message: 'Zed could not be started. Check Settings > Editor.',
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
        'Could not start Zed. Configure the Zed executable in Settings > Editor.',
      );
    } on Object {
      return externalEditorLaunchFailure(
        .failed,
        'Could not open the requested path in Zed. Check Settings > Editor and try again.',
      );
    }
  }

  String _resolvedCommand() => _nonBlank(_commandReader()) ?? 'zed';

  ({String normalized, String canonical})? _validatedExistingAbsolutePath(
    String value,
  ) {
    final context = _pathContextFor(value);
    if (!context.isAbsolute(value)) return null;
    final normalized = context.normalize(value);
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

p.Context _pathContextFor(String value) =>
    RegExp(r'^[A-Za-z]:[\\/]').hasMatch(value)
    ? p.Context(style: .windows)
    : p.Context(style: .posix);
