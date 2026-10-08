import 'package:alera/src/features/agent_profiles/application/local_agent_detection.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/shared/infra/process/process_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'local_agent_providers.g.dart';

@Riverpod(keepAlive: true)
LocalAgentDetection localAgentDetection(Ref ref) {
  final executablePaths = ref.watch(
    settingsControllerProvider.select(
      (settings) => settings.agents.agentExecutablePaths,
    ),
  );
  return LocalAgentDetection(
    commandEnvironmentResolver: ref.watch(commandEnvironmentResolverProvider),
    processRunner: ref.watch(processRunnerProvider),
    executablePaths: executablePaths,
  );
}

/// Agent CLIs found on this machine's PATH. Probed once per session and kept
/// alive so the workspace tab strip's new-tab menu does not rescan on every
/// open; `ref.invalidate(installedAgentClisProvider)` forces a fresh probe.
@Riverpod(keepAlive: true)
Future<List<AgentType>> installedAgentClis(Ref ref) {
  return ref.watch(localAgentDetectionProvider).detectInstalled();
}
