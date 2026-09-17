import 'dart:io';

import 'package:alera/src/features/agent_status/infra/agent_runtime_overlay_service.dart';
import 'package:alera/src/features/agent_status/infra/managed_agent_hook_installer.dart';
import 'package:alera/src/rust/api/agent_runtime_overlay.dart' as native;
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

part 'agent_runtime_overlay_amp_test_cases.dart';

void main() {
  group('AgentRuntimeOverlayService', () {
    final fakeNative = _FakeAgentRuntimeOverlayNative();
    late Directory home;
    late Directory support;

    setUp(() async {
      home = await Directory.systemTemp.createTemp('alera-overlay-home-');
      support = await Directory.systemTemp.createTemp('alera-overlay-support-');
    });

    tearDown(() {
      if (home.existsSync()) {
        home.deleteSync(recursive: true);
      }
      if (support.existsSync()) {
        support.deleteSync(recursive: true);
      }
    });

    AgentRuntimeOverlayService service({
      Map<String, String>? environment,
      ManagedAgentHookPlatform platform = ManagedAgentHookPlatform.posix,
      AgentOverlayApplicationSupportDirectoryResolver?
      applicationSupportDirectory,
    }) {
      return AgentRuntimeOverlayService(
        homeDirectory: home.path,
        platform: platform,
        environment: <String, String>{
          'HOME': home.path,
          'SHELL': '/bin/zsh',
          ...?environment,
        },
        applicationSupportDirectory:
            applicationSupportDirectory ?? () async => support,
        nativePreparer: fakeNative.prepare,
        nativeCleaner: fakeNative.clear,
      );
    }

    _registerAmpRuntimeOverlayTests(() => home, () => service());

    test('creates an OpenCode overlay with only the Alera plugin', () async {
      final preparation = await service().prepareOpenCodeForTerminalLaunch(
        terminalSessionId: 'session-1',
      );

      final overlay = preparation.overlayPath;
      expect(overlay, isNotNull);
      expect(
        preparation.environment,
        containsPair('OPENCODE_CONFIG_DIR', overlay),
      );
      expect(
        preparation.environment,
        containsPair('ALERA_OPENCODE_CONFIG_DIR', overlay),
      );

      final plugin = File(p.join(overlay!, 'plugins', 'alera-agent-status.js'))
          .readAsStringSync();
      expect(plugin, contains('ALERA_AGENT_STATUS_MANAGED_FILE'));
      expect(plugin, contains('AleraOpenCodeStatusPlugin'));
    });

    test(
      'mirrors OpenCode user config without overwriting same-name plugin',
      () async {
        final userConfig = Directory(p.join(home.path, 'opencode-user'))
          ..createSync(recursive: true);
        File(p.join(userConfig.path, 'opencode.json'))
            .writeAsStringSync('{"theme":"solarized"}');
        final plugins = Directory(p.join(userConfig.path, 'plugins'))
          ..createSync();
        File(p.join(plugins.path, 'user-plugin.js'))
            .writeAsStringSync('export default function userPlugin() {}\n');
        File(p.join(plugins.path, 'alera-agent-status.js'))
            .writeAsStringSync('USER OWNED PLUGIN\n');

        final preparation = await service(
          environment: <String, String>{'OPENCODE_CONFIG_DIR': userConfig.path},
        ).prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-2');

        final overlay = preparation.overlayPath!;
        expect(preparation.sourcePath, userConfig.path);
        expect(
          File(p.join(overlay, 'opencode.json')).readAsStringSync(),
          '{"theme":"solarized"}',
        );
        expect(
          File(p.join(overlay, 'plugins', 'user-plugin.js')).readAsStringSync(),
          'export default function userPlugin() {}\n',
        );
        final overlayPlugin = File(
          p.join(overlay, 'plugins', 'alera-agent-status.js'),
        ).readAsStringSync();
        expect(overlayPlugin, contains('AleraOpenCodeStatusPlugin'));
        expect(overlayPlugin, isNot('USER OWNED PLUGIN\n'));
        expect(
          File(p.join(plugins.path, 'alera-agent-status.js'))
              .readAsStringSync(),
          'USER OWNED PLUGIN\n',
        );
      },
    );

    test('preserves an explicit missing OpenCode config path', () async {
      final missing = p.join(home.path, 'missing-opencode');

      final preparation = await service(
        environment: <String, String>{'OPENCODE_CONFIG_DIR': missing},
      ).prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-3');

      expect(preparation.overlayPath, isNull);
      expect(preparation.environment, <String, String>{
        'OPENCODE_CONFIG_DIR': missing,
      });
      expect(Directory(missing).existsSync(), isFalse);
    });

    test(
      'falls back to explicit OpenCode source when overlay creation fails',
      () async {
        final userConfig = Directory(p.join(home.path, 'opencode-user'))
          ..createSync(recursive: true);
        File(p.join(userConfig.path, 'opencode.json')).writeAsStringSync('{}');
        final supportFile = File(p.join(home.path, 'support-file'))
          ..writeAsStringSync('not a directory');

        final preparation = await service(
          environment: <String, String>{'OPENCODE_CONFIG_DIR': userConfig.path},
          applicationSupportDirectory: () async => Directory(supportFile.path),
        ).prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-fail');

        expect(preparation.overlayPath, isNull);
        expect(preparation.sourcePath, userConfig.path);
        expect(preparation.environment, <String, String>{
          'OPENCODE_CONFIG_DIR': userConfig.path,
        });
      },
    );

    test('uses bash startup exports as the OpenCode source', () async {
      final userConfig = Directory(p.join(home.path, 'custom-opencode'))
        ..createSync(recursive: true);
      File(p.join(userConfig.path, 'opencode.json')).writeAsStringSync('{}');
      File(p.join(home.path, '.bashrc')).writeAsStringSync(
        'export OPENCODE_CONFIG_DIR="\${HOME}/custom-opencode" # user config\n',
      );

      final preparation = await service(
        environment: <String, String>{'SHELL': '/bin/bash'},
      ).prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-bash');

      final overlay = preparation.overlayPath!;
      expect(preparation.sourcePath, userConfig.path);
      expect(
        preparation.environment,
        containsPair('ALERA_OPENCODE_SOURCE_CONFIG_DIR', userConfig.path),
      );
      expect(File(p.join(overlay, 'opencode.json')).readAsStringSync(), '{}');
    });

    test('uses unquoted shell startup exports as overlay sources', () async {
      final userConfig = Directory(p.join(home.path, 'plain-opencode'))
        ..createSync(recursive: true);
      File(p.join(userConfig.path, 'opencode.json')).writeAsStringSync('{}');
      File(p.join(home.path, '.bashrc'))
          .writeAsStringSync('export OPENCODE_CONFIG_DIR=${userConfig.path}\n');

      final preparation = await service(
        environment: <String, String>{'SHELL': '/bin/bash'},
      ).prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-unquoted');

      expect(preparation.sourcePath, userConfig.path);
    });

    test(
      'uses zsh ZDOTDIR startup exports and preserves single quotes',
      () async {
        final userConfig = Directory(p.join(home.path, 'literal-opencode'))
          ..createSync(recursive: true);
        File(p.join(userConfig.path, 'opencode.json')).writeAsStringSync('{}');
        final zdotdir = Directory(p.join(home.path, 'zsh-config'))
          ..createSync(recursive: true);
        File(p.join(home.path, '.zshenv'))
            .writeAsStringSync('export ZDOTDIR="\${HOME}/zsh-config"\n');
        File(p.join(zdotdir.path, '.zshrc')).writeAsStringSync(
          "export OPENCODE_CONFIG_DIR='${userConfig.path}' # literal source\n",
        );

        final preparation = await service(
          environment: <String, String>{'SHELL': '/bin/zsh'},
        ).prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-zsh');

        expect(preparation.sourcePath, userConfig.path);
        expect(
          File(p.join(preparation.overlayPath!, 'opencode.json'))
              .readAsStringSync(),
          '{}',
        );
      },
    );

    test(
      'uses explicit OpenCode source env before public env values',
      () async {
        final explicitSource = Directory(p.join(home.path, 'explicit-opencode'))
          ..createSync(recursive: true);
        File(p.join(explicitSource.path, 'opencode.json'))
            .writeAsStringSync('{"source":"explicit"}');
        final publicSource = Directory(p.join(home.path, 'public-opencode'))
          ..createSync(recursive: true);
        File(p.join(publicSource.path, 'opencode.json'))
            .writeAsStringSync('{"source":"public"}');

        final preparation = await service(
          environment: <String, String>{
            'ALERA_OPENCODE_SOURCE_CONFIG_DIR': explicitSource.path,
            'OPENCODE_CONFIG_DIR': publicSource.path,
          },
        ).prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-source');

        expect(preparation.sourcePath, explicitSource.path);
        expect(
          File(p.join(preparation.overlayPath!, 'opencode.json'))
              .readAsStringSync(),
          '{"source":"explicit"}',
        );
      },
    );

    test(
      'mirrors nested OpenCode resources through the native bridge',
      () async {
        final userConfig = Directory(p.join(home.path, 'nested-opencode'))
          ..createSync(recursive: true);
        File(p.join(userConfig.path, 'opencode.json')).writeAsStringSync('{}');
        final nested = Directory(p.join(userConfig.path, 'nested'))
          ..createSync();
        File(p.join(nested.path, 'config.json')).writeAsStringSync('nested');

        final preparation = await service(
          environment: <String, String>{'OPENCODE_CONFIG_DIR': userConfig.path},
        ).prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-nested');

        expect(
          File(p.join(preparation.overlayPath!, 'nested', 'config.json'))
              .readAsStringSync(),
          'nested',
        );
      },
    );

    test('preserves an explicit missing Copilot home path', () async {
      final missing = p.join(home.path, 'missing-copilot');

      final preparation =
          await service(environment: <String, String>{'COPILOT_HOME': missing})
              .prepareCopilotForTerminalLaunch(
                terminalSessionId: 'session-copilot-missing',
              );

      expect(preparation.overlayPath, isNull);
      expect(preparation.sourcePath, missing);
      expect(preparation.environment, <String, String>{
        'COPILOT_HOME': missing,
      });
    });

    test('falls back without overriding Copilot home when default hook install fails', () async {
      File(p.join(home.path, '.copilot')).writeAsStringSync('not a directory');

      final preparation = await service().prepareCopilotForTerminalLaunch(
        terminalSessionId: 'session-copilot-fail',
      );

      expect(preparation.overlayPath, isNull);
      expect(preparation.sourcePath, isNull);
      expect(preparation.environment, isEmpty);
    });

    test(
      'falls back to explicit Copilot source when managed hook install fails',
      () async {
        final copilotHome = File(p.join(home.path, 'copilot-user'))
          ..writeAsStringSync('not a directory');

        final preparation =
            await service(
              environment: <String, String>{'COPILOT_HOME': copilotHome.path},
            ).prepareCopilotForTerminalLaunch(
              terminalSessionId: 'session-copilot-install-fail',
            );

        expect(preparation.overlayPath, isNull);
        expect(preparation.sourcePath, copilotHome.path);
        expect(preparation.environment, <String, String>{
          'COPILOT_HOME': copilotHome.path,
        });
      },
    );

    test('falls back to an empty Amp overlay when setup fails', () async {
      final ampSupportFile = File(p.join(home.path, 'amp-support-file'))
        ..writeAsStringSync('not a directory');
      final ampPreparation = await service(
        applicationSupportDirectory: () async => Directory(ampSupportFile.path),
      ).prepareAmpForTerminalLaunch(terminalSessionId: 'session-amp-fail');

      expect(ampPreparation.overlayPath, isNull);
      expect(ampPreparation.environment, isEmpty);
    });

    test(
      'keeps Pi agent state in place and installs the status extension',
      () async {
        final piAgent = Directory(p.join(home.path, '.pi', 'agent'))
          ..createSync(recursive: true);
        File(p.join(piAgent.path, 'auth.json'))
            .writeAsStringSync('secret token');
        Directory(p.join(piAgent.path, 'sessions')).createSync();
        final session = File(p.join(piAgent.path, 'sessions', 'session.json'))
          ..writeAsStringSync('{}');
        final extensions = Directory(p.join(piAgent.path, 'extensions'))
          ..createSync();
        File(p.join(extensions.path, 'user-extension.ts'))
            .writeAsStringSync('export default function userExtension() {}\n');

        final preparation = await service().preparePiForTerminalLaunch(
          terminalSessionId: 'session-4',
        );

        expect(preparation.overlayPath, isNull);
        expect(preparation.sourcePath, piAgent.path);
        expect(preparation.environment, isEmpty);
        expect(
          File(p.join(piAgent.path, 'auth.json')).readAsStringSync(),
          'secret token',
        );
        expect(session.readAsStringSync(), '{}');
        expect(
          File(p.join(extensions.path, 'user-extension.ts')).readAsStringSync(),
          'export default function userExtension() {}\n',
        );
        final statusExtension = File(
          p.join(extensions.path, 'alera-agent-status.ts'),
        ).readAsStringSync();
        expect(statusExtension, contains('ALERA_AGENT_STATUS_MANAGED_FILE'));
        expect(statusExtension, contains("pi.on('agent_start'"));
      },
    );

    test(
      'keeps Copilot on its global home and installs managed hooks',
      () async {
        final copilotHome = Directory(p.join(home.path, '.copilot'))
          ..createSync(recursive: true);
        File(p.join(copilotHome.path, 'settings.json')).writeAsStringSync('{}');
        final sessionStore = File(p.join(copilotHome.path, 'session-store.db'))
          ..writeAsStringSync('resume-state');
        final hooks = Directory(p.join(copilotHome.path, 'hooks'))
          ..createSync();
        File(p.join(hooks.path, 'alera.json')).writeAsStringSync(
          '{"hooks":{"UserPromptSubmit":[{"command":"user-owned"}]}}\n',
        );

        final preparation = await service().prepareCopilotForTerminalLaunch(
          terminalSessionId: 'session-copilot',
        );

        expect(preparation.overlayPath, isNull);
        expect(preparation.sourcePath, copilotHome.path);
        expect(preparation.environment, isEmpty);
        expect(sessionStore.readAsStringSync(), 'resume-state');
        final managedHooks = File(p.join(hooks.path, 'alera.json'))
            .readAsStringSync();
        expect(managedHooks, contains('ALERA_COPILOT_HOOK_EVENT'));
        expect(managedHooks, contains('UserPromptSubmit'));
        expect(
          File(
            p.join(
              copilotHome.path,
              '.alera',
              'agent-hooks',
              'alera-copilot-hook.sh',
            ),
          ).readAsStringSync(),
          contains('/hook/copilot'),
        );
      },
    );

    test(
      'resolves Amp sources from explicit, public, and XDG env values',
      () async {
        final explicitSource = Directory(p.join(home.path, 'explicit-amp'))
          ..createSync(recursive: true);
        File(p.join(explicitSource.path, 'settings.json'))
            .writeAsStringSync('{"source":"explicit"}\n');
        final explicitPreparation =
            await service(
              environment: <String, String>{
                'ALERA_AMP_SOURCE_CONFIG_DIR': explicitSource.path,
              },
            ).prepareAmpForTerminalLaunch(
              terminalSessionId: 'session-amp-explicit',
            );
        expect(explicitPreparation.sourcePath, explicitSource.path);

        final publicSource = Directory(p.join(home.path, 'public-amp'))
          ..createSync(recursive: true);
        File(p.join(publicSource.path, 'settings.json'))
            .writeAsStringSync('{"source":"public"}\n');
        final publicPreparation = await service(
          environment: <String, String>{
            'AMP_CONFIG_DIR': publicSource.path,
            'ALERA_AMP_CONFIG_DIR': p.join(home.path, 'old-overlay'),
          },
        ).prepareAmpForTerminalLaunch(terminalSessionId: 'session-amp-public');
        expect(publicPreparation.sourcePath, publicSource.path);

        final xdgHome = Directory(p.join(home.path, 'xdg'))..createSync();
        final xdgSource = Directory(p.join(xdgHome.path, 'amp'))
          ..createSync(recursive: true);
        File(p.join(xdgSource.path, 'settings.json'))
            .writeAsStringSync('{"source":"xdg"}\n');
        final xdgPreparation = await service(
          environment: <String, String>{'XDG_CONFIG_HOME': xdgHome.path},
        ).prepareAmpForTerminalLaunch(terminalSessionId: 'session-amp-xdg');
        expect(xdgPreparation.sourcePath, xdgSource.path);

        final defaultAmp = Directory(p.join(home.path, '.config', 'amp'))
          ..createSync(recursive: true);
        File(p.join(defaultAmp.path, 'settings.json'))
            .writeAsStringSync('{"source":"default"}\n');
        final skippedXdgPreparation = await service(
          environment: <String, String>{
            'XDG_CONFIG_HOME': xdgHome.path,
            'ALERA_AMP_CONFIG_DIR': xdgSource.path,
          },
        ).prepareAmpForTerminalLaunch(terminalSessionId: 'session-amp-default');
        expect(skippedXdgPreparation.sourcePath, defaultAmp.path);
      },
    );

    test(
      'ignores public source env values that already point at the overlay',
      () async {
        final defaultOpenCode = Directory(
          p.join(home.path, '.config', 'opencode'),
        )..createSync(recursive: true);
        File(p.join(defaultOpenCode.path, 'opencode.json'))
            .writeAsStringSync('{"source":"default"}');

        final preparation =
            await service(
              environment: <String, String>{
                'OPENCODE_CONFIG_DIR': '/runtime/opencode',
                'ALERA_OPENCODE_CONFIG_DIR': '/runtime/opencode',
              },
            ).prepareOpenCodeForTerminalLaunch(
              terminalSessionId: 'session-overlay-env',
            );

        expect(preparation.sourcePath, defaultOpenCode.path);
      },
    );

    test(
      'replaces managed-file directories while preparing overlays',
      () async {
        final userConfig = Directory(p.join(home.path, 'opencode-managed-dir'))
          ..createSync(recursive: true);
        Directory(p.join(userConfig.path, 'plugins', 'alera-agent-status.js'))
            .createSync(recursive: true);

        final preparation =
            await service(
              environment: <String, String>{
                'OPENCODE_CONFIG_DIR': userConfig.path,
              },
            ).prepareOpenCodeForTerminalLaunch(
              terminalSessionId: 'session-managed-dir',
            );

        expect(
          File(
            p.join(
              preparation.overlayPath!,
              'plugins',
              'alera-agent-status.js',
            ),
          ).readAsStringSync(),
          contains('AleraOpenCodeStatusPlugin'),
        );
      },
    );

    test('uses Windows default overlay source paths', () async {
      final appData = Directory(p.join(home.path, 'AppData', 'Roaming'))
        ..createSync(recursive: true);
      final opencode = Directory(p.join(appData.path, 'opencode'))
        ..createSync(recursive: true);
      File(p.join(opencode.path, 'opencode.json')).writeAsStringSync('{}');
      final amp = Directory(p.join(home.path, '.config', 'amp'))
        ..createSync(recursive: true);
      File(p.join(amp.path, 'settings.json')).writeAsStringSync('{}');

      final windowsService = service(
        platform: .windows,
        environment: <String, String>{
          'USERPROFILE': home.path,
          'APPDATA': appData.path,
        },
      );

      final openCodePreparation = await windowsService
          .prepareOpenCodeForTerminalLaunch(terminalSessionId: 'windows-open');
      final ampPreparation = await windowsService.prepareAmpForTerminalLaunch(
        terminalSessionId: 'windows-amp',
      );

      expect(openCodePreparation.sourcePath, opencode.path);
      expect(ampPreparation.sourcePath, amp.path);
    });

    test(
      'resolves home from USERPROFILE and current directory fallbacks',
      () async {
        final profileHome = Directory(p.join(home.path, 'profile-home'))
          ..createSync(recursive: true);
        final piAgent = Directory(p.join(profileHome.path, '.pi', 'agent'))
          ..createSync(recursive: true);
        File(p.join(piAgent.path, 'auth.json'))
            .writeAsStringSync('profile auth');
        final profileService = AgentRuntimeOverlayService(
          environment: <String, String>{'USERPROFILE': profileHome.path},
          applicationSupportDirectory: () async => support,
          nativePreparer: fakeNative.prepare,
          nativeCleaner: fakeNative.clear,
        );

        final piPreparation = await profileService.preparePiForTerminalLaunch(
          terminalSessionId: 'session-profile-home',
        );

        expect(piPreparation.sourcePath, piAgent.path);

        final currentFallbackService = AgentRuntimeOverlayService(
          environment: const <String, String>{},
          applicationSupportDirectory: () async => support,
          nativePreparer: fakeNative.prepare,
          nativeCleaner: fakeNative.clear,
        );
        final openCodePreparation = await currentFallbackService
            .prepareOpenCodeForTerminalLaunch(
              terminalSessionId: 'session-current',
            );

        expect(openCodePreparation.overlayPath, isNotNull);
        expect(openCodePreparation.sourcePath, isNull);
      },
    );

    test(
      'clearTerminalOverlays removes overlays without touching sources',
      () async {
        final userConfig = Directory(p.join(home.path, 'opencode-user'))
          ..createSync(recursive: true);
        File(p.join(userConfig.path, 'auth.json')).writeAsStringSync('auth');

        final overlayService = service(
          environment: <String, String>{'OPENCODE_CONFIG_DIR': userConfig.path},
        );
        final preparation = await overlayService
            .prepareOpenCodeForTerminalLaunch(terminalSessionId: 'session-5');
        final ampPreparation = await overlayService.prepareAmpForTerminalLaunch(
          terminalSessionId: 'session-5',
        );
        expect(Directory(preparation.overlayPath!).existsSync(), isTrue);
        expect(Directory(ampPreparation.overlayPath!).existsSync(), isTrue);

        await overlayService.clearTerminalOverlays('session-5');

        expect(Directory(preparation.overlayPath!).existsSync(), isFalse);
        expect(Directory(ampPreparation.overlayPath!).existsSync(), isFalse);
        expect(
          File(p.join(userConfig.path, 'auth.json')).readAsStringSync(),
          'auth',
        );
      },
    );
  });
}

