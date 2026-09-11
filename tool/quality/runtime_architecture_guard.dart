import 'dart:io';

const _legacyProtocolImport =
    'package:alera/src/features/workbench/infra/terminal_host/terminal_host_protocol.dart';
const _legacyCliSidecarImport =
    'package:alera/src/features/workbench/infra/terminal_host/alera_cli_sidecar.dart';
const _platformProtocolPath =
    'lib/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
const _legacyProtocolPath =
    'lib/src/features/workbench/infra/terminal_host/terminal_host_protocol.dart';
const _sharedCliSidecarPath =
    'lib/src/shared/infra/runtime/alera_cli_sidecar.dart';
const _legacyCliSidecarPath =
    'lib/src/features/workbench/infra/terminal_host/alera_cli_sidecar.dart';
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
const _appWindowApplicationRoot = 'lib/src/features/app_window/application';
const _resourceManagerPresentationRoot =
    'lib/src/features/resource_manager/presentation';
const _diagnosticsInfraRoot = 'lib/src/features/diagnostics/infra';
const _workbenchInfraRoot = 'lib/src/features/workbench/infra';
const _workbenchApplicationRoot = 'lib/src/features/workbench/application';
const _runtimeTransportInfraRoot = 'lib/src/shared/infra/runtime';

final _dependencyDirective = RegExp(
  r'''^\s*(?:import|export|part)\s+['\"]([^'\"]+)['\"]''',
);
final _exportDirective = RegExp(r'''^\s*export\s+['\"]([^'\"]+)['\"]''');

void main() {
  final violations = <String>[];

  _checkMovedFiles(violations);
  _checkLegacyReferences(violations);
  _checkPlatformFeatureDependencies(violations);
  _checkApplicationPresentationDependencies(violations);
  _checkCrossFeatureWorkbenchInfraDependencies(violations);
  _checkRuntimeHostWorkbenchApplicationDependencies(violations);
  _checkRuntimeHostPresentationDependencies(violations);
  _checkAppWindowApplicationDependencies(violations);
  _checkResourceManagerPresentationDependencies(violations);
  _checkDiagnosticsInfraDependencies(violations);

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
  if (File(_legacyCliSidecarPath).existsSync()) {
    violations.add(
      'Runtime CLI sidecar resolver is still owned by Workbench: '
      '$_legacyCliSidecarPath',
    );
  }
  if (!File(_sharedCliSidecarPath).existsSync()) {
    violations.add(
      'Missing shared runtime CLI resolver: $_sharedCliSidecarPath',
    );
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
    _legacyCliSidecarImport,
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
      if (uri == null) {
        continue;
      }
      if (_resolvesToPresentation(file, uri)) {
        violations.add(
          'Application code imports presentation code: '
          '$path:${index + 1}:$uri',
        );
        continue;
      }
      final dependency = _resolveLocalDependency(file, uri);
      if (dependency != null &&
          _reexportsPresentation(dependency, <String>{})) {
        violations.add(
          'Application code reaches presentation code through an exported dependency: '
          '$path:${index + 1}:$uri',
        );
      }
    }
  }
}

void _checkCrossFeatureWorkbenchInfraDependencies(List<String> violations) {
  for (final file in _dartFilesUnder(_featureRoot)) {
    final path = _displayPath(file);
    if (!path.contains('/application/') ||
        path.endsWith('.g.dart') ||
        path.startsWith('lib/src/features/workbench/')) {
      continue;
    }
    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final match = _dependencyDirective.firstMatch(lines[index]);
      final uri = match?.group(1);
      if (uri == null || !_resolvesWithin(file, uri, _workbenchInfraRoot)) {
        continue;
      }
      violations.add(
        'Application code imports Workbench infrastructure across features: '
        '$path:${index + 1}:$uri',
      );
    }
  }
}

