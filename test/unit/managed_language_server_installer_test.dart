import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/language_intelligence/application/managed_language_server_installer.dart';
import 'package:alera/src/features/language_intelligence/domain/language_capability.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/infra/managed_language_server_catalog.dart';
import 'package:alera/src/features/language_intelligence/infra/managed_language_server_installer.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  late Directory temp;
  late LanguageProviderDescriptor provider;

  setUp(() async {
    temp = await Directory.systemTemp.createTemp('alera-managed-lsp-test-');
    provider = LanguageProviderDescriptor(
      id: 'python.pyright',
      kind: LanguageProviderKind.semanticServer,
      languages: <LanguageId>{LanguageId('python')},
      capabilities: const <LanguageCapability>{LanguageCapability.definition},
      processScope: LanguageProviderProcessScope.workspace,
      launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
      executableResolutionPolicy:
          LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
      executableCandidates: const <String>['pyright-langserver'],
    );
  });

  tearDown(() async {
    if (await temp.exists()) await temp.delete(recursive: true);
  });

  test(
    'installs pinned npm recipe and reuses SHA-256 verified cache',
    () async {
      final runner = _FakeNpmInstallRunner();
      final installer = _installer(temp, runner);

      final first = await installer.ensureInstalled(provider);
      expect(first, isA<ManagedLanguageServerInstalled>());
      final installed = first as ManagedLanguageServerInstalled;
      expect(installed.version, '1.1.414');
      expect(await File(installed.executable).exists(), isTrue);
      expect(runner.installCalls, 1);
      expect(runner.probeCalls, 2);

      final marker = File(
        p.join(
          temp.path,
          'language-servers',
          'python.pyright',
          '1.1.414',
          '.alera-managed-language-server.json',
        ),
      );
      expect(await marker.exists(), isTrue);
      final markerJson = jsonDecode(await marker.readAsString());
      expect(markerJson['providerId'], 'python.pyright');
      expect(markerJson['version'], '1.1.414');
      expect(markerJson['executableSha256'], isNotEmpty);

      final second = await installer.ensureInstalled(provider);
      expect(second, isA<ManagedLanguageServerInstalled>());
      expect(runner.installCalls, 1);
      expect(runner.probeCalls, 2);
    },
  );

  test('tampered managed executable is reinstalled', () async {
    final runner = _FakeNpmInstallRunner();
    final installer = _installer(temp, runner);
    final first = await installer.ensureInstalled(
      provider,
    ) as ManagedLanguageServerInstalled;
    expect(runner.installCalls, 1);

    await File(first.executable).writeAsString('tampered', flush: true);
    await installer.ensureInstalled(provider);
    expect(runner.installCalls, 2);
    expect(runner.probeCalls, 4);
  });

  test('concurrent requests share one install operation', () async {
    final gate = Completer<void>();
    final runner = _FakeNpmInstallRunner(gate: gate);
    final installer = _installer(temp, runner);

    final first = installer.ensureInstalled(provider);
    final second = installer.ensureInstalled(provider);
    await runner.started.future;
    expect(runner.installCalls, 1);

    gate.complete();
    final results = await Future.wait(
      <Future<ManagedLanguageServerInstallResult>>[first, second],
    );
    expect(results, everyElement(isA<ManagedLanguageServerInstalled>()));
    expect(runner.installCalls, 1);
    expect(runner.probeCalls, 2);
  });

  test(
    'missing prerequisite reports unavailable without running installer',
    () async {
      final runner = _FakeNpmInstallRunner();
      final installer = ManagedLanguageServerInstaller(
        environmentReader: () => const <String, String>{'PATH': '/missing'},
        isWindows: false,
        executableExists: (_) => false,
        supportDirectory: () async => temp,
        processRunner: runner,
      );

      final result = await installer.ensureInstalled(provider);
      expect(result, isA<ManagedLanguageServerUnavailable>());
      expect(
        (result as ManagedLanguageServerUnavailable).reason,
        allOf(contains('node and npm'), contains('Check Again')),
      );
      expect(runner.installCalls, 0);
      expect(runner.probeCalls, 0);
    },
  );

  test(
    'broken node probe stops before npm install and explains recovery',
    () async {
      final runner = _FakeNpmInstallRunner(failNodeProbe: true);
      final installer = _installer(temp, runner);

      final result = await installer.ensureInstalled(provider);

      expect(result, isA<ManagedLanguageServerUnavailable>());
      expect(
        (result as ManagedLanguageServerUnavailable).reason,
        allOf(contains('node --version'), contains('Check Again')),
      );
      expect(runner.installCalls, 0);
      expect(runner.probeCalls, 1);
    },
  );

  test(
    'dotnet runtime without an SDK is reported before tool install',
    () async {
      final csharp = LanguageProviderDescriptor(
        id: 'csharp.csharp-ls',
        kind: LanguageProviderKind.semanticServer,
        languages: <LanguageId>{LanguageId('csharp')},
        capabilities: const <LanguageCapability>{LanguageCapability.definition},
        processScope: LanguageProviderProcessScope.workspace,
        launchPolicy: LanguageProviderLaunchPolicy.lazyOnDemand,
        executableResolutionPolicy:
            LanguageExecutableResolutionPolicy.explicitOverrideThenPath,
        executableCandidates: const <String>['csharp-ls'],
      );
      final runner = _CallbackProcessRunner((request) async {
        expect(request.arguments, const <String>['--list-sdks']);
        return ProcessResult(1, 0, '', '');
      });
      final installer = ManagedLanguageServerInstaller(
        environmentReader: () => const <String, String>{'PATH': '/tools'},
        isWindows: false,
        executableExists: (path) => path == '/tools/dotnet',
        supportDirectory: () async => temp,
        processRunner: runner,
      );

      final result = await installer.ensureInstalled(csharp);

      expect(result, isA<ManagedLanguageServerUnavailable>());
      expect(
        (result as ManagedLanguageServerUnavailable).reason,
        allOf(contains('.NET SDK'), contains('dotnet --list-sdks')),
      );
      expect(runner.calls, 1);
    },
  );

  test('catalog pins every first-wave external semantic provider', () {
    expect(
      managedLanguageServerRecipes.keys,
      containsAll(<String>[
        'csharp.csharp-ls',
        'go.gopls',
        'python.pyright',
        'rust.rust-analyzer',
        'php.intelephense',
        'typescript-javascript.typescript-language-server',
      ]),
    );
    for (final recipe in managedLanguageServerRecipes.values) {
      expect(recipe.version.trim(), isNotEmpty);
      expect(recipe.source, startsWith('https://'));
      expect(recipe.integrityPolicy.toLowerCase(), contains('sha-256'));
    }
  });
}

