import 'package:alera/src/features/agent_profiles/domain/agent_profile_adapters.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/shared/infra/process/command_environment_resolver.dart';
import 'package:alera/src/shared/infra/process/command_path_probe.dart';

/// Which of the spawnable agent adapters have their launch command on this
/// machine's PATH.
///
/// The probe uses the same environment a terminal launch gets
/// (login-shell-hydrated PATH on macOS/Linux), so a detected agent also
/// resolves when it is typed into a workspace shell.
class const LocalAgentDetection({
  required final CommandEnvironmentResolver commandEnvironmentResolver,
  final Map<String, String> executablePaths = const <String, String>{},
  final bool? isWindows,
  final bool Function(String path)? executableExists,
}) {
  Future<List<AgentType>> detectInstalled() async {
    final environment = await commandEnvironmentResolver.environment();
    return <AgentType>[
      for (final agentType in spawnableAgentProfileAdapters)
        if (_commandResolves(agentType, environment)) agentType,
    ];
  }

  bool _commandResolves(AgentType agentType, Map<String, String> environment) {
    final configuredPath = executablePaths[agentType.key]?.trim();
    if (configuredPath != null && configuredPath.isNotEmpty) {
      return executablePathExists(
        configuredPath,
        isWindows: isWindows,
        executableExists: executableExists,
      );
    }
    final command = agentProfileDefaultCommands[agentType];
    if (command == null) {
      return false;
    }
    return commandResolvesOnPath(
      command,
      environment: environment,
      isWindows: isWindows,
      executableExists: executableExists,
    );
  }
}
