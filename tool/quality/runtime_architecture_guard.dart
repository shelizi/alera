import 'dart:io';

const _legacyProtocolImport =
    'package:alera/src/features/workbench/infra/terminal_host/terminal_host_protocol.dart';
const _platformProtocolPath =
    'lib/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
const _legacyProtocolPath =
    'lib/src/features/workbench/infra/terminal_host/terminal_host_protocol.dart';
const _legacyTransportImports = <String>[
  'package:alera/src/features/workbench/infra/terminal_host/terminal_host_frame_codec.dart',
  'package:alera/src/features/workbench/infra/terminal_host/terminal_host_socket_isolate.dart',
];
const _platformTransportPaths = <String>[
  'lib/src/platform/runtime_host/transport/terminal_host_frame_codec.dart',
  'lib/src/platform/runtime_host/transport/terminal_host_socket_isolate.dart',
];
const _legacyTransportPaths = <String>[
  'lib/src/features/workbench/infra/terminal_host/terminal_host_frame_codec.dart',
  'lib/src/features/workbench/infra/terminal_host/terminal_host_socket_isolate.dart',
];

void main() {
  final violations = <String>[];

  if (File(_legacyProtocolPath).existsSync()) {
    violations.add(
      'Runtime protocol is still owned by Workbench: $_legacyProtocolPath',
    );
  }
  if (!File(_platformProtocolPath).existsSync()) {
    violations.add('Missing neutral runtime protocol: $_platformProtocolPath');
  }

  final grep = Process.runSync('git', <String>[
    'grep',
    '-n',
    '-F',
    _legacyProtocolImport,
    '--',
    'lib/src',
    'test',
  ]);
  if (grep.exitCode == 0) {
    final matches = '${grep.stdout}'.trim();
    if (matches.isNotEmpty) {
      violations.add('Legacy runtime protocol imports:\n$matches');
    }
  } else if (grep.exitCode != 1) {
    violations.add('Unable to scan runtime protocol imports: ${grep.stderr}');
  }

  for (final path in _legacyTransportPaths) {
    if (File(path).existsSync()) {
      violations.add('Runtime transport is still owned by Workbench: $path');
    }
  }
  for (final path in _platformTransportPaths) {
    if (!File(path).existsSync()) {
      violations.add('Missing neutral runtime transport: $path');
    }
  }
  for (final legacyImport in _legacyTransportImports) {
    final transportGrep = Process.runSync('git', <String>[
      'grep',
      '-n',
      '-F',
      legacyImport,
      '--',
      'lib/src',
      'test',
    ]);
    if (transportGrep.exitCode == 0) {
      final matches = '${transportGrep.stdout}'.trim();
      if (matches.isNotEmpty) {
        violations.add('Legacy runtime transport imports:\n$matches');
      }
    } else if (transportGrep.exitCode != 1) {
      violations.add(
        'Unable to scan runtime transport imports: ${transportGrep.stderr}',
      );
    }
  }

  final platformProtocol = File(_platformProtocolPath);
  if (platformProtocol.existsSync()) {
    final source = platformProtocol.readAsStringSync();
    final featureImport = RegExp(r"import 'package:alera/src/features/[^']+';");
    if (featureImport.hasMatch(source)) {
      violations.add(
        'Runtime protocol boundary imports feature code: $_platformProtocolPath',
      );
    }
  }

  if (violations.isEmpty) {
    stdout.writeln('Runtime architecture guard passed.');
    return;
  }

  stderr.writeln('Runtime architecture guard failed:');
  for (final violation in violations) {
    stderr.writeln(' - $violation');
  }
  exitCode = 1;
}