ManagedLanguageServerInstaller _installer(
  Directory support,
  ManagedLanguageServerProcessRunner runner,
) => ManagedLanguageServerInstaller(
  environmentReader: () => const <String, String>{'PATH': '/tools'},
  isWindows: false,
  executableExists: (path) =>
      path == '/tools/node' || path == '/tools/npm' || File(path).existsSync(),
  supportDirectory: () async => support,
  processRunner: runner,
);

final class _FakeNpmInstallRunner
    implements ManagedLanguageServerProcessRunner {
  _FakeNpmInstallRunner({this.gate, this.failNodeProbe = false});

  final Completer<void>? gate;
  final bool failNodeProbe;
  final Completer<void> started = Completer<void>();
  int probeCalls = 0;
  int installCalls = 0;

  @override
  Future<ProcessResult> run(ManagedLanguageServerProcessRequest request) async {
    if (request.arguments case ['--version']) {
      probeCalls += 1;
      if (request.executable.endsWith('/node') && failNodeProbe) {
        return ProcessResult(1, 1, '', 'broken node');
      }
      return ProcessResult(1, 0, '1.0.0', '');
    }
    installCalls += 1;
    if (!started.isCompleted) started.complete();
    await gate?.future;
    final prefixIndex = request.arguments.indexOf('--prefix');
    if (prefixIndex < 0 || prefixIndex + 1 >= request.arguments.length) {
      return ProcessResult(1, 2, '', 'missing --prefix');
    }
    final prefix = request.arguments[prefixIndex + 1];
    final bin = Directory(p.join(prefix, 'node_modules', '.bin'));
    await bin.create(recursive: true);
    await File(p.join(bin.path, 'pyright-langserver'))
        .writeAsString('#!/bin/sh\necho pyright\n', flush: true);
    await File(p.join(prefix, 'package-lock.json')).writeAsString(
      jsonEncode(<String, Object?>{
        'lockfileVersion': 3,
        'packages': <String, Object?>{
          'node_modules/pyright': <String, Object?>{
            'version': '1.1.414',
            'integrity': 'sha512-test-integrity',
          },
        },
      }),
      flush: true,
    );
    return ProcessResult(1, 0, '', '');
  }
}

final class _CallbackProcessRunner
    implements ManagedLanguageServerProcessRunner {
  _CallbackProcessRunner(this.callback);

  final Future<ProcessResult> Function(ManagedLanguageServerProcessRequest)
  callback;
  int calls = 0;

  @override
  Future<ProcessResult> run(ManagedLanguageServerProcessRequest request) {
    calls += 1;
    return callback(request);
  }
}
