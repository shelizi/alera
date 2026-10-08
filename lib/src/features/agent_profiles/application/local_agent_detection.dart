import 'dart:async';

import 'package:alera/src/features/agent_profiles/domain/agent_profile_adapters.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/shared/infra/process/command_environment_resolver.dart';
import 'package:alera/src/shared/infra/process/command_path_probe.dart';
import 'package:alera/src/shared/infra/process/process_runner.dart';

/// Which of the spawnable agent adapters have their launch command on this
/// machine's PATH.
///
/// The probe uses the same environment a terminal launch gets
/// (login-shell-hydrated PATH on macOS/Linux), so a detected agent also
/// resolves when it is typed into a workspace shell.
class const LocalAgentDetection({
  required final CommandEnvironmentResolver commandEnvironmentResolver,
  required final ProcessRunner processRunner,
  final Map<String, String> executablePaths = const <String, String>{},
  final bool? isWindows,
  final bool Function(String path)? executableExists,
}) {
  Future<List<AgentType>> detectInstalled() async {
    final environment = await commandEnvironmentResolver.environment();
    final detected = <AgentType>[];
    for (final agentType in spawnableAgentProfileAdapters) {
      final executable = _resolvedCommand(agentType, environment);
      if (executable == null) {
        continue;
      }
      if (agentType == AgentType.agy &&
          !await _isHealthyAgy(executable, environment)) {
        continue;
      }
      detected.add(agentType);
    }
    return detected;
  }

  String? _resolvedCommand(
    AgentType agentType,
    Map<String, String> environment,
  ) {
    final configuredPath = executablePaths[agentType.key]?.trim();
    if (configuredPath != null && configuredPath.isNotEmpty) {
      return executablePathExists(
            configuredPath,
            isWindows: isWindows,
            executableExists: executableExists,
          )
          ? configuredPath
          : null;
    }
    final command = agentProfileDefaultCommands[agentType];
    if (command == null) {
      return null;
    }
    return resolveCommandOnPath(
      command,
      environment: environment,
      isWindows: isWindows,
      executableExists: executableExists,
    );
  }

  Future<bool> _isHealthyAgy(
    String executable,
    Map<String, String> environment,
  ) async {
    try {
      final result = await processRunner
          .run(executable, const <String>[
            '--version',
          ], environment: environment)
          .timeout(const Duration(seconds: 5));
      return result.exitCode == 0;
    } catch (_) {
      return false;
    }
  }
}
