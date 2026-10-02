import 'dart:async';

import 'package:alera/src/features/agent_quota/application/agent_account_launch_environment.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';

import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/agent_status/application/agent_status_controller.dart';
import 'package:alera/src/features/agent_status/application/agent_status_providers.dart';
import 'package:alera/src/features/app_window/application/app_window_providers.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/terminal_launch_environment.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_pty_session.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime.dart';
import 'package:alera/src/shared/infra/uri/uri_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'terminal_runtime_providers.g.dart';

@Riverpod(keepAlive: true)
TerminalRuntime terminalRuntime(Ref ref) {
  final terminalHostClient = ref.watch(terminalHostClientProvider);
  final agentRuntimeOverlay = ref.watch(agentRuntimeOverlayServiceProvider);
  final aleraCliShim = ref.watch(aleraCliTerminalShimServiceProvider);
  final shellStartupPreparer = ref.watch(terminalShellStartupPreparerProvider);
  final runtime = XtermTerminalRuntime(
    parserWorkerEnabled: true,
    ptySessionFactory: TerminalHostPtySessionFactory(
      client: terminalHostClient,
    ),
    initialSettings: ref.read(settingsControllerProvider).terminal,
    shellLaunchesBuilder: () => terminalShellLaunches(
      powerShell7ExecutablePath: ref
          .read(settingsControllerProvider)
          .terminal
          .powerShell7ExecutablePath,
    ),
    externalUriLauncher: ref.watch(externalUriLauncherProvider),
    shellStartupPreparer: shellStartupPreparer,
    terminalSessionCleanup: (terminalSessionId) {
      // A terminal closed mid-turn never emits the Codex Stop hook, so the
      // transcript watch has to be dropped here or its file poller outlives
      // the session.
      ref
          .read(agentHookReceiverProvider)
          .clearTerminalSession(terminalSessionId);
      return agentRuntimeOverlay.clearTerminalOverlays(terminalSessionId);
    },
    terminalProcessCreated: (terminalSessionId) => ref
        .read(agentStatusControllerProvider.notifier)
        .clearTerminal(terminalSessionId),
    interactionNotice: (message, {error = false}) {
      AleraToast.publish(
        message: message,
        tone: error ? AleraToastTone.error : AleraToastTone.info,
        duration: error
            ? const Duration(seconds: 6)
            : const Duration(seconds: 12),
      );
    },
    agentHookEnvironmentBuilder:
        ({required terminalSessionId, required workspaceId, required tabId}) {
          final workbench = ref.read(workbenchControllerProvider);
          final workspace = workbench.workspacesByProject.values
              .expand((entries) => entries)
              .where((entry) => entry.id == workspaceId)
              .firstOrNull;
          final environment = agentAccountLaunchEnvironment(
            ref
                .read(settingsControllerProvider)
                .agents
                .quotas
                .forHost(workspace?.hostId ?? 'local'),
          );
          Future<void> addAleraCliShim() async {
            try {
              mergeTerminalLaunchEnvironment(
                environment,
                await aleraCliShim.prepareForTerminalLaunch(),
              );
            } catch (_) {}
          }

          return addAleraCliShim().then(
            (_) => environment.isEmpty ? null : environment,
          );
        },
  );
  ref.listen<TerminalSettings>(
    settingsControllerProvider.select((settings) => settings.terminal),
    (_, next) => runtime.updateSettings(next),
  );
  final foreground = ref.watch(appForegroundProvider);
  runtime.setAppForeground(foreground.isForeground);
  final foregroundSub = foreground.changes.listen(runtime.setAppForeground);
  ref.onDispose(() {
    unawaited(foregroundSub.cancel());
    runtime.dispose();
  });
  return runtime;
}
