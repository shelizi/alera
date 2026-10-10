import 'dart:async';
import 'dart:convert';

import 'package:alera/src/features/agent_quota/domain/agent_quota.dart';
import 'package:alera/src/features/agent_quota/infra/runtime_proxy_client.dart';
import 'package:alera/src/features/remote_hosts/application/ssh_target_providers.dart';
import 'package:alera/src/features/remote_hosts/domain/ssh_target.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/process/process_providers.dart';
import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';
import 'package:uuid/uuid.dart';

part 'agent_quota_service.dart';
part 'agent_quota_providers.g.dart';

const agentQuotaRefreshInterval = Duration(minutes: 15);

@Riverpod(keepAlive: true)
RuntimeProxyClient runtimeProxyClient(Ref ref) {
  return RuntimeProxyClient(
    processRunner: ref.watch(processRunnerProvider),
    environmentResolver: ref.watch(commandEnvironmentResolverProvider),
  );
}

@Riverpod(keepAlive: true)
AgentQuotaService agentQuotaService(Ref ref) {
  return AgentQuotaService(
    ref.watch(runtimeProxyClientProvider),
    ref.watch(runtimeHostClientProvider),
  );
}

@Riverpod(keepAlive: true, name: 'agentQuotaStateProvider')
class AgentQuotaSnapshotList extends _$AgentQuotaSnapshotList {
  var _epoch = 0;

  @override
  Future<AgentQuotaState> build() {
    ++_epoch;
    final hostId = ref.watch(
      workbenchControllerProvider.select(
        (state) => state.activeWorkspace?.hostId ?? 'local',
      ),
    );
    ref.watch(
      settingsControllerProvider.select(
        (settings) =>
            agentQuotaRequestKey(settings.agents.quotas.forHost(hostId)),
      ),
    );
    final settings = ref
        .read(settingsControllerProvider)
        .agents
        .quotas
        .forHost(hostId);
    final targets = ref.watch(sshTargetsProvider).value ?? const <SshTarget>[];
    final target = hostId == 'local'
        ? null
        : targets.where((candidate) => candidate.id == hostId).firstOrNull;
    final timer = Timer(agentQuotaRefreshInterval, () {
      if (ref.mounted) {
        ref.invalidateSelf();
      }
    });
    ref.onDispose(timer.cancel);
    final service = ref.watch(agentQuotaServiceProvider);
    return service.fetch(
      hostId: hostId,
      target: target,
      settings: settings,
      forceRefresh: service.consumeForceRefresh(hostId),
    );
  }

  /// Refetches one account and publishes it without reloading the other rows.
  Future<void> refreshSnapshot({
    required String hostId,
    required AgentQuotaProviderId provider,
    required String accountId,
  }) async {
    final epoch = _epoch;
    final settings = ref
        .read(settingsControllerProvider)
        .agents
        .quotas
        .forHost(hostId);
    final targets = ref.read(sshTargetsProvider).value ?? const <SshTarget>[];
    final target = hostId == 'local'
        ? null
        : targets.where((candidate) => candidate.id == hostId).firstOrNull;
    final next = await ref
        .read(agentQuotaServiceProvider)
        .refreshSnapshot(
          hostId: hostId,
          target: target,
          settings: settings,
          provider: provider,
          accountId: accountId,
        );
    if (!ref.mounted || epoch != _epoch) {
      return;
    }
    final activeHost =
        ref.read(workbenchControllerProvider).activeWorkspace?.hostId ??
        'local';
    if (activeHost != hostId) {
      return;
    }
    state = AsyncData(next);
  }
}

String agentQuotaRequestKey(AgentQuotaHostSettings settings) {
  final providers =
      settings.enabledProviders.map((provider) => provider.name).toList()
        ..sort();
  final profiles =
      settings.claudeProfiles
          .map(
            (profile) => <String, String>{
              'alias': profile.alias,
              'profile': profile.profile,
            },
          )
          .toList()
        ..sort((left, right) {
          final byProfile = left['profile']!.compareTo(right['profile']!);
          return byProfile != 0
              ? byProfile
              : left['alias']!.compareTo(right['alias']!);
        });
  return jsonEncode(<String, Object?>{
    'providers': providers,
    'claudeDefaultEnabled': settings.claudeDefaultEnabled,
    'claudeProfiles': profiles,
    'codexProfiles': [
      for (final profile in settings.codexProfiles) profile.toMap(),
    ],
    'environment': settings.environment.toJson(),
  });
}

extension<T> on Iterable<T> {
  T? get firstOrNull {
    final iterator = this.iterator;
    return iterator.moveNext() ? iterator.current : null;
  }
}
