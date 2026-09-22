import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:crypto/crypto.dart';
import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

import '../../../shared/infra/process/command_path_probe.dart';
import '../application/language_intelligence_activity.dart';
import '../application/managed_language_server_installer.dart';
import '../domain/language_provider_descriptor.dart';
import 'managed_language_server_catalog.dart';

typedef ManagedLanguageServerEnvironmentReader = Map<String, String> Function();
typedef ManagedLanguageServerExecutableExists = bool Function(String path);
typedef ManagedLanguageServerSupportDirectory = Future<Directory> Function();

final class ManagedLanguageServerProcessRequest {
  const ManagedLanguageServerProcessRequest({
    required this.executable,
    required this.arguments,
    required this.environment,
    this.workingDirectory,
    this.runInShell = false,
  });

  final String executable;
  final List<String> arguments;
  final Map<String, String> environment;
  final String? workingDirectory;
  final bool runInShell;
}

abstract interface class ManagedLanguageServerProcessRunner {
  Future<ProcessResult> run(ManagedLanguageServerProcessRequest request);
}

final class DefaultManagedLanguageServerProcessRunner
    implements ManagedLanguageServerProcessRunner {
  const DefaultManagedLanguageServerProcessRunner();

  @override
  Future<ProcessResult> run(ManagedLanguageServerProcessRequest request) =>
      Process.run(
        request.executable,
        request.arguments,
        workingDirectory: request.workingDirectory,
        environment: request.environment,
        runInShell: request.runInShell,
      );
}

final class ManagedLanguageServerInstallException implements Exception {
  const ManagedLanguageServerInstallException(this.message);

  final String message;

  @override
  String toString() => 'ManagedLanguageServerInstallException: $message';
}

final class _ManagedPrerequisiteProbe {
  const _ManagedPrerequisiteProbe.ready(this.executable) : reason = null;

  const _ManagedPrerequisiteProbe.unavailable(this.reason) : executable = null;

  final String? executable;
  final String? reason;

  bool get isReady => executable != null;
}