final class _FakeAgentRuntimeOverlayNative {
  Future<native.AgentRuntimeOverlayResult> prepare({
    required native.AgentRuntimeOverlayRequest request,
  }) async {
    var removedCount = 0;
    var copiedCount = 0;
    var writtenCount = 0;

    final overlayPath = request.overlayPath;
    if (overlayPath != null) {
      removedCount += _removeEntity(overlayPath);
      Directory(overlayPath).createSync(recursive: true);
    }
    final mirrorPath = request.mirrorPath ?? overlayPath;
    if (mirrorPath != null) {
      Directory(mirrorPath).createSync(recursive: true);
    }

    final sourcePath = request.sourcePath;
    final sourceExists =
        sourcePath != null && Directory(sourcePath).existsSync();
    if (sourceExists && mirrorPath != null) {
      copiedCount += _mirrorSource(
        sourcePath: sourcePath,
        targetPath: mirrorPath,
        managedSubdirectory: request.managedSubdirectory,
        managedFileNames: request.managedFileNames.toSet(),
      );
    }

    for (final managedFile in request.managedFiles) {
      if (_writeManagedFile(managedFile)) {
        writtenCount += 1;
      }
    }

    return native.AgentRuntimeOverlayResult(
      sourceExists: sourceExists,
      linkedCount: BigInt.zero,
      copiedCount: BigInt.from(copiedCount),
      writtenCount: BigInt.from(writtenCount),
      removedCount: BigInt.from(removedCount),
      warnings: const <String>[],
    );
  }

