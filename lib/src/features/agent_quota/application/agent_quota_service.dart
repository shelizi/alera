part of 'agent_quota_providers.dart';

class AgentQuotaService(
  final RuntimeProxyClient _client, [
  final RuntimeHostClient? _runtimeClient,
]) {
  final Map<String, AgentQuotaState> _cache = <String, AgentQuotaState>{};
  final Set<String> _forceRefreshHosts = <String>{};
  final Map<String, String> _pendingCodexResetCreditMutationIds =
      <String, String>{};
  Future<void> _cacheWrite = Future<void>.value();

  void requestForceRefresh(String hostId) {
    _forceRefreshHosts.add(hostId);
  }

  bool consumeForceRefresh(String hostId) {
    return _forceRefreshHosts.remove(hostId);
  }

  Future<AgentQuotaState> fetch({
    required String hostId,
    required SshTarget? target,
    required AgentQuotaHostSettings settings,
    bool forceRefresh = false,
  }) async {
    try {
      final payload = await _requestQuota(
        hostId: hostId,
        target: target,
        settings: settings,
        forceRefresh: forceRefresh,
      );
      return await _withCache(
        () => _storeFetched(
          hostId,
          _snapshotsFromPayload(payload),
          _boolMap(payload['environment']),
        ),
      );
    } catch (error) {
      return _withCache(() => _storeFetchError(hostId, error.toString()));
    }
  }

  /// Refetches one account. Rows absent from the reply stay put, including when
  /// an older host ignores the filter and returns every agent.
  Future<AgentQuotaState> refreshSnapshot({
    required String hostId,
    required SshTarget? target,
    required AgentQuotaHostSettings settings,
    required AgentQuotaProviderId provider,
    required String accountId,
  }) async {
    try {
      final payload = await _requestQuota(
        hostId: hostId,
        target: target,
        settings: settings,
        forceRefresh: true,
        provider: provider,
        accountId: accountId,
      );
      return await _withCache(
        () => _storeRefresh(
          hostId,
          _snapshotsFromPayload(payload),
          _boolMap(payload['environment']),
        ),
      );
    } catch (error) {
      return _withCache(
        () => _storeRefreshError(hostId, provider, accountId, error.toString()),
      );
    }
  }

  Future<Map<String, Object?>> _requestQuota({
    required String hostId,
    required SshTarget? target,
    required AgentQuotaHostSettings settings,
    required bool forceRefresh,
    AgentQuotaProviderId? provider,
    String? accountId,
  }) async {
    final environmentNames = <String>[
      settings.environment.kimiApiKey,
      settings.environment.zaiApiKey,
      settings.environment.zaiBaseUrl,
      settings.environment.minimaxApiKey,
      settings.environment.minimaxApiHost,
    ];
    final environmentValues = hostId == 'local' && _runtimeClient != null
        ? await _client.resolveMissingLocalEnvironment(environmentNames)
        : const <String, String>{};
    if (hostId == 'local' && _runtimeClient != null) {
      return _mapValue(
        await _runtimeClient.runtimeRequest(
          'agentQuota.snapshot',
          <String, Object?>{
            'forceRefresh': forceRefresh,
            'provider': ?provider?.name,
            'accountId': ?accountId,
            if (environmentValues.isNotEmpty)
              'environmentValues': environmentValues,
          },
          const Duration(seconds: 45),
        ),
      );
    }
    return _client.request(
      hostId: hostId,
      target: target,
      type: 'agentQuota.fetch',
      localEnvironmentNames: environmentNames,
      payload: _remoteQuotaPayload(
        settings: settings,
        provider: provider,
        accountId: accountId,
      ),
    );
  }

  Future<T> _withCache<T>(T Function() action) async {
    final previous = _cacheWrite;
    final done = Completer<void>();
    _cacheWrite = done.future;
    try {
      await previous;
    } catch (_) {
      // A failed writer must not wedge the next cache update.
    }
    try {
      return action();
    } finally {
      done.complete();
    }
  }

  AgentQuotaState _storeFetched(
    String hostId,
    List<AgentQuotaSnapshot> fresh,
    Map<String, bool> environment,
  ) {
    final previous = <String, AgentQuotaSnapshot>{
      for (final snapshot
          in _cache[hostId]?.snapshots ?? const <AgentQuotaSnapshot>[])
        snapshot.key: snapshot,
    };
    final state = AgentQuotaState(
      hostId: hostId,
      snapshots: <AgentQuotaSnapshot>[
        for (final snapshot in fresh)
          if (snapshot.status == AgentQuotaStatus.ok ||
              !previous.containsKey(snapshot.key))
            _markOldSnapshotStale(snapshot)
          else
            previous[snapshot.key]!.asStale(snapshot.error),
      ],
      environment: environment,
      fetchedAt: DateTime.now().toUtc(),
    );
    _cache[hostId] = state;
    return state;
  }

  AgentQuotaState _storeFetchError(String hostId, String error) {
    final previous = _cache[hostId];
    if (previous != null) {
      return AgentQuotaState(
        hostId: hostId,
        snapshots: <AgentQuotaSnapshot>[
          for (final snapshot in previous.snapshots) snapshot.asStale(error),
        ],
        environment: previous.environment,
        fetchedAt: previous.fetchedAt,
        error: error,
      );
    }
    return AgentQuotaState(
      hostId: hostId,
      snapshots: const <AgentQuotaSnapshot>[],
      environment: const <String, bool>{},
      fetchedAt: DateTime.now().toUtc(),
      error: error,
    );
  }

  AgentQuotaState _storeRefresh(
    String hostId,
    List<AgentQuotaSnapshot> fresh,
    Map<String, bool> environment,
  ) {
    final previous = _cache[hostId];
    final state = AgentQuotaState(
      hostId: hostId,
      snapshots: mergeAgentQuotaSnapshots(
        previous: previous?.snapshots ?? const <AgentQuotaSnapshot>[],
        fresh: fresh,
      ),
      environment: environment.isNotEmpty
          ? environment
          : previous?.environment ?? const <String, bool>{},
      fetchedAt: DateTime.now().toUtc(),
    );
    _cache[hostId] = state;
    return state;
  }

  AgentQuotaState _storeRefreshError(
    String hostId,
    AgentQuotaProviderId provider,
    String accountId,
    String error,
  ) {
    final previous = _cache[hostId];
    if (previous == null) {
      return AgentQuotaState(
        hostId: hostId,
        snapshots: const <AgentQuotaSnapshot>[],
        environment: const <String, bool>{},
        fetchedAt: DateTime.now().toUtc(),
        error: error,
      );
    }
    final state = AgentQuotaState(
      hostId: hostId,
      snapshots: <AgentQuotaSnapshot>[
        for (final snapshot in previous.snapshots)
          if (snapshot.provider == provider && snapshot.accountId == accountId)
            snapshot.asStale(error)
          else
            snapshot,
      ],
      environment: previous.environment,
      fetchedAt: previous.fetchedAt,
    );
    _cache[hostId] = state;
    return state;
  }

  Future<AgentQuotaSnapshot> fetchClaudeTui({
    required String hostId,
    required SshTarget? target,
    required String accountId,
    String? displayName,
  }) async {
    final payload = hostId == 'local' && _runtimeClient != null
        ? _mapValue(
            await _runtimeClient.runtimeRequest(
              'agentQuota.fetchClaudeTui',
              <String, Object?>{
                'accountId': accountId,
                if (displayName != null && displayName.trim().isNotEmpty)
                  'displayName': displayName.trim(),
              },
              const Duration(seconds: 60),
            ),
          )
        : await _client.request(
            hostId: hostId,
            target: target,
            type: 'agentQuota.fetchClaudeTui',
            payload: <String, Object?>{
              'accountId': accountId,
              if (displayName != null && displayName.trim().isNotEmpty)
                'displayName': displayName.trim(),
            },
            timeout: const Duration(seconds: 60),
          );
    final raw = payload['snapshot'];
    if (raw is! Map) {
      throw const FormatException('Claude TUI response missing snapshot.');
    }
    final snapshot = AgentQuotaSnapshot.tryFromJson(
      Map<String, Object?>.from(raw),
    );
    if (snapshot == null) {
      throw const FormatException(
        'Claude TUI response had an unknown provider.',
      );
    }
    final previous = _cache[hostId];
    if (previous != null) {
      final snapshots = <AgentQuotaSnapshot>[
        for (final item in previous.snapshots)
          if (item.key == snapshot.key) snapshot else item,
      ];
      if (!snapshots.any((item) => item.key == snapshot.key)) {
        snapshots.add(snapshot);
      }
      _cache[hostId] = AgentQuotaState(
        hostId: hostId,
        snapshots: snapshots,
        environment: previous.environment,
        fetchedAt: DateTime.now().toUtc(),
      );
    }
    return snapshot;
  }

  Future<CodexResetConsumeResult> consumeCodexResetCredit({
    required String hostId,
    required SshTarget? target,
    required String offerRevision,
  }) async {
    final clientMutationId = _pendingCodexResetCreditMutationIds.putIfAbsent(
      offerRevision,
      () => const Uuid().v4(),
    );
    final requestPayload = <String, Object?>{
      'offerRevision': offerRevision,
      'clientMutationId': clientMutationId,
    };
    final payload = hostId == 'local' && _runtimeClient != null
        ? _mapValue(
            await _runtimeClient.runtimeRequest(
              'agentQuota.consumeCodexResetCredit',
              requestPayload,
              const Duration(seconds: 45),
            ),
          )
        : await _client.request(
            hostId: hostId,
            target: target,
            type: 'agentQuota.consumeCodexResetCredit',
            payload: requestPayload,
            timeout: const Duration(seconds: 45),
          );
    final result = CodexResetConsumeResult.fromJson(payload);
    final previous = _cache[hostId];
    final snapshots = <AgentQuotaSnapshot>[
      for (final snapshot
          in previous?.snapshots ?? const <AgentQuotaSnapshot>[])
        if (snapshot.key == result.snapshot.key) result.snapshot else snapshot,
    ];
    if (!snapshots.any((snapshot) => snapshot.key == result.snapshot.key)) {
      snapshots.add(result.snapshot);
    }
    _cache[hostId] = AgentQuotaState(
      hostId: hostId,
      snapshots: snapshots,
      environment: previous?.environment ?? const <String, bool>{},
      fetchedAt: DateTime.now().toUtc(),
    );
    _pendingCodexResetCreditMutationIds.remove(offerRevision);
    return result;
  }
}

