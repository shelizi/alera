import 'package:alera/src/features/agent_profiles/application/local_agent_detection.dart';
import 'package:alera/src/features/agent_profiles/domain/agent_profile_adapters.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/shared/infra/process/command_environment_resolver.dart';
import 'package:flutter_test/flutter_test.dart';

class _FakeEnvironmentResolver implements CommandEnvironmentResolver {
  _FakeEnvironmentResolver(this._environment);

  final Map<String, String> _environment;

  @override
  Future<Map<String, String>> environment() async => _environment;

  @override
  Future<Map<String, String>> environmentVariables(List<String> names) async =>
      const <String, String>{};
}

LocalAgentDetection _detection({
  required Map<String, String> environment,
  required bool isWindows,
  required Set<String> existing,
  Map<String, String> executablePaths = const <String, String>{},
}) {
  return LocalAgentDetection(
    commandEnvironmentResolver: _FakeEnvironmentResolver(environment),
    executablePaths: executablePaths,
    isWindows: isWindows,
    executableExists: existing.contains,
  );
}

void main() {
  group('LocalAgentDetection', () {
    test('returns adapters whose default command is on PATH', () async {
      final detection = _detection(
        environment: <String, String>{'PATH': '/usr/bin:/opt/agents'},
        isWindows: false,
        existing: <String>{'/usr/bin/codex', '/opt/agents/claude'},
      );

      expect(await detection.detectInstalled(), <AgentType>[
        AgentType.codex,
        AgentType.claude,
      ]);
    });

    test('keeps adapter order rather than PATH order', () async {
      final detection = _detection(
        environment: <String, String>{'PATH': '/usr/bin'},
        isWindows: false,
        existing: <String>{'/usr/bin/claude', '/usr/bin/codex'},
      );

      expect(await detection.detectInstalled(), <AgentType>[
        AgentType.codex,
        AgentType.claude,
      ]);
    });

    test('resolves PATHEXT shims on Windows', () async {
      final detection = _detection(
        environment: <String, String>{
          'Path': r'C:\tools',
          'PATHEXT': '.COM;.EXE;.BAT;.CMD',
        },
        isWindows: true,
        existing: <String>{
          r'C:\tools\claude.cmd',
          r'C:\tools\cursor-agent.exe',
        },
      );

      expect(await detection.detectInstalled(), <AgentType>[
        AgentType.claude,
        AgentType.cursor,
      ]);
    });

    test(
      'detects an agent from its configured executable outside PATH',
      () async {
        final detection = _detection(
          environment: <String, String>{'Path': r'C:\tools'},
          isWindows: true,
          existing: <String>{r'D:\Agents\Devin\devin.exe'},
          executablePaths: <String, String>{
            'devin': r'D:\Agents\Devin\devin.exe',
          },
        );

        expect(await detection.detectInstalled(), contains(AgentType.devin));
      },
    );

    test(
      'an explicit invalid executable override does not fall back to PATH',
      () async {
        final detection = _detection(
          environment: <String, String>{'Path': r'C:\tools'},
          isWindows: true,
          existing: <String>{r'C:\tools\devin.exe'},
          executablePaths: <String, String>{'devin': r'D:\missing\devin.exe'},
        );

        expect(
          await detection.detectInstalled(),
          isNot(contains(AgentType.devin)),
        );
      },
    );

    test('reports nothing when PATH is missing', () async {
      final detection = _detection(
        environment: const <String, String>{},
        isWindows: false,
        existing: <String>{'/usr/bin/claude'},
      );

      expect(await detection.detectInstalled(), isEmpty);
    });

    test('skips adapters without a resolvable command', () async {
      final detection = _detection(
        environment: <String, String>{'PATH': '/usr/bin'},
        isWindows: false,
        existing: const <String>{},
      );

      expect(await detection.detectInstalled(), isEmpty);
      expect(
        spawnableAgentProfileAdapters.every(
          agentProfileDefaultCommands.containsKey,
        ),
        isTrue,
      );
    });
  });
}