  Future<native.AgentRuntimeOverlayCleanupResult> clear({
    required List<native.AgentRuntimeOverlayCleanupTarget> targets,
  }) async {
    var removedCount = 0;
    for (final target in targets) {
      removedCount += _removeEntity(target.overlayPath);
    }
    return native.AgentRuntimeOverlayCleanupResult(
      removedCount: BigInt.from(removedCount),
      warnings: const <String>[],
    );
  }

  int _mirrorSource({
    required String sourcePath,
    required String targetPath,
    required String? managedSubdirectory,
    required Set<String> managedFileNames,
  }) {
    var copiedCount = 0;
    for (final entity in Directory(sourcePath).listSync(followLinks: false)) {
      final name = p.basename(entity.path);
      final type = FileSystemEntity.typeSync(entity.path, followLinks: false);
      if (managedSubdirectory != null &&
          name == managedSubdirectory &&
          type == FileSystemEntityType.directory) {
        final managedTarget = p.join(targetPath, name);
        Directory(managedTarget).createSync(recursive: true);
        for (final child in Directory(
          entity.path,
        ).listSync(followLinks: false)) {
          if (managedFileNames.contains(p.basename(child.path))) {
            continue;
          }
          copiedCount += _copyEntity(
            child.path,
            p.join(managedTarget, p.basename(child.path)),
          );
        }
        continue;
      }
      copiedCount += _copyEntity(entity.path, p.join(targetPath, name));
    }
    return copiedCount;
  }

