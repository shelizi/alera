import 'dart:io';

import 'package:alera/src/shared/infra/files/path_identity.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';
import 'package:path/path.dart' as p;
import 'package:url_launcher/url_launcher.dart' show launchUrl;

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
  Future<bool> Function(Uri uri)? launchUri,
  Future<bool> Function(String path)? revealWindowsFile,
}) {
  this
    : _platform = platform ?? currentWorkspaceFolderPlatform(),
      _directoryExists =
          directoryExists ?? ((path) async => Directory(path).exists()),
      _entityType = entityType ?? FileSystemEntity.type,
      _launchUri = launchUri ?? ((uri) => launchUrl(uri)),
      _revealWindowsFile = revealWindowsFile ?? _revealFileInWindowsExplorer;

  final WorkspaceFolderPlatform _platform;
  final Future<bool> Function(String path) _directoryExists;
  final Future<FileSystemEntityType> Function(String path) _entityType;
  final Future<bool> Function(Uri uri) _launchUri;
  final Future<bool> Function(String path) _revealWindowsFile;

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

    if (_platform == WorkspaceFolderPlatform.windows &&
        entityType != FileSystemEntityType.directory) {
      try {
        if (await _revealWindowsFile(_windowsExplorerPath(normalized))) {
          return const WorkspaceFolderOpenResult.success();
        }
      } catch (_) {}
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

  Future<WorkspaceFolderOpenResult> openWithDefaultApplication(
    String path,
  ) async {
    final normalized = path.trim();
    if (normalized.isEmpty) {
      return const WorkspaceFolderOpenResult.failure('Path is empty.');
    }
    final entityType = await _entityType(normalized);
    if (entityType == FileSystemEntityType.notFound) {
      return const WorkspaceFolderOpenResult.failure('Path was not found.');
    }

    if (_platform == WorkspaceFolderPlatform.windows) {
      try {
        final launched = await _launchUri(
          Uri.file(_windowsExplorerPath(normalized), windows: true),
        );
        if (launched) {
          return const WorkspaceFolderOpenResult.success();
        }
      } catch (_) {}
    }

    final commands = _defaultApplicationCommandsForPlatform(normalized);
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
    return const WorkspaceFolderOpenResult.failure(
      'Could not open item with the system default application.',
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
        // Native Explorer selection is attempted before this fallback. Keep
        // opening the containing directory here in case Explorer cannot be
        // started directly.
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

  List<_WorkspaceFolderOpenCommand> _defaultApplicationCommandsForPlatform(
    String path,
  ) {
    switch (_platform) {
      case WorkspaceFolderPlatform.macos:
        return <_WorkspaceFolderOpenCommand>[
          _WorkspaceFolderOpenCommand('open', <String>[path]),
        ];
      case WorkspaceFolderPlatform.windows:
        return const <_WorkspaceFolderOpenCommand>[];
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

  String _fileManagerTargetForReveal(String path) {
    return FileSystemEntity.isDirectorySync(path)
        ? path
        : File(path).parent.path;
  }
}

Future<bool> _revealFileInWindowsExplorer(String path) async {
  await Process.start('explorer.exe', <String>[
    '/select,$path',
  ], mode: ProcessStartMode.detached);
  return true;
}

String _windowsExplorerPath(String path) => withoutWindowsPathPrefix(
  path.replaceAll('/', r'\'),
  pathContext: p.windows,
);

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
