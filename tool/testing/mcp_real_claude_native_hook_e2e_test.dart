// Explicit-only real Claude CLI native Hook acceptance.
// Run: flutter test --no-pub tool/testing/mcp_real_claude_native_hook_e2e_test.dart
// Uses an authenticated but non-persistent --print model turn, a disposable
// workspace/home and Alera's actual generated Hook scripts. Never copies,
// edits or approves personal Claude settings, trust or credentials.
import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/application/agent_status_host_forwarder.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/agent_status/infra/agent_hook_request_parser.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:alera/src/features/agent_status/infra/claude_runtime_home_service.dart';
import 'package:alera/src/features/agent_status/infra/managed_agent_hook_installer.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

void main() {
  test(
    'real Claude provider emits native prompt and Stop through Alera Hook',
    () async {
      final root = await Directory.systemTemp.createTemp(
        'alera-native-claude-model-hook-',
      );
      addTearDown(() async {
        if (root.existsSync()) {
          await root.delete(recursive: true);
        }
      });
      final home = Directory(p.join(root.path, 'private home'))
        ..createSync(recursive: true);
      final support = Directory(p.join(root.path, 'support'))
        ..createSync(recursive: true);
      final workspace = Directory(p.join(root.path, 'private workspace'))
        ..createSync(recursive: true);
      File(p.join(workspace.path, 'sentinel.txt')).writeAsStringSync(
        'Safe test-only Claude workspace. No real project files.\n',
      );

      final installed = await Process.run('where.exe', <String>['claude.exe']);
      expect(installed.exitCode, 0, reason: 'Claude CLI not installed');
      final claude = installed.stdout
          .toString()
          .split(RegExp(r'[\r\n]+'))
          .firstWhere((line) => line.toLowerCase().endsWith('claude.exe'));
      final prepared = await ClaudeRuntimeHomeService(
        homeDirectory: home.path,
        applicationSupportDirectory: () async => support,
        platform: .windows,
        environment: <String, String>{'USERPROFILE': home.path},
        syncMacOSKeychainCredentials: false,
      ).prepareForTerminalLaunch();
      expect(prepared.hookStatus.state, ManagedAgentHookInstallState.installed);
      final settingsPath = p.join(prepared.runtimeHomePath, 'settings.json');
      final hookPath = p.join(
        home.path,
        '.alera',
        'agent-hooks',
        'alera-claude-hook.cmd',
      );
      expect(File(hookPath).existsSync(), isTrue);
      expect(File(settingsPath).existsSync(), isTrue);

      final events = <String>[];
      final host = _RecordingHookPresenceHost();
      addTearDown(host.dispose);
      final container = ProviderContainer();
      addTearDown(container.dispose);
      final forwarder = AgentStatusHostForwarder(
        client: host,
        debounce: const Duration(milliseconds: 1),
      );
      addTearDown(forwarder.dispose);
      final stateSubscription = container.listen(
        agentStatusControllerProvider,
        forwarder.onStatusChanged,
        fireImmediately: true,
      );
      addTearDown(stateSubscription.close);
      final sink = await HttpServer.bind(InternetAddress.loopbackIPv4, 0);
      addTearDown(() => sink.close(force: true));
      sink.listen((request) async {
        try {
          final bytes = await request.fold<List<int>>(
            <int>[],
            (bytes, chunk) => bytes..addAll(chunk),
          );
          final data = jsonDecode(utf8.decode(bytes));
          final body = Map<String, Object?>.from(data as Map);
          if (request.method == 'POST' &&
              request.uri.path == '/hook/claude' &&
              request.headers.value('X-Alera-Agent-Hook-Token') ==
                  'alera-disposable-token' &&
              body['terminalSessionId'] == 'private-managed-pty' &&
              body['workspaceId'] == 'private-workspace' &&
              body['tabId'] == 'private-tab' &&
              body['hookEventName'] is String) {
            // Preserve only event names. Do not persist model prompts,
            // transcripts, tokens, workspace paths or hook payloads.
            // Use Alera's production request parser, status normalizer and
            // Host forwarder, not a synthetic working/done test adapter.
            final parsed = parseAgentHookRequest(
              agentType: AgentType.claude,
              body: body,
            );
            if (parsed == null) {
              request.response.statusCode = HttpStatus.badRequest;
            } else {
              container
                  .read(agentStatusControllerProvider.notifier)
                  .applyHookEvent(parsed);
              events.add(body['hookEventName']! as String);
              request.response.statusCode = HttpStatus.noContent;
            }
          } else {
            request.response.statusCode = HttpStatus.forbidden;
          }
        } catch (_) {
          request.response.statusCode = HttpStatus.badRequest;
        }
        await request.response.close();
      });

      final model = await Process.start(
        claude,
        <String>[
          '--no-session-persistence',
          '--setting-sources',
          '',
          '--settings',
          settingsPath,
          '--strict-mcp-config',
          '--tools',
          '',
          '--permission-prompts',
          'none',
          '--max-turns',
          '2',
          '--max-budget-usd',
          '0.10',
          '--output-format',
          'stream-json',
          '--verbose',
          '--print',
          'This is an isolated Alera status hook smoke test. '
              'Do not use any tools or edit files. '
              'Reply exactly ALERA_NATIVE_CLAUDE_HOOK_TEST_OK.',
        ],
        workingDirectory: workspace.path,
        environment: <String, String>{
          ...Platform.environment,
          'ALERA_AGENT_HOOK_PORT': '${sink.port}',
          'ALERA_AGENT_HOOK_TOKEN': 'alera-disposable-token',
          'ALERA_TERMINAL_SESSION_ID': 'private-managed-pty',
          'ALERA_WORKSPACE_ID': 'private-workspace',
          'ALERA_TAB_ID': 'private-tab',
          'ALERA_AGENT_HOOK_VERSION': '1',
        },
        mode: ProcessStartMode.normal,
      );
      final stdout = model.stdout
          .transform(const Utf8Decoder(allowMalformed: true))
          .join();
      final stderr = model.stderr
          .transform(const Utf8Decoder(allowMalformed: true))
          .join();
      int exitCode;
      try {
        exitCode = await model.exitCode.timeout(const Duration(seconds: 115));
      } on TimeoutException {
        model.kill();
        fail('Native Claude model invocation did not finish in time');
      }
      final output = await stdout;
      await stderr; // Intentionally discard diagnostic text with user context.
      expect(
        exitCode,
        0,
        reason: 'Real Claude CLI did not finish successfully',
      );
      final completed = output.split('\n').any((line) {
        try {
          final event = jsonDecode(line);
          return event is Map &&
              event['type'] == 'result' &&
              event['is_error'] != true &&
              event['result'].toString().contains(
                'ALERA_NATIVE_CLAUDE_HOOK_TEST_OK',
              );
        } catch (_) {
          return false;
        }
      });
      expect(completed, isTrue, reason: 'Authenticated model result missing');
      for (
        var attempt = 0;
        attempt < 50 && !events.contains('Stop');
        attempt++
      ) {
        await Future<void>.delayed(const Duration(milliseconds: 100));
      }
      expect(events, contains('UserPromptSubmit'));
      expect(events, contains('Stop'));
      expect(
        events.indexOf('UserPromptSubmit'),
        lessThan(events.lastIndexOf('Stop')),
      );
      for (
        var attempt = 0;
        attempt < 100 && !host.states.contains('done');
        attempt++
      ) {
        await Future<void>.delayed(const Duration(milliseconds: 25));
      }
      expect(host.states, containsAllInOrder(<String>['working', 'done']));
      expect(host.allForwardedScopesValid, isTrue);
      final presence = container.read(agentStatusControllerProvider);
      expect(presence['private-managed-pty']?.state, AgentStatusState.done);
    },
    skip: !Platform.isWindows,
  );
}

class _RecordingHookPresenceHost implements RuntimeHostClient {
  final states = <String>[];
  bool allForwardedScopesValid = true;
  final _events = StreamController<RuntimeHostEvent>.broadcast();

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => _events.stream;

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) async {
    if (type == 'orchestration.agentStatus') {
      for (final entry in (payload['entries'] as List<Object?>? ?? const [])) {
        if (entry is Map &&
            entry['terminalSessionId'] == 'private-managed-pty' &&
            entry['state'] is String) {
          // Keep only fixed state labels; never retain Hook payload or model
          // output in this explicit-only authenticated E2E harness.
          allForwardedScopesValid &=
              entry['workspaceId'] == 'private-workspace' &&
              entry['tabId'] == 'private-tab' &&
              entry['agentType'] == 'claude';
          states.add(entry['state'] as String);
        }
      }
    }
    return const <String, Object?>{};
  }

  Future<void> dispose() => _events.close();
}
