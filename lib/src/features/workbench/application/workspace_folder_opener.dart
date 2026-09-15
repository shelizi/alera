import 'dart:io';

import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:path/path.dart' as p;

enum WorkspaceFolderPlatform { macos, windows, linux, other }

class const WorkspaceFolderOpenResult._({
  required final bool ok,
  final String? message,
}) {
  const new success() : this._(ok: true);

  const new failure(String message) : this._(ok: false, message: message);
}

class WorkspaceFolderOpener({
  required final ProcessRunner processRunner,
  WorkspaceFolderPlatform? platform,
  Future<bool> Function(String path)? directoryExists,
  Future<FileSystemEntityType> Function(String path)? entityType,
}) {
  this
    : _platform = platform ?? currentWorkspaceFolderPlatform(),
      _directoryExists =
          directoryExists ?? ((path) async => Directory(path).exists()),
      _entityType = entityType ?? FileSystemEntity.type;

  final WorkspaceFolderPlatform _platform;
  final Future<bool> Function(String path) _directoryExists;
  final Future<FileSystemEntityType> Function(String path) _entityType;

  String get fileManagerLabel {
    switch (_platform) {
      case WorkspaceFolderPlatform.macos:
        return 'Finder';
      case WorkspaceFolderPlatform.windows:
        return 'Explorer';
      case WorkspaceFolderPlatform.linux:
      case WorkspaceFolderPlatform.other:
        return 'File Manager';
    }
  }

  Future<WorkspaceFolderOpenResult> open(String path) async {
    final normalized = path.trim();
    if (normalized.isEmpty) {
      return const WorkspaceFolderOpenResult.failure(
        'Workspace path is empty.',
      );
    }
    if (!await _directoryExists(normalized)) {
      return const WorkspaceFolderOpenResult.failure(
        'Workspace folder was not found.',
      );
    }

    final commands = _commandsForPlatform(normalized);
    for (final command in commands) {
      try {
        final result = await processRunner.run(
          command.executable,
          command.arguments,
        );
        if (result.exitCode == 0) {
          return const WorkspaceFolderOpenResult.success();
        }
      } catch (_) {
        continue;
      }
    }
    return WorkspaceFolderOpenResult.failure(
      'Could not open workspace folder in $fileManagerLabel.',
    );
  }

  Future<WorkspaceFolderOpenResult> reveal(String path) async {
    final normalized = path.trim();
    if (normalized.isEmpty) {
      return const WorkspaceFolderOpenResult.failure('Path is empty.');
    }
    final entityType = await _entityType(normalized);
    if (entityType == FileSystemEntityType.notFound) {
      return const WorkspaceFolderOpenResult.failure('Path was not found.');
    }

    final commands = entityType == FileSystemEntityType.directory
        ? _commandsForPlatform(normalized)
        : _revealCommandsForPlatform(normalized);
    for (final command in commands) {
      try {
        final result = await processRunner.run(
          command.executable,
          command.arguments,
        );
        if (result.exitCode == 0) {
          return const WorkspaceFolderOpenResult.success();
        }
      } catch (_) {
        continue;
      }
    }
    return WorkspaceFolderOpenResult.failure(
      'Could not reveal item in $fileManagerLabel.',
    );
  }

  List<_WorkspaceFolderOpenCommand> _commandsForPlatform(String path) {
    switch (_platform) {
      case WorkspaceFolderPlatform.macos:
        return <_WorkspaceFolderOpenCommand>[
          _WorkspaceFolderOpenCommand('open', <String>[path]),
        ];
      case WorkspaceFolderPlatform.windows:
        return <_WorkspaceFolderOpenCommand>[
          _WorkspaceFolderOpenCommand('explorer.exe', <String>[
            _windowsExplorerPath(path),
          ]),
        ];
      case WorkspaceFolderPlatform.linux:
        return <_WorkspaceFolderOpenCommand>[
          _WorkspaceFolderOpenCommand('xdg-open', <String>[path]),
          _WorkspaceFolderOpenCommand('gio', <String>['open', path]),
        ];
      case WorkspaceFolderPlatform.other:
        return <_WorkspaceFolderOpenCommand>[
          _WorkspaceFolderOpenCommand('xdg-open', <String>[path]),
        ];
    }
  }

  List<_WorkspaceFolderOpenCommand> _revealCommandsForPlatform(String path) {
    switch (_platform) {
      case WorkspaceFolderPlatform.macos:
        return <_WorkspaceFolderOpenCommand>[
          _WorkspaceFolderOpenCommand('open', <String>['-R', path]),
        ];
      case WorkspaceFolderPlatform.windows:
        // Alera's ProcessRunner reaches Windows commands through cmd.exe. When
        // Explorer's `/select,<path>` switch is quoted as one shell argument,
        // Explorer can return success while ignoring the target. Opening the
        // containing directory uses the same reliable path as folder actions.
        return <_WorkspaceFolderOpenCommand>[
          _WorkspaceFolderOpenCommand('explorer.exe', <String>[
            _windowsExplorerParentPath(path),
          ]),
        ];
      case WorkspaceFolderPlatform.linux:
        final target = _fileManagerTargetForReveal(path);
        return <_WorkspaceFolderOpenCommand>[
          _showItemsCommand(path),
          _WorkspaceFolderOpenCommand('xdg-open', <String>[target]),
          _WorkspaceFolderOpenCommand('gio', <String>['open', target]),
        ];
      case WorkspaceFolderPlatform.other:
        final target = _fileManagerTargetForReveal(path);
        return <_WorkspaceFolderOpenCommand>[
          _WorkspaceFolderOpenCommand('xdg-open', <String>[target]),
        ];
    }
  }

  String _fileManagerTargetForReveal(String path) {
    return FileSystemEntity.isDirectorySync(path)
        ? path
        : File(path).parent.path;
  }
}

