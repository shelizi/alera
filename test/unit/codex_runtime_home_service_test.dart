import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/agent_status/infra/codex_runtime_home_service.dart';
import 'package:alera/src/features/agent_status/infra/managed_agent_hook_installer.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;
import 'package:toml/toml.dart';

part 'codex_runtime_home_service_test_harness.dart';

part 'codex_runtime_home_service_core_test_cases.dart';
part 'codex_runtime_home_service_advanced_test_cases.dart';
part 'codex_runtime_home_service_duplicate_trust_test_cases.dart';

late Directory root;
late Directory home;
late Directory support;
late CodexRuntimeHomeService service;

void main() {
  group('CodexRuntimeHomeService', () {
    setUp(() async {
      root = await Directory.systemTemp.createTemp('alera-codex-runtime-');
      home = Directory(p.join(root.path, 'home'))..createSync(recursive: true);
      support = Directory(p.join(root.path, 'support'))
        ..createSync(recursive: true);
      service = CodexRuntimeHomeService(
        homeDirectory: home.path,
        applicationSupportDirectory: () async => support,
        platform: .posix,
        environment: <String, String>{'HOME': home.path},
      );
    });

    tearDown(() {
      if (root.existsSync()) {
        root.deleteSync(recursive: true);
      }
    });

    _registerCodexRuntimeHomeServiceCoreTests();
    _registerCodexRuntimeHomeServiceAdvancedTests();
    _registerCodexRuntimeHomeServiceDuplicateTrustTests();
    test(
      'actual Windows Codex managed Hook script sends prompt and Stop events',
      () async {
        final isolatedHome = Directory(
          p.join(root.path, 'Windows Codex Hook Home With Spaces'),
        )..createSync(recursive: true);
        final windowsService = CodexRuntimeHomeService(
          homeDirectory: isolatedHome.path,
          applicationSupportDirectory: () async => support,
          platform: .windows,
          environment: <String, String>{'USERPROFILE': isolatedHome.path},
        );
        final preparation = await windowsService.prepareForTerminalLaunch();
        expect(
          preparation.hookStatus.state,
          ManagedAgentHookInstallState.installed,
        );
        final scriptPath = p.join(
          isolatedHome.path,
          '.alera',
          'agent-hooks',
          'alera-codex-hook.cmd',
        );
        expect(File(scriptPath).existsSync(), isTrue);

        final server = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
        addTearDown(() => server.close(force: true));
        final events = <Map<String, Object?>>[];
        final observed = Completer<void>();
        server.listen((request) async {
          final rawBody = await utf8.decoder.bind(request).join();
          final body = Map<String, Object?>.from(jsonDecode(rawBody) as Map);
          events.add(<String, Object?>{
            'method': request.method,
            'path': request.uri.path,
            'tokenValid':
                request.headers.value('X-Alera-Agent-Hook-Token') ==
                'test-only-codex-token',
            'body': body,
          });
          request.response.statusCode = HttpStatus.noContent;
          await request.response.close();
          if (events.length == 2 && !observed.isCompleted) {
            observed.complete();
          }
        });

        for (final eventName in <String>['UserPromptSubmit', 'Stop']) {
          final child = await Process.start(
            'cmd.exe',
            <String>['/d', '/c', 'call', scriptPath],
            workingDirectory: isolatedHome.path,
            environment: <String, String>{
              'ALERA_AGENT_HOOK_PORT': server.port.toString(),
              'ALERA_AGENT_HOOK_TOKEN': 'test-only-codex-token',
              'ALERA_TERMINAL_SESSION_ID': 'private-codex-session',
              'ALERA_WORKSPACE_ID': 'private-workspace',
              'ALERA_TAB_ID': 'private-tab',
              'ALERA_AGENT_HOOK_EVENT': eventName,
              'ALERA_AGENT_HOOK_VERSION': '1',
            },
          );
          try {
            final stdout = child.stdout
                .transform(const Utf8Decoder(allowMalformed: true))
                .join();
            final stderr = child.stderr
                .transform(const Utf8Decoder(allowMalformed: true))
                .join();
            child.stdin.write(
              jsonEncode(<String, Object?>{
                'hook_event_name': eventName,
                'thread_id': 'private-codex-thread',
              }),
            );
            await child.stdin.close();
            final exitCode = await child.exitCode.timeout(
              const Duration(seconds: 25),
            );
            final output = await stdout;
            final errorOutput = await stderr;
            final safeHint = (output + ' ' + errorOutput)
                .replaceAll(isolatedHome.path, '<isolated-home>')
                .replaceAll('test-only-codex-token', '[redacted]')
                .trim();
            expect(
              exitCode,
              0,
              reason: safeHint.length > 450
                  ? safeHint.substring(0, 450)
                  : safeHint,
            );
          } finally {
            child.kill();
          }
        }

        await observed.future.timeout(const Duration(seconds: 5));
        expect(events, hasLength(2));
        for (var i = 0; i < events.length; i++) {
          final actual = events[i];
          expect(actual['method'], 'POST');
          expect(actual['path'], '/hook/codex');
          expect(actual['tokenValid'], isTrue);
          final body = Map<String, Object?>.from(actual['body']! as Map);
          expect(body['terminalSessionId'], 'private-codex-session');
          expect(body['workspaceId'], 'private-workspace');
          expect(body['tabId'], 'private-tab');
          expect(
            body['hookEventName'],
            <String>['UserPromptSubmit', 'Stop'][i],
          );
          expect(body['version'], '1');
          expect(
            (body['payload'] as Map)['hook_event_name'],
            body['hookEventName'],
          );
        }
      },
      skip: !Platform.isWindows,
    );
  });
}
