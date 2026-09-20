import 'dart:async';

import '../domain/language_id.dart';
import '../domain/language_intelligence_settings.dart';
import '../domain/language_provider_descriptor.dart';
import '../domain/language_server_session_state.dart';
import 'language_provider_registry.dart';
import 'language_server_runtime.dart';

typedef LanguageServerDelay = Future<void> Function(Duration duration);

final class LanguageServerReadySession {
  const LanguageServerReadySession({
    required this.workspaceId,
    required this.provider,
    required this.generation,
    required this.session,
  });

  final String workspaceId;
  final LanguageProviderDescriptor provider;
  final int generation;
  final LanguageServerRuntimeSession session;
}

final class LanguageServerSessionManager {
  factory LanguageServerSessionManager({
    required LanguageExtensionRegistry registry,
    required LanguageServerRuntimePort runtime,
    Duration idleShutdownDelay = const Duration(seconds: 30),
    Duration restartBackoff = const Duration(seconds: 1),
    int maxRestartAttempts = 2,
    LanguageServerDelay delay = _defaultDelay,
  }) {
    if (maxRestartAttempts < 0) {
      throw ArgumentError.value(
        maxRestartAttempts,
        'maxRestartAttempts',
        'Restart attempts must not be negative.',
      );
    }
    return LanguageServerSessionManager._(
      registry: registry,
      runtime: runtime,
      idleShutdownDelay: idleShutdownDelay,
      restartBackoff: restartBackoff,
      maxRestartAttempts: maxRestartAttempts,
      delay: delay,
    );
  }

  LanguageServerSessionManager._({
    required this._registry,
    required this._runtime,
    required this._idleShutdownDelay,
    required this._restartBackoff,
    required this._maxRestartAttempts,
    required this._delay,
  });

  final LanguageExtensionRegistry _registry;
  final LanguageServerRuntimePort _runtime;
  final Duration _idleShutdownDelay;
  final Duration _restartBackoff;
  final int _maxRestartAttempts;
  final LanguageServerDelay _delay;
  final Map<_LanguageServerSessionKey, _LanguageServerSessionRecord> _sessions =
      <_LanguageServerSessionKey, _LanguageServerSessionRecord>{};

  Future<LanguageServerSessionSnapshot> attachDocument({
    required String workspaceId,
    required String workspaceRoot,
    required LanguageId language,
    required String documentId,
    required LanguageIntelligenceSettings settings,
    required LanguageServerTarget target,
    Map<String, String> environment = const <String, String>{},
  }) async {
    final activation = settings.forLanguage(language);
    final providerId = activation.semanticProviderId?.trim();
    if (!activation.enabled || providerId == null || providerId.isEmpty) {
      return LanguageServerSessionSnapshot.disabled(
        workspaceId: workspaceId,
        providerId: providerId ?? '',
      );
    }

    final registration = _registry.provider(providerId);
    if (registration == null) {
      throw StateError('Language provider $providerId is not registered.');
    }
    final provider = registration.descriptor;
    if (provider.kind != LanguageProviderKind.semanticServer) {
      throw StateError(
        'Language provider $providerId is not a semantic server.',
      );
    }
    if (!provider.languages.contains(language)) {
      throw StateError(
        'Language provider $providerId does not support $language.',
      );
    }

    final key = _LanguageServerSessionKey(workspaceId, providerId);
    final record = _sessions.putIfAbsent(
      key,
      () => _LanguageServerSessionRecord(
        key: key,
        workspaceRoot: workspaceRoot,
        provider: provider,
      ),
    );
    if (record.workspaceRoot != workspaceRoot) {
      throw StateError(
        'Workspace $workspaceId changed root while provider $providerId was active.',
      );
    }

    record
      ..activation = activation
      ..target = target
      ..environment = Map<String, String>.unmodifiable(environment)
      ..idleGeneration += 1;
    record.documents.add(documentId);
    if (record.state == LanguageServerSessionState.disabled) {
      record.state = LanguageServerSessionState.available;
      record.restartAttempts = 0;
    }

    await _ensureReady(record);
    return record.snapshot;
  }