Map<String, Object?> _remoteQuotaPayload({
  required AgentQuotaHostSettings settings,
  AgentQuotaProviderId? provider,
  String? accountId,
}) {
  return <String, Object?>{
    'providers': <String>[
      if (provider != null)
        provider.name
      else
        for (final item in settings.enabledProviders) item.name,
    ],
    'accountId': ?accountId,
    'claudeDefaultEnabled': provider == AgentQuotaProviderId.claude
        ? accountId == 'default' && settings.claudeDefaultEnabled
        : settings.claudeDefaultEnabled,
    'claudeProfiles': <Map<String, String>>[
      for (final profile in settings.claudeProfiles)
        if (provider == null ||
            (provider == AgentQuotaProviderId.claude &&
                (accountId == null || profile.profile == accountId)))
          <String, String>{'alias': profile.alias, 'profile': profile.profile},
    ],
    'codexProfiles': [
      for (final profile in settings.codexProfiles)
        if (provider == null ||
            (provider == AgentQuotaProviderId.codex &&
                (accountId == null || profile.profile == accountId)))
          profile.toMap(),
    ],
    'allowCliFallback': false,
    'environmentNames': <String, String>{
      'kimiApiKey': settings.environment.kimiApiKey,
      'zaiApiKey': settings.environment.zaiApiKey,
      'zaiBaseUrl': settings.environment.zaiBaseUrl,
      'minimaxApiKey': settings.environment.minimaxApiKey,
      'minimaxApiHost': settings.environment.minimaxApiHost,
    },
  };
}