String _windowsExplorerPath(String path) {
  final normalized = path.replaceAll('/', r'\');
  const extendedUncPrefix = r'\\?\UNC\';
  const extendedPrefix = r'\\?\';
  if (normalized.startsWith(extendedUncPrefix)) {
    return r'\\' + normalized.substring(extendedUncPrefix.length);
  }
  if (normalized.startsWith(extendedPrefix)) {
    final unprefixed = normalized.substring(extendedPrefix.length);
    if (RegExp(r'^[A-Za-z]:\\').hasMatch(unprefixed)) {
      return unprefixed;
    }
  }
  return normalized;
}

String _windowsExplorerParentPath(String path) {
  return p.windows.dirname(_windowsExplorerPath(path));
}

/// FreeDesktop FileManager1.ShowItems selects the path in the session file
/// manager (Nautilus, Dolphin, ...). The GVariant array is one argv token so
/// ProcessRunner shell-quoting leaves it intact for `gdbus`.
_WorkspaceFolderOpenCommand _showItemsCommand(String path) {
  final uri = Uri.file(path).toString();
  return _WorkspaceFolderOpenCommand('gdbus', <String>[
    'call',
    '--session',
    '--dest',
    'org.freedesktop.FileManager1',
    '--object-path',
    '/org/freedesktop/FileManager1',
    '--method',
    'org.freedesktop.FileManager1.ShowItems',
    "['$uri']",
    '',
  ]);
}

WorkspaceFolderPlatform currentWorkspaceFolderPlatform() {
  return workspaceFolderPlatformForOperatingSystem(Platform.operatingSystem);
}

WorkspaceFolderPlatform workspaceFolderPlatformForOperatingSystem(
  String operatingSystem,
) {
  return switch (operatingSystem) {
    'macos' => WorkspaceFolderPlatform.macos,
    'windows' => WorkspaceFolderPlatform.windows,
    'linux' => WorkspaceFolderPlatform.linux,
    _ => WorkspaceFolderPlatform.other,
  };
}

class const _WorkspaceFolderOpenCommand(
  final String executable,
  final List<String> arguments,
);