  Future<void> releaseDocument({
    required String workspaceId,
    required String providerId,
    required String documentId,
  }) async {
    final record =
        _sessions[_LanguageServerSessionKey(workspaceId, providerId)];
    if (record == null) {
      return;
    }
    record.documents.remove(documentId);
    if (record.documents.isNotEmpty) {
      return;
    }
    if (record.session == null && record.startFuture == null) {
      return;
    }

    final idleGeneration = ++record.idleGeneration;
    unawaited(_stopAfterIdle(record, idleGeneration));
  }

  Future<void> disableProvider({
    required String workspaceId,
    required String providerId,
  }) async {
    final record =
        _sessions[_LanguageServerSessionKey(workspaceId, providerId)];
    if (record == null) {
      return;
    }
    record.documents.clear();
    record.idleGeneration += 1;
    record.restartGeneration += 1;
    record.restartAttempts = 0;
    await _stopRecord(record, finalState: LanguageServerSessionState.disabled);
  }

  LanguageServerSessionSnapshot snapshotFor(
    String workspaceId,
    String providerId,
  ) =>
      _sessions[_LanguageServerSessionKey(workspaceId, providerId)]?.snapshot ??
      LanguageServerSessionSnapshot.disabled(
        workspaceId: workspaceId,
        providerId: providerId,
      );

  LanguageServerReadySession? readySessionFor(
    String workspaceId,
    String providerId,
  ) {
    final record =
        _sessions[_LanguageServerSessionKey(workspaceId, providerId)];
    final session = record?.session;
    if (record == null ||
        session == null ||
        record.state != LanguageServerSessionState.ready) {
      return null;
    }
    return LanguageServerReadySession(
      workspaceId: workspaceId,
      provider: record.provider,
      generation: record.generation,
      session: session,
    );
  }

  Future<void> _ensureReady(_LanguageServerSessionRecord record) async {
    while (true) {
      if (record.state == LanguageServerSessionState.ready &&
          record.session != null) {
        return;
      }
      final inFlight = record.startFuture;
      if (inFlight != null) {
        await inFlight;
        if (record.state == LanguageServerSessionState.available) {
          continue;
        }
        return;
      }

      final future = _startRecord(record);
      record.startFuture = future;
      try {
        await future;
      } finally {
        if (identical(record.startFuture, future)) {
          record.startFuture = null;
        }
      }
      return;
    }
  }

  Future<void> _startRecord(_LanguageServerSessionRecord record) async {
    final activation = record.activation;
    final target = record.target;
    if (activation == null || target == null) {
      return;
    }

    final generation = ++record.generation;
    record
      ..state = LanguageServerSessionState.resolvingExecutable
      ..lastError = null;
    try {
      final resolution = await _runtime.resolveExecutable(
        provider: record.provider,
        settings: activation,
        target: target,
      );
      if (generation != record.generation ||
          record.state == LanguageServerSessionState.disabled) {
        return;
      }

      if (resolution is LanguageServerExecutableMissing) {
        record
          ..state = LanguageServerSessionState.missingExecutable
          ..lastError = resolution.reason
          ..executable = null;
        return;
      }
      final resolved = resolution as LanguageServerExecutableResolved;
      record
        ..executable = resolved.executable
        ..state = LanguageServerSessionState.starting;

      final session = await _runtime.start(
        LanguageServerRuntimeStartRequest(
          provider: record.provider,
          executable: resolved.executable,
          workspaceRoot: record.workspaceRoot,
          target: target,
          arguments: <String>[
            ...record.provider.defaultArguments,
            ...activation.extraArgs,
          ],
          environment: record.environment,
        ),
      );
      if (generation != record.generation ||
          record.state == LanguageServerSessionState.disabled) {
        await _runtime.stop(session);
        return;
      }

      record
        ..session = session
        ..state = LanguageServerSessionState.ready
        ..lastError = null;
      await record.exitSubscription?.cancel();
      record.exitSubscription = _runtime
          .observeExit(session)
          .listen(
            (exit) => unawaited(_handleExit(record, session, exit)),
            onError: (Object error, StackTrace stackTrace) => unawaited(
              _handleExit(record, session, LanguageServerExit(error: error)),
            ),
          );
    } catch (error) {
      if (generation != record.generation ||
          record.state == LanguageServerSessionState.disabled) {
        return;
      }
      record
        ..state = LanguageServerSessionState.failed
        ..lastError = error.toString();
      _scheduleRestartIfNeeded(record);
    }
  }