List<AgentQuotaSnapshot> _snapshotsFromPayload(Map<String, Object?> payload) {
  final fresh = <AgentQuotaSnapshot>[];
  for (final item in payload['snapshots'] as List? ?? const <Object?>[]) {
    if (item is! Map) continue;
    final snapshot = AgentQuotaSnapshot.tryFromJson(
      Map<String, Object?>.from(item),
    );
    if (snapshot != null) fresh.add(snapshot);
  }
  return fresh;
}

List<AgentQuotaSnapshot> mergeAgentQuotaSnapshots({
  required List<AgentQuotaSnapshot> previous,
  required List<AgentQuotaSnapshot> fresh,
}) {
  final freshByKey = <String, AgentQuotaSnapshot>{
    for (final snapshot in fresh) snapshot.key: snapshot,
  };
  final seen = <String>{};
  final merged = <AgentQuotaSnapshot>[];
  for (final snapshot in previous) {
    seen.add(snapshot.key);
    final update = freshByKey[snapshot.key];
    if (update == null) {
      merged.add(snapshot);
      continue;
    }
    merged.add(
      update.status == AgentQuotaStatus.ok
          ? _markOldSnapshotStale(update)
          : snapshot.asStale(update.error),
    );
  }
  for (final snapshot in fresh) {
    if (!seen.add(snapshot.key)) continue;
    merged.add(
      snapshot.status == AgentQuotaStatus.ok
          ? _markOldSnapshotStale(snapshot)
          : snapshot,
    );
  }
  return merged;
}

Map<String, Object?> _mapValue(Object? value) {
  if (value is Map<String, Object?>) {
    return value;
  }
  if (value is Map) {
    return Map<String, Object?>.from(value);
  }
  throw const FormatException('Agent quota response must be an object.');
}

AgentQuotaSnapshot _markOldSnapshotStale(AgentQuotaSnapshot snapshot) {
  if (DateTime.now().toUtc().difference(snapshot.updatedAt) >
      const Duration(minutes: 30)) {
    return snapshot.asStale(snapshot.error);
  }
  return snapshot;
}

Map<String, bool> _boolMap(Object? value) {
  if (value is! Map) {
    return const <String, bool>{};
  }
  return <String, bool>{
    for (final entry in value.entries)
      if (entry.key is String && entry.value is bool)
        entry.key as String: entry.value as bool,
  };
}
