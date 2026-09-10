import 'dart:io';

const _legacyProtocolImport =
    'package:alera/src/features/workbench/infra/terminal_host/terminal_host_protocol.dart';
const _platformProtocolPath =
    'lib/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
const _legacyProtocolPath =
    'lib/src/features/workbench/infra/terminal_host/terminal_host_protocol.dart';

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