  Future<void> _stopAfterIdle(
    _LanguageServerSessionRecord record,
    int idleGeneration,
  ) async {
    await _delay(_idleShutdownDelay);
    if (idleGeneration != record.idleGeneration ||
        record.documents.isNotEmpty) {
      return;
    }
    await _stopRecord(record, finalState: LanguageServerSessionState.available);
    record.restartAttempts = 0;
  }

  Future<void> _stopRecord(
    _LanguageServerSessionRecord record, {
    required LanguageServerSessionState finalState,
  }) async {
    record.generation += 1;
    record.restartGeneration += 1;
    final session = record.session;
    record.session = null;
    await record.exitSubscription?.cancel();
    record.exitSubscription = null;
    if (session != null) {
      record.state = LanguageServerSessionState.stopping;
      try {
        await _runtime.stop(session);
      } catch (error) {
        record.lastError = error.toString();
      }
    }
    record.state = finalState;
  }

  Future<void> _handleExit(
    _LanguageServerSessionRecord record,
    LanguageServerRuntimeSession session,
    LanguageServerExit exit,
  ) async {
    if (!identical(record.session, session) ||
        record.state == LanguageServerSessionState.stopping ||
        record.state == LanguageServerSessionState.disabled) {
      return;
    }
    record
      ..session = null
      ..generation += 1
      ..state = LanguageServerSessionState.failed
      ..lastError = _describeExit(exit);
    await record.exitSubscription?.cancel();
    record.exitSubscription = null;
    _scheduleRestartIfNeeded(record);
  }

  void _scheduleRestartIfNeeded(_LanguageServerSessionRecord record) {
    if (record.documents.isEmpty ||
        record.restartAttempts >= _maxRestartAttempts) {
      return;
    }
    record.restartAttempts += 1;
    final restartGeneration = ++record.restartGeneration;
    unawaited(_restartAfterBackoff(record, restartGeneration));
  }

  Future<void> _restartAfterBackoff(
    _LanguageServerSessionRecord record,
    int restartGeneration,
  ) async {
    await _delay(_scaleDuration(_restartBackoff, record.restartAttempts));
    if (restartGeneration != record.restartGeneration ||
        record.documents.isEmpty ||
        record.state != LanguageServerSessionState.failed) {
      return;
    }
    await _ensureReady(record);
  }

  static String _describeExit(LanguageServerExit exit) {
    if (exit.error != null) {
      return 'Language server failed: ${exit.error}';
    }
    if (exit.exitCode != null) {
      return 'Language server exited with code ${exit.exitCode}.';
    }
    return 'Language server exited.';
  }

  static Duration _scaleDuration(Duration duration, int multiplier) =>
      Duration(microseconds: duration.inMicroseconds * multiplier);

  static Future<void> _defaultDelay(Duration duration) =>
      Future<void>.delayed(duration);
}

final class _LanguageServerSessionKey {
  const _LanguageServerSessionKey(this.workspaceId, this.providerId);

  final String workspaceId;
  final String providerId;

  @override
  bool operator ==(Object other) =>
      other is _LanguageServerSessionKey &&
      other.workspaceId == workspaceId &&
      other.providerId == providerId;

  @override
  int get hashCode => Object.hash(workspaceId, providerId);
}

final class _LanguageServerSessionRecord {
  _LanguageServerSessionRecord({
    required this.key,
    required this.workspaceRoot,
    required this.provider,
  });

  final _LanguageServerSessionKey key;
  final String workspaceRoot;
  final LanguageProviderDescriptor provider;
  final Set<String> documents = <String>{};
  LanguageServerSessionState state = LanguageServerSessionState.available;
  LanguageActivationSettings? activation;
  LanguageServerTarget? target;
  Map<String, String> environment = const <String, String>{};
  LanguageServerRuntimeSession? session;
  StreamSubscription<LanguageServerExit>? exitSubscription;
  Future<void>? startFuture;
  int generation = 0;
  int idleGeneration = 0;
  int restartGeneration = 0;
  int restartAttempts = 0;
  String? executable;
  String? lastError;

  LanguageServerSessionSnapshot get snapshot => LanguageServerSessionSnapshot(
    workspaceId: key.workspaceId,
    providerId: key.providerId,
    state: state,
    activeDocumentCount: documents.length,
    generation: generation,
    restartAttempts: restartAttempts,
    executable: executable,
    lastError: lastError,
  );
}
