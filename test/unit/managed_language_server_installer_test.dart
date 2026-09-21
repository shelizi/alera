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
      expect(runner.calls, 1);

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
      expect(runner.calls, 1);
    },
  );

  test('tampered managed executable is reinstalled', () async {
    final runner = _FakeNpmInstallRunner();
    final installer = _installer(temp, runner);
    final first = await installer.ensureInstalled(
      provider,
    ) as ManagedLanguageServerInstalled;
    expect(runner.calls, 1);

    await File(first.executable).writeAsString('tampered', flush: true);
    await installer.ensureInstalled(provider);
    expect(runner.calls, 2);
  });

  test('concurrent requests share one install operation', () async {
    final gate = Completer<void>();
    final runner = _FakeNpmInstallRunner(gate: gate);
    final installer = _installer(temp, runner);

    final first = installer.ensureInstalled(provider);
    final second = installer.ensureInstalled(provider);
    await runner.started.future;
    expect(runner.calls, 1);

    gate.complete();
    final results = await Future.wait(
      <Future<ManagedLanguageServerInstallResult>>[first, second],
    );
    expect(results, everyElement(isA<ManagedLanguageServerInstalled>()));
    expect(runner.calls, 1);
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
        contains('npm/Node.js'),
      );
      expect(runner.calls, 0);
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
  executableExists: (path) => path == '/tools/npm' || File(path).existsSync(),
  supportDirectory: () async => support,
  processRunner: runner,
);

final class _FakeNpmInstallRunner
    implements ManagedLanguageServerProcessRunner {
  _FakeNpmInstallRunner({this.gate});

  final Completer<void>? gate;
  final Completer<void> started = Completer<void>();
  int calls = 0;

  @override
  Future<ProcessResult> run(ManagedLanguageServerProcessRequest request) async {
    calls += 1;
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