  int _copyEntity(String sourcePath, String targetPath) {
    final type = FileSystemEntity.typeSync(sourcePath, followLinks: false);
    switch (type) {
      case FileSystemEntityType.directory:
        Directory(targetPath).createSync(recursive: true);
        var copiedCount = 1;
        for (final child in Directory(
          sourcePath,
        ).listSync(followLinks: false)) {
          copiedCount += _copyEntity(
            child.path,
            p.join(targetPath, p.basename(child.path)),
          );
        }
        return copiedCount;
      case FileSystemEntityType.file:
        Directory(p.dirname(targetPath)).createSync(recursive: true);
        File(sourcePath).copySync(targetPath);
        return 1;
      case FileSystemEntityType.link:
        Directory(p.dirname(targetPath)).createSync(recursive: true);
        Link(targetPath).createSync(Link(sourcePath).targetSync());
        return 1;
      case FileSystemEntityType.notFound:
        return 0;
      default:
        throw FileSystemException(
          'Unsupported fake overlay entity',
          sourcePath,
        );
    }
  }

  bool _writeManagedFile(native.AgentRuntimeOverlayManagedFile managedFile) {
    if (managedFile.writeMode ==
            native.AgentRuntimeOverlayWriteMode.createIfMissing &&
        File(managedFile.path).existsSync()) {
      return false;
    }
    _removeEntity(managedFile.path);
    Directory(p.dirname(managedFile.path)).createSync(recursive: true);
    File(managedFile.path).writeAsStringSync(managedFile.content);
    return true;
  }

  int _removeEntity(String path) {
    final type = FileSystemEntity.typeSync(path, followLinks: false);
    switch (type) {
      case FileSystemEntityType.directory:
        Directory(path).deleteSync(recursive: true);
        return 1;
      case FileSystemEntityType.file:
        File(path).deleteSync();
        return 1;
      case FileSystemEntityType.link:
        Link(path).deleteSync();
        return 1;
      case FileSystemEntityType.notFound:
        return 0;
      default:
        throw FileSystemException('Unsupported fake overlay entity', path);
    }
  }
}