final class ManagedLanguageServerInstaller
    implements ManagedLanguageServerInstallerPort {
  ManagedLanguageServerInstaller({
    ManagedLanguageServerEnvironmentReader? environmentReader,
    bool? isWindows,
    ManagedLanguageServerExecutableExists? executableExists,
    ManagedLanguageServerSupportDirectory? supportDirectory,
    ManagedLanguageServerProcessRunner? processRunner,
    Map<String, ManagedLanguageServerRecipe>? recipes,
    LanguageIntelligenceActivityReporter? activityReporter,
  }) : _environmentReader = environmentReader ?? _platformEnvironment,
       _isWindows = isWindows ?? Platform.isWindows,
       _executableExists = executableExists,
       _supportDirectory =
           supportDirectory ?? (() => getApplicationSupportDirectory()),
       _processRunner =
           processRunner ?? const DefaultManagedLanguageServerProcessRunner(),
       _recipes = recipes ?? managedLanguageServerRecipes,
       _activityReporter = activityReporter;

  final ManagedLanguageServerEnvironmentReader _environmentReader;
  final bool _isWindows;
  final ManagedLanguageServerExecutableExists? _executableExists;
  final ManagedLanguageServerSupportDirectory _supportDirectory;
  final ManagedLanguageServerProcessRunner _processRunner;
  final Map<String, ManagedLanguageServerRecipe> _recipes;
  final LanguageIntelligenceActivityReporter? _activityReporter;
  final Map<String, Future<ManagedLanguageServerInstallResult>> _inFlight =
      <String, Future<ManagedLanguageServerInstallResult>>{};

  @override
  Future<ManagedLanguageServerInstallResult> ensureInstalled(
    LanguageProviderDescriptor provider,
  ) {
    final recipe = _recipes[provider.id];
    if (recipe == null) {
      _report(
        providerId: provider.id,
        state: ManagedLanguageServerAcquisitionState.missing,
        detail:
            'Alera has no managed download recipe for ${provider.id}; '
            'configure an executable or install it on PATH.',
      );
      return Future<ManagedLanguageServerInstallResult>.value(
        ManagedLanguageServerUnavailable(
          reason:
              'Alera has no managed download recipe for ${provider.id}; '
              'configure an executable or install it on PATH.',
        ),
      );
    }
    final existing = _inFlight[recipe.providerId];
    if (existing != null) return existing;

    final install = _ensureRecipeWithReporting(recipe);
    _inFlight[recipe.providerId] = install;
    return install.whenComplete(() {
      if (identical(_inFlight[recipe.providerId], install)) {
        _inFlight.remove(recipe.providerId);
      }
    });
  }

  Future<ManagedLanguageServerInstallResult> _ensureRecipeWithReporting(
    ManagedLanguageServerRecipe recipe,
  ) async {
    _report(
      providerId: recipe.providerId,
      state: ManagedLanguageServerAcquisitionState.checking,
      version: recipe.version,
      detail: 'Checking managed language server ${recipe.version}.',
    );
    try {
      return await _ensureRecipe(recipe);
    } catch (error) {
      _report(
        providerId: recipe.providerId,
        state: ManagedLanguageServerAcquisitionState.failed,
        version: recipe.version,
        detail: error.toString(),
      );
      rethrow;
    }
  }

  Future<ManagedLanguageServerInstallResult> _ensureRecipe(
    ManagedLanguageServerRecipe recipe,
  ) async {
    final support = await _supportDirectory();
    final managedRoot = Directory(p.join(support.path, 'language-servers'));
    await managedRoot.create(recursive: true);
    final providerRoot = Directory(
      p.join(managedRoot.path, _safeSegment(recipe.providerId)),
    );
    await providerRoot.create(recursive: true);

    final lockFile = File(p.join(providerRoot.path, '.install.lock'));
    final lock = await lockFile.open(mode: FileMode.append);
    await lock.lock(FileLock.exclusive);
    try {
      final installDirectory = Directory(
        p.join(providerRoot.path, _safeSegment(recipe.version)),
      );
      final executable = _managedExecutablePath(installDirectory.path, recipe);
      if (await _isValidInstall(recipe, executable)) {
        _report(
          providerId: recipe.providerId,
          state: ManagedLanguageServerAcquisitionState.ready,
          version: recipe.version,
          executable: executable,
          detail: 'Managed language server is installed and verified.',
        );
        return ManagedLanguageServerInstalled(
          executable: executable,
          version: recipe.version,
        );
      }

      final environment = Map<String, String>.of(_environmentReader());
      final prerequisiteProbe = await _probePrerequisite(recipe, environment);
      if (!prerequisiteProbe.isReady) {
        _report(
          providerId: recipe.providerId,
          state: ManagedLanguageServerAcquisitionState.missing,
          version: recipe.version,
          detail: prerequisiteProbe.reason,
        );
        return ManagedLanguageServerUnavailable(
          reason: prerequisiteProbe.reason!,
        );
      }
      final prerequisite = prerequisiteProbe.executable!;

      if (await installDirectory.exists()) {
        await installDirectory.delete(recursive: true);
      }
      await installDirectory.create(recursive: true);

      _report(
        providerId: recipe.providerId,
        state: ManagedLanguageServerAcquisitionState.installing,
        version: recipe.version,
        detail: 'Downloading / installing from ${recipe.source}.',
      );

      try {
        await _installRecipe(
          recipe,
          prerequisite: prerequisite,
          installDirectory: installDirectory,
          managedRoot: managedRoot,
          environment: environment,
        );
        if (!_pathExists(executable)) {
          throw ManagedLanguageServerInstallException(
            'Managed install for ${recipe.providerId} completed without '
            'creating $executable.',
          );
        }
        _report(
          providerId: recipe.providerId,
          state: ManagedLanguageServerAcquisitionState.verifying,
          version: recipe.version,
          detail: 'Verifying package integrity and executable checksum.',
        );
        if (recipe.kind == ManagedLanguageServerInstallKind.npm) {
          await _validateNpmLockIntegrity(installDirectory, recipe);
        }

        final executableSha256 = await _sha256File(executable);
        await _markerFile(installDirectory.path).writeAsString(
          jsonEncode(<String, Object?>{
            'schemaVersion': 1,
            'providerId': recipe.providerId,
            'version': recipe.version,
            'source': recipe.source,
            'integrityPolicy': recipe.integrityPolicy,
            'executableRelativePath': p.relative(
              executable,
              from: installDirectory.path,
            ),
            'executableSha256': executableSha256,
          }),
          flush: true,
        );
        _report(
          providerId: recipe.providerId,
          state: ManagedLanguageServerAcquisitionState.ready,
          version: recipe.version,
          executable: executable,
          detail: 'Managed language server is installed and verified.',
        );
        return ManagedLanguageServerInstalled(
          executable: executable,
          version: recipe.version,
        );
      } on Object {
        if (await installDirectory.exists()) {
          await installDirectory.delete(recursive: true);
        }
        rethrow;
      }
    } finally {
      await lock.unlock();
      await lock.close();
    }
  }

  void _report({
    required String providerId,
    required ManagedLanguageServerAcquisitionState state,
    String? version,
    String? executable,
    String? detail,
  }) {
    _activityReporter?.reportManagedServerAcquisition(
      ManagedLanguageServerAcquisitionSnapshot(
        providerId: providerId,
        state: state,
        version: version,
        executable: executable,
        detail: detail,
      ),
    );
  }

  Future<bool> _isValidInstall(
    ManagedLanguageServerRecipe recipe,
    String executable,
  ) async {
    if (!_pathExists(executable)) return false;
    final marker = _markerFile(
      _installDirectoryForExecutable(recipe, executable),
    );
    if (!await marker.exists()) return false;
    try {
      final decoded = jsonDecode(await marker.readAsString());
      if (decoded is! Map<String, dynamic> ||
          decoded['providerId'] != recipe.providerId ||
          decoded['version'] != recipe.version) {
        return false;
      }
      final expected = decoded['executableSha256'];
      return expected is String && expected == await _sha256File(executable);
    } on Object {
      return false;
    }
  }

  String _installDirectoryForExecutable(
    ManagedLanguageServerRecipe recipe,
    String executable,
  ) {
    final relative = _managedExecutableRelativePath(recipe);
    var directory = executable;
    for (var i = 0; i < p.split(relative).length; i += 1) {
      directory = p.dirname(directory);
    }
    return directory;
  }

  File _markerFile(String installDirectory) =>
      File(p.join(installDirectory, '.alera-managed-language-server.json'));

  Future<_ManagedPrerequisiteProbe> _probePrerequisite(
    ManagedLanguageServerRecipe recipe,
    Map<String, String> environment,
  ) async {
    switch (recipe.kind) {
      case ManagedLanguageServerInstallKind.npm:
        final node = _resolveEnvironmentCommand('node', environment);
        final npm = _resolveEnvironmentCommand('npm', environment);
        if (node == null || npm == null) {
          final missing = <String>[
            if (node == null) 'node',
            if (npm == null) 'npm',
          ].join(' and ');
          return _ManagedPrerequisiteProbe.unavailable(
            '$missing ${node == null && npm == null ? 'were' : 'was'} not '
            'found on PATH. ${_npmSetupGuidance(recipe)}',
          );
        }
        final nodeResult = await _runEnvironmentProbe(
          executable: node,
          arguments: const <String>['--version'],
          environment: environment,
        );
        if (!_probeSucceeded(nodeResult)) {
          return _ManagedPrerequisiteProbe.unavailable(
            'Node.js was found but `node --version` could not run. '
            '${_npmSetupGuidance(recipe)}',
          );
        }
        final npmResult = await _runEnvironmentProbe(
          executable: npm,
          arguments: const <String>['--version'],
          environment: environment,
        );
        if (!_probeSucceeded(npmResult)) {
          return _ManagedPrerequisiteProbe.unavailable(
            'npm was found but `npm --version` could not run. '
            '${_npmSetupGuidance(recipe)}',
          );
        }
        return _ManagedPrerequisiteProbe.ready(npm);
      case ManagedLanguageServerInstallKind.go:
        final go = _resolveEnvironmentCommand('go', environment);
        if (go == null) {
          return _ManagedPrerequisiteProbe.unavailable(
            _goSetupGuidance(recipe),
          );
        }
        final result = await _runEnvironmentProbe(
          executable: go,
          arguments: const <String>['version'],
          environment: environment,
        );
        if (!_probeSucceeded(result)) {
          return _ManagedPrerequisiteProbe.unavailable(
            'Go was found but `go version` could not run. '
            '${_goSetupGuidance(recipe)}',
          );
        }
        return _ManagedPrerequisiteProbe.ready(go);
      case ManagedLanguageServerInstallKind.dotnetTool:
        final dotnet = _resolveEnvironmentCommand('dotnet', environment);
        if (dotnet == null) {
          return _ManagedPrerequisiteProbe.unavailable(
            _dotnetSetupGuidance(recipe),
          );
        }
        final result = await _runEnvironmentProbe(
          executable: dotnet,
          arguments: const <String>['--list-sdks'],
          environment: environment,
        );
        if (!_probeSucceeded(result) ||
            result!.stdout.toString().trim().isEmpty) {
          return _ManagedPrerequisiteProbe.unavailable(
            'dotnet was found, but no usable .NET SDK was detected. '
            '${_dotnetSetupGuidance(recipe)}',
          );
        }
        return _ManagedPrerequisiteProbe.ready(dotnet);
      case ManagedLanguageServerInstallKind.rustupComponent:
        final rustup = _resolveEnvironmentCommand('rustup', environment);
        if (rustup == null) {
          return _ManagedPrerequisiteProbe.unavailable(
            _rustupSetupGuidance(recipe),
          );
        }
        final result = await _runEnvironmentProbe(
          executable: rustup,
          arguments: const <String>['--version'],
          environment: environment,
        );
        if (!_probeSucceeded(result)) {
          return _ManagedPrerequisiteProbe.unavailable(
            'rustup was found but `rustup --version` could not run. '
            '${_rustupSetupGuidance(recipe)}',
          );
        }
        return _ManagedPrerequisiteProbe.ready(rustup);
    }
  }

  String? _resolveEnvironmentCommand(
    String candidate,
    Map<String, String> environment,
  ) => resolveCommandOnPath(
    candidate,
    environment: environment,
    isWindows: _isWindows,
    executableExists: _executableExists,
  );

  Future<ProcessResult?> _runEnvironmentProbe({
    required String executable,
    required List<String> arguments,
    required Map<String, String> environment,
  }) async {
    try {
      return await _processRunner
          .run(
            ManagedLanguageServerProcessRequest(
              executable: executable,
              arguments: arguments,
              environment: environment,
              runInShell: _requiresShell(executable),
            ),
          )
          .timeout(const Duration(seconds: 5));
    } on Object {
      return null;
    }
  }

  static bool _probeSucceeded(ProcessResult? result) =>
      result != null && result.exitCode == 0;

  String _npmSetupGuidance(ManagedLanguageServerRecipe recipe) =>
      'Alera can install ${recipe.executableName} automatically after Node.js '
      'and npm are available. Install Node.js, make sure `node --version` and '
      '`npm --version` work on PATH, then choose Check Again.';

  String _goSetupGuidance(ManagedLanguageServerRecipe recipe) =>
      'Alera can install ${recipe.executableName} automatically after the Go '
      'toolchain is available. Install Go, make sure `go version` works on '
      'PATH, then choose Check Again.';

  String _dotnetSetupGuidance(ManagedLanguageServerRecipe recipe) =>
      'Alera can install ${recipe.executableName} automatically after a .NET '
      'SDK is available. Install the .NET SDK (runtime-only is not enough), '
      'make sure `dotnet --list-sdks` lists an SDK, then choose Check Again.';

  String _rustupSetupGuidance(ManagedLanguageServerRecipe recipe) =>
      'Alera can install ${recipe.executableName} automatically after rustup '
      'is available. Install rustup, make sure `rustup --version` works on '
      'PATH, then choose Check Again.';

  Future<void> _installRecipe(
    ManagedLanguageServerRecipe recipe, {
    required String prerequisite,
    required Directory installDirectory,
    required Directory managedRoot,
    required Map<String, String> environment,
  }) async {
    switch (recipe.kind) {
      case ManagedLanguageServerInstallKind.npm:
        await _installNpm(
          recipe,
          prerequisite: prerequisite,
          installDirectory: installDirectory,
          environment: environment,
        );
      case ManagedLanguageServerInstallKind.go:
        await _installGo(
          recipe,
          prerequisite: prerequisite,
          installDirectory: installDirectory,
          environment: environment,
        );
      case ManagedLanguageServerInstallKind.dotnetTool:
        await _installDotnetTool(
          recipe,
          prerequisite: prerequisite,
          installDirectory: installDirectory,
          environment: environment,
        );
      case ManagedLanguageServerInstallKind.rustupComponent:
        await _installRustupComponent(
          recipe,
          prerequisite: prerequisite,
          installDirectory: installDirectory,
          managedRoot: managedRoot,
          environment: environment,
        );
    }
  }

  Future<void> _installNpm(
    ManagedLanguageServerRecipe recipe, {
    required String prerequisite,
    required Directory installDirectory,
    required Map<String, String> environment,
  }) async {
    final packageSpecs = recipe.packages
        .map((package) => '${package.name}@${package.version}')
        .toList(growable: false);
    final installEnvironment = <String, String>{
      ...environment,
      'npm_config_ignore_scripts': 'true',
      'npm_config_audit': 'false',
      'npm_config_fund': 'false',
      'npm_config_save_exact': 'true',
      'npm_config_package_lock': 'true',
    };
    await _runChecked(
      recipe,
      ManagedLanguageServerProcessRequest(
        executable: prerequisite,
        arguments: <String>[
          'install',
          '--ignore-scripts',
          '--no-audit',
          '--no-fund',
          '--save-exact',
          '--prefix',
          installDirectory.path,
          ...packageSpecs,
        ],
        environment: installEnvironment,
        runInShell: _requiresShell(prerequisite),
      ),
    );
  }

  Future<void> _installGo(
    ManagedLanguageServerRecipe recipe, {
    required String prerequisite,
    required Directory installDirectory,
    required Map<String, String> environment,
  }) async {
    final module = recipe.goModule;
    if (module == null) {
      throw ManagedLanguageServerInstallException(
        '${recipe.providerId} is missing its Go module recipe.',
      );
    }
    await _runChecked(
      recipe,
      ManagedLanguageServerProcessRequest(
        executable: prerequisite,
        arguments: <String>['install', '$module@${recipe.version}'],
        environment: <String, String>{
          ...environment,
          'GOBIN': installDirectory.path,
          'GOSUMDB': 'sum.golang.org',
        },
        runInShell: _requiresShell(prerequisite),
      ),
    );
  }

  Future<void> _installDotnetTool(
    ManagedLanguageServerRecipe recipe, {
    required String prerequisite,
    required Directory installDirectory,
    required Map<String, String> environment,
  }) async {
    final package = recipe.dotnetPackage;
    if (package == null) {
      throw ManagedLanguageServerInstallException(
        '${recipe.providerId} is missing its dotnet-tool package recipe.',
      );
    }
    await _runChecked(
      recipe,
      ManagedLanguageServerProcessRequest(
        executable: prerequisite,
        arguments: <String>[
          'tool',
          'install',
          package,
          '--tool-path',
          installDirectory.path,
          '--version',
          recipe.version,
          '--no-cache',
        ],
        environment: environment,
        runInShell: _requiresShell(prerequisite),
      ),
    );
  }

  Future<void> _installRustupComponent(
    ManagedLanguageServerRecipe recipe, {
    required String prerequisite,
    required Directory installDirectory,
    required Directory managedRoot,
    required Map<String, String> environment,
  }) async {
    final toolchain = recipe.rustupToolchain;
    final component = recipe.rustupComponent;
    if (toolchain == null || component == null) {
      throw ManagedLanguageServerInstallException(
        '${recipe.providerId} is missing its rustup component recipe.',
      );
    }
    final runtimeRoot = p.join(managedRoot.path, '.runtime', 'rust');
    final rustEnvironment = <String, String>{
      ...environment,
      'RUSTUP_HOME': p.join(runtimeRoot, 'rustup'),
      'CARGO_HOME': p.join(runtimeRoot, 'cargo'),
    };
    await Directory(rustEnvironment['RUSTUP_HOME']!).create(recursive: true);
    await Directory(rustEnvironment['CARGO_HOME']!).create(recursive: true);
    await _runChecked(
      recipe,
      ManagedLanguageServerProcessRequest(
        executable: prerequisite,
        arguments: <String>[
          'toolchain',
          'install',
          toolchain,
          '--profile',
          'minimal',
          '--component',
          component,
        ],
        environment: rustEnvironment,
        runInShell: _requiresShell(prerequisite),
      ),
    );
    final which = await _runChecked(
      recipe,
      ManagedLanguageServerProcessRequest(
        executable: prerequisite,
        arguments: <String>['which', '--toolchain', toolchain, component],
        environment: rustEnvironment,
        runInShell: _requiresShell(prerequisite),
      ),
    );
    final source = which.stdout.toString().trim();
    if (source.isEmpty || !File(source).existsSync()) {
      throw ManagedLanguageServerInstallException(
        'rustup did not return a usable $component executable for $toolchain.',
      );
    }
    final destination = _managedExecutablePath(installDirectory.path, recipe);
    await File(source).copy(destination);
    if (!_isWindows) {
      await _runChecked(
        recipe,
        ManagedLanguageServerProcessRequest(
          executable: 'chmod',
          arguments: <String>['u+x', destination],
          environment: environment,
        ),
      );
    }
  }

  Future<ProcessResult> _runChecked(
    ManagedLanguageServerRecipe recipe,
    ManagedLanguageServerProcessRequest request,
  ) async {
    final result = await _processRunner.run(request);
    if (result.exitCode == 0) return result;
    final stderr = result.stderr.toString().trim();
    final stdout = result.stdout.toString().trim();
    final detail = stderr.isNotEmpty ? stderr : stdout;
    throw ManagedLanguageServerInstallException(
      'Failed to install ${recipe.providerId} ${recipe.version} '
      '(exit ${result.exitCode})${detail.isEmpty ? '' : ': ${_bounded(detail)}'}',
    );
  }

  Future<void> _validateNpmLockIntegrity(
    Directory installDirectory,
    ManagedLanguageServerRecipe recipe,
  ) async {
    final lockFile = File(p.join(installDirectory.path, 'package-lock.json'));
    if (!await lockFile.exists()) {
      throw ManagedLanguageServerInstallException(
        'npm did not create package-lock.json for ${recipe.providerId}.',
      );
    }
    final decoded = jsonDecode(await lockFile.readAsString());
    if (decoded is! Map<String, dynamic> ||
        decoded['packages'] is! Map<String, dynamic>) {
      throw ManagedLanguageServerInstallException(
        'npm package-lock.json for ${recipe.providerId} is invalid.',
      );
    }
    final packages = decoded['packages'] as Map<String, dynamic>;
    for (final package in recipe.packages) {
      final entry = packages['node_modules/${package.name}'];
      if (entry is! Map<String, dynamic> ||
          entry['version'] != package.version ||
          entry['integrity'] is! String ||
          (entry['integrity'] as String).trim().isEmpty) {
        throw ManagedLanguageServerInstallException(
          'npm integrity metadata did not match '
          '${package.name}@${package.version}.',
        );
      }
    }
  }

  bool _pathExists(String path) => executablePathExists(
    path,
    isWindows: _isWindows,
    executableExists: _executableExists,
  );

  String _managedExecutablePath(
    String installDirectory,
    ManagedLanguageServerRecipe recipe,
  ) => p.join(installDirectory, _managedExecutableRelativePath(recipe));

  String _managedExecutableRelativePath(ManagedLanguageServerRecipe recipe) {
    if (recipe.kind == ManagedLanguageServerInstallKind.npm) {
      return p.join(
        'node_modules',
        '.bin',
        _isWindows ? '${recipe.executableName}.cmd' : recipe.executableName,
      );
    }
    return _isWindows ? '${recipe.executableName}.exe' : recipe.executableName;
  }

  bool _requiresShell(String executable) {
    if (!_isWindows) return false;
    final lower = executable.toLowerCase();
    return lower.endsWith('.cmd') || lower.endsWith('.bat');
  }
}

String _safeSegment(String value) =>
    value.replaceAll(RegExp(r'[^A-Za-z0-9._+-]'), '_');

String _bounded(String value) =>
    value.length <= 1200 ? value : '${value.substring(0, 1200)}...';

Future<String> _sha256File(String path) async =>
    (await sha256.bind(File(path).openRead()).first).toString();

Map<String, String> _platformEnvironment() => Platform.environment;
