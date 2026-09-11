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
const _scanRoots = <String>['lib/src', 'test'];
const _platformRuntimeRoot = 'lib/src/platform/runtime_host';
const _featureRoot = 'lib/src/features';
const _runtimeHostApplicationRoot = 'lib/src/features/runtime_host/application';
const _runtimeHostPresentationRoot =
    'lib/src/features/runtime_host/presentation';
const _workbenchInfraImportPrefix =
    'package:alera/src/features/workbench/infra/';
const _runtimeTransportInfraImportPrefix =
    'package:alera/src/shared/infra/runtime/';

final _dependencyDirective = RegExp(
  r'''^\s*(?:import|export|part)\s+['\"]([^'\"]+)['\"]''',
);

void main() {
  final violations = <String>[];

  _checkMovedFiles(violations);
  _checkLegacyReferences(violations);
  _checkPlatformFeatureDependencies(violations);
  _checkApplicationPresentationDependencies(violations);
  _checkRuntimeHostApplicationDependencies(violations);
  _checkRuntimeHostPresentationDependencies(violations);

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

void _checkMovedFiles(List<String> violations) {
  if (File(_legacyProtocolPath).existsSync()) {
    violations.add(
      'Runtime protocol is still owned by Workbench: $_legacyProtocolPath',
    );
  }
  if (!File(_platformProtocolPath).existsSync()) {
    violations.add('Missing neutral runtime protocol: $_platformProtocolPath');
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
}

void _checkLegacyReferences(List<String> violations) {
  for (final legacyImport in <String>[
    _legacyProtocolImport,
    ..._legacyTransportImports,
  ]) {
    final matches = _findLiteralReferences(legacyImport);
    if (matches.isNotEmpty) {
      violations.add(
        'Legacy runtime import still referenced:\n${matches.join('\n')}',
      );
    }
  }
}

List<String> _findLiteralReferences(String needle) {
  final matches = <String>[];
  for (final root in _scanRoots) {
    for (final file in _dartFilesUnder(root)) {
      final lines = file.readAsLinesSync();
      for (var index = 0; index < lines.length; index += 1) {
        if (lines[index].contains(needle)) {
          matches.add(
            '${_displayPath(file)}:${index + 1}:${lines[index].trim()}',
          );
        }
      }
    }
  }
  return matches;
}

void _checkPlatformFeatureDependencies(List<String> violations) {
  final platformRoot = Directory(_platformRuntimeRoot);
  if (!platformRoot.existsSync()) {
    violations.add('Missing neutral runtime root: $_platformRuntimeRoot');
    return;
  }

  for (final file in _dartFilesUnder(_platformRuntimeRoot)) {
    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final match = _dependencyDirective.firstMatch(lines[index]);
      final uri = match?.group(1);
      if (uri != null && _resolvesToFeature(file, uri)) {
        violations.add(
          'Runtime boundary imports feature code: '
          '${_displayPath(file)}:${index + 1}:$uri',
        );
      }
    }
  }
}

void _checkApplicationPresentationDependencies(List<String> violations) {
  for (final file in _dartFilesUnder(_featureRoot)) {
    final path = _displayPath(file);
    if (!path.contains('/application/') || path.endsWith('.g.dart')) {
      continue;
    }

    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final line = lines[index];
      if (line.contains('terminalRuntimeProvider')) {
        violations.add(
          'Application code depends on the full terminal runtime provider: '
          '$path:${index + 1}:${line.trim()}',
        );
      }

      final match = _dependencyDirective.firstMatch(line);
      final uri = match?.group(1);
      if (uri == null || !uri.contains('/presentation/')) {
        continue;
      }
      violations.add(
        'Application code imports presentation code: '
        '$path:${index + 1}:$uri',
      );
    }
  }
}

void _checkRuntimeHostApplicationDependencies(List<String> violations) {
  for (final file in _dartFilesUnder(_runtimeHostApplicationRoot)) {
    if (_displayPath(file).endsWith('.g.dart')) {
      continue;
    }
    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final match = _dependencyDirective.firstMatch(lines[index]);
      final uri = match?.group(1);
      if (uri == null || !uri.startsWith(_workbenchInfraImportPrefix)) {
        continue;
      }
      violations.add(
        'Runtime-host application imports Workbench infrastructure: '
        '${_displayPath(file)}:${index + 1}:$uri',
      );
    }
  }
}

void _checkRuntimeHostPresentationDependencies(List<String> violations) {
  for (final file in _dartFilesUnder(_runtimeHostPresentationRoot)) {
    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final match = _dependencyDirective.firstMatch(lines[index]);
      final uri = match?.group(1);
      if (uri == null ||
          (!uri.startsWith(_runtimeTransportInfraImportPrefix) &&
              !uri.startsWith(_workbenchInfraImportPrefix))) {
        continue;
      }
      violations.add(
        'Runtime-host presentation imports transport infrastructure: '
        '${_displayPath(file)}:${index + 1}:$uri',
      );
    }
  }
}

bool _resolvesToFeature(File owner, String uri) {
  if (uri.startsWith('package:alera/src/features/')) {
    return true;
  }
  if (uri.startsWith('dart:') || uri.startsWith('package:')) {
    return false;
  }

  final featureRoot = Directory('lib/src/features').absolute.uri;
  final resolved = owner.parent.absolute.uri.resolve(uri);
  return _isWithin(resolved, featureRoot);
}

bool _isWithin(Uri candidate, Uri root) {
  final candidatePath = candidate.normalizePath().path;
  final normalizedRoot = root.normalizePath().path;
  final rootPath = normalizedRoot.endsWith('/')
      ? normalizedRoot
      : '$normalizedRoot/';
  return candidatePath.startsWith(rootPath);
}

Iterable<File> _dartFilesUnder(String rootPath) sync* {
  final root = Directory(rootPath);
  if (!root.existsSync()) {
    return;
  }
  for (final entity in root.listSync(recursive: true, followLinks: false)) {
    if (entity is File && entity.path.endsWith('.dart')) {
      yield entity;
    }
  }
}

String _displayPath(File file) => file.path.replaceAll('\\', '/');