void _checkRuntimeHostWorkbenchApplicationDependencies(
  List<String> violations,
) {
  for (final file in _dartFilesUnder(_runtimeHostApplicationRoot)) {
    if (_displayPath(file).endsWith('.g.dart')) {
      continue;
    }
    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final match = _dependencyDirective.firstMatch(lines[index]);
      final uri = match?.group(1);
      if (uri == null ||
          !_resolvesWithin(file, uri, _workbenchApplicationRoot)) {
        continue;
      }
      violations.add(
        'Runtime-host application imports Workbench application code: '
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
          (!_resolvesWithin(file, uri, _runtimeTransportInfraRoot) &&
              !_resolvesWithin(file, uri, _workbenchInfraRoot))) {
        continue;
      }
      violations.add(
        'Runtime-host presentation imports transport infrastructure: '
        '${_displayPath(file)}:${index + 1}:$uri',
      );
    }
  }
}

void _checkAppWindowApplicationDependencies(List<String> violations) {
  for (final file in _dartFilesUnder(_appWindowApplicationRoot)) {
    if (_displayPath(file).endsWith('.g.dart')) {
      continue;
    }
    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final match = _dependencyDirective.firstMatch(lines[index]);
      final uri = match?.group(1);
      if (uri == null ||
          !_resolvesWithin(file, uri, _runtimeTransportInfraRoot)) {
        continue;
      }
      violations.add(
        'App-window application imports runtime transport infrastructure: '
        '${_displayPath(file)}:${index + 1}:$uri',
      );
    }
  }
}

void _checkResourceManagerPresentationDependencies(List<String> violations) {
  for (final file in _dartFilesUnder(_resourceManagerPresentationRoot)) {
    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final match = _dependencyDirective.firstMatch(lines[index]);
      final uri = match?.group(1);
      if (uri == null ||
          !_resolvesWithin(file, uri, _runtimeTransportInfraRoot)) {
        continue;
      }
      violations.add(
        'Resource-manager presentation imports runtime transport infrastructure: '
        '${_displayPath(file)}:${index + 1}:$uri',
      );
    }
  }
}

void _checkDiagnosticsInfraDependencies(List<String> violations) {
  for (final file in _dartFilesUnder(_diagnosticsInfraRoot)) {
    final lines = file.readAsLinesSync();
    for (var index = 0; index < lines.length; index += 1) {
      final match = _dependencyDirective.firstMatch(lines[index]);
      final uri = match?.group(1);
      if (uri == null || !_resolvesWithin(file, uri, _workbenchInfraRoot)) {
        continue;
      }
      violations.add(
        'Diagnostics infrastructure imports Workbench infrastructure: '
        '${_displayPath(file)}:${index + 1}:$uri',
      );
    }
  }
}

bool _resolvesToFeature(File owner, String uri) =>
    _resolvesWithin(owner, uri, _featureRoot);

bool _resolvesWithin(File owner, String uri, String rootPath) {
  final dependency = _resolveLocalDependency(owner, uri);
  return dependency != null &&
      _isWithin(dependency.absolute.uri, Directory(rootPath).absolute.uri);
}

bool _resolvesToPresentation(File owner, String uri) {
  final dependency = _resolveLocalDependency(owner, uri);
  if (dependency == null ||
      !_isWithin(
        dependency.absolute.uri,
        Directory(_featureRoot).absolute.uri,
      )) {
    return false;
  }
  return _displayPath(dependency.absolute).contains('/presentation/');
}

bool _reexportsPresentation(File file, Set<String> visited) {
  final absolute = file.absolute;
  if (!absolute.existsSync() || !visited.add(absolute.path)) {
    return false;
  }
  for (final line in absolute.readAsLinesSync()) {
    final uri = _exportDirective.firstMatch(line)?.group(1);
    if (uri == null) {
      continue;
    }
    if (_resolvesToPresentation(absolute, uri)) {
      return true;
    }
    final dependency = _resolveLocalDependency(absolute, uri);
    if (dependency != null && _reexportsPresentation(dependency, visited)) {
      return true;
    }
  }
  return false;
}

File? _resolveLocalDependency(File owner, String uri) {
  const packagePrefix = 'package:alera/';
  if (uri.startsWith(packagePrefix)) {
    return File('lib/${uri.substring(packagePrefix.length)}');
  }
  if (uri.startsWith('dart:') || uri.startsWith('package:')) {
    return null;
  }
  return File.fromUri(owner.parent.absolute.uri.resolve(uri));
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
