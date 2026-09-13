part of 'mobile_runtime_client.dart';

const String _runtimeSettingsRevisionConflictCode =
    'runtime_settings_revision_conflict';

mixin MobileRuntimeClientHostTools {
  bool get isConnectionUsable;
  int _nextHostToolOperationId = 1;
  int? _runtimeSettingsRevision;
  final Map<String, String> _pendingCodexResetCreditMutationIds =
      <String, String>{};

  Future<Map<String, Object?>> requestMap(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]);

  Future<void> _refreshCrashReportingRuntimeContext() async {
    try {
      final status = await requestMap('status.get');
      if (isConnectionUsable) CrashReporting.updateRuntimeContext(this, status);
    } on Object {
      // Version metadata must never make an otherwise compatible host unusable.
      CrashReporting.clearRuntimeContext(this);
    }
  }

  Future<PortableHostSettings> loadPortableSettings() async {
    final payload = await requestMap('mobile.runtimeSettings.get');
    _rememberRuntimeSettingsRevision(payload);
    return PortableHostSettings.fromJson(payload);
  }

  Future<PortableHostSettings> updatePortableSettings(
    Map<String, Object?> patch,
  ) async {
    final requestPayload =
        _runtimeSettingsRevision == null ||
            patch.containsKey('expectedRevision')
        ? patch
        : <String, Object?>{
            ...patch,
            'expectedRevision': _runtimeSettingsRevision,
          };
    try {
      final payload = await requestMap(
        'mobile.runtimeSettings.update',
        requestPayload,
      );
      _rememberRuntimeSettingsRevision(payload);
      return PortableHostSettings.fromJson(payload);
    } on MobileRuntimeConflictException catch (error) {
      if (error.code == _runtimeSettingsRevisionConflictCode) {
        try {
          final refreshed = await requestMap('mobile.runtimeSettings.get');
          _rememberRuntimeSettingsRevision(refreshed);
        } on Object catch (refreshError, refreshStackTrace) {
          Logger('MobileRuntimeClient').warning(
            'could not refresh portable settings after a revision conflict',
            refreshError,
            refreshStackTrace,
          );
        }
      }
      rethrow;
    }
  }

  void _rememberRuntimeSettingsRevision(Map<String, Object?> payload) {
    final revision = payload['revision'];
    _runtimeSettingsRevision = revision is num ? revision.toInt() : null;
  }

  Future<QuotaSnapshotState> fetchAgentQuotas({
    bool forceRefresh = false,
  }) async {
    final payload = await requestMap('agentQuota.snapshot', <String, Object?>{
      'forceRefresh': forceRefresh,
    }, const Duration(seconds: 45));
    return QuotaSnapshotState.fromJson(payload);
  }

  Future<QuotaSnapshot> fetchClaudeQuotaViaTui({
    required String accountId,
    String? displayName,
  }) async {
    final payload = await requestMap(
      'agentQuota.fetchClaudeTui',
      <String, Object?>{
        'accountId': accountId,
        if (displayName != null && displayName.trim().isNotEmpty)
          'displayName': displayName.trim(),
      },
      const Duration(seconds: 60),
    );
    final raw = payload['snapshot'];
    if (raw is! Map) {
      throw const FormatException('Claude TUI response missing snapshot.');
    }
    return QuotaSnapshot.fromJson(asJsonMap(raw));
  }

  Future<CodexResetConsumeResult> consumeCodexResetCredit(
    String offerRevision,
  ) async {
    final clientMutationId = _pendingCodexResetCreditMutationIds.putIfAbsent(
      offerRevision,
      _newClientMutationId,
    );
    final payload = await requestMap(
      'agentQuota.consumeCodexResetCredit',
      <String, Object?>{
        'offerRevision': offerRevision,
        'clientMutationId': clientMutationId,
      },
      const Duration(seconds: 45),
    );
    final result = CodexResetConsumeResult.fromJson(payload);
    _pendingCodexResetCreditMutationIds.remove(offerRevision);
    return result;
  }

  Future<CliRegistrationStatus> cliRegistrationStatus() async {
    final payload = await requestMap(
      'cliRegistration.status',
      const <String, Object?>{},
      const Duration(seconds: 30),
    );
    return CliRegistrationStatus.fromJson(payload);
  }

  Future<CliRegistrationStatus> installCliRegistration() async {
    final payload = await requestMap(
      'cliRegistration.install',
      const <String, Object?>{},
      const Duration(seconds: 30),
    );
    return CliRegistrationStatus.fromJson(payload);
  }

  Future<SkillInstallResult> installSkill({
    required String skill,
    required String runner,
    String? operationId,
  }) async {
    final effectiveOperationId =
        operationId ??
        '${DateTime.now().microsecondsSinceEpoch}-${_nextHostToolOperationId++}';
    final payload = await requestMap('agentSkill.install', <String, Object?>{
      'operationId': effectiveOperationId,
      'skill': skill,
      'runner': runner,
    }, const Duration(minutes: 10));
    return SkillInstallResult.fromJson(payload);
  }
}

String _newClientMutationId() {
  final random = Random.secure();
  final bytes = List<int>.generate(16, (_) => random.nextInt(256));
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  final hex = bytes
      .map((byte) => byte.toRadixString(16).padLeft(2, '0'))
      .join();
  return '${hex.substring(0, 8)}-${hex.substring(8, 12)}-'
      '${hex.substring(12, 16)}-${hex.substring(16, 20)}-'
      '${hex.substring(20)}';
}
