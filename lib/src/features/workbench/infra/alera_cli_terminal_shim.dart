import 'dart:io';

import 'package:alera/src/shared/infra/runtime/alera_cli_sidecar.dart';
import 'package:alera/src/shared/infra/files/posix_file_mode.dart';
import 'package:path/path.dart' as p;
import 'package:alera/src/shared/infra/storage/alera_support_directory.dart';

typedef ApplicationSupportDirectoryResolver = Future<Directory> Function();

class AleraCliTerminalShimService({
  AleraCliResolver? cliResolver,
  ApplicationSupportDirectoryResolver? applicationSupportDirectory,
  String? operatingSystem,
}) {
  this
    : _cliResolver = cliResolver ?? DefaultAleraCliResolver(),
      _applicationSupportDirectory =
          applicationSupportDirectory ?? aleraApplicationSupportDirectory,
      _operatingSystem = operatingSystem ?? Platform.operatingSystem;

  final AleraCliResolver _cliResolver;
  final ApplicationSupportDirectoryResolver _applicationSupportDirectory;
  final String _operatingSystem;

  Future<Map<String, String>> prepareForTerminalLaunch() async {
    final support = await _applicationSupportDirectory();
    final runtimeDir = Directory(p.join(support.path, 'terminal_host'));
    if (!await runtimeDir.exists()) {
      await runtimeDir.create(recursive: true);
    }
    final shimDir = Directory(p.join(support.path, 'terminal_tools', 'bin'));
    if (!await shimDir.exists()) {
      await shimDir.create(recursive: true);
    }
    final command = await _cliResolver.resolve(runtimeDir: runtimeDir.path);
    if (_operatingSystem == 'windows') {
      await _writeWindowsShim(shimDir, runtimeDir.path, command);
    } else {
      await _writePosixShim(shimDir, runtimeDir.path, command);
    }
    return <String, String>{
      'ALERA_RUNTIME_DIR': runtimeDir.path,
      'ALERA_AGENT_WRAPPER_PATH': shimDir.path,
    };
  }

  Future<void> _writePosixShim(
    Directory shimDir,
    String runtimeDir,
    AleraCliCommand command,
  ) async {
    final file = File(p.join(shimDir.path, 'alera'));
    final lines = <String>[
      '#!/bin/sh',
      'export ALERA_RUNTIME_DIR=${_shQuote(runtimeDir)}',
      if (command.workingDirectory != null)
        'cd ${_shQuote(command.workingDirectory!)} || exit \$?',
      'exec ${_shQuote(command.executable)} ${command.prefixArguments.map(_shQuote).join(' ')} "\$@"',
      '',
    ];
    await _writeShimIfChanged(file, lines.join('\n'));
    setPosixFileMode(file.path, posixExecutableFileMode);
  }

  Future<void> _writeWindowsShim(
    Directory shimDir,
    String runtimeDir,
    AleraCliCommand command,
  ) async {
    final content = <String>[
      '@echo off',
      'set "ALERA_RUNTIME_DIR=${_cmdQuoteValue(runtimeDir)}"',
      if (command.workingDirectory != null)
        'cd /d "${_cmdQuoteValue(command.workingDirectory!)}" || exit /b %ERRORLEVEL%',
      '"${_cmdQuoteValue(command.executable)}" ${command.prefixArguments.map((arg) => '"${_cmdQuoteValue(arg)}"').join(' ')} %*',
      'exit /b %ERRORLEVEL%',
      '',
    ].join('\r\n');
    await _writeShimIfChanged(File(p.join(shimDir.path, 'alera.cmd')), content);
    await _writeShimIfChanged(File(p.join(shimDir.path, 'alera.bat')), content);
  }

  // A terminal launch may call this for every new tab. Avoid rewriting an
  // identical script: Windows antivirus and file watchers can add latency.
  Future<void> _writeShimIfChanged(File file, String content) async {
    try {
      if (await file.exists() && await file.readAsString() == content) {
        return;
      }
    } on FileSystemException {
      // Restore the launcher using the original write path below.
    }
    await file.writeAsString(content, flush: true);
  }
}

String _shQuote(String value) {
  return "'${value.replaceAll("'", "'\"'\"'")}'";
}

String _cmdQuoteValue(String value) {
  return value.replaceAll('"', r'\"');
}
