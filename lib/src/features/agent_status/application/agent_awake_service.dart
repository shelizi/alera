import 'dart:async';

import 'package:alera/src/features/agent_profiles/domain/agent_descriptor_registry.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:logging/logging.dart';

const Duration agentAwakeStatusStaleAfter = Duration(hours: 2);

abstract interface class AgentAwakeDisplayLock {
  Future<void> setEnabled(bool enabled);
}

abstract interface class AgentAwakeAssertion {
  Future<void> start(String reason);
  Future<void> stop(String reason);
  Future<void> dispose();
}

class AgentAwakeService({
  required final AgentAwakeDisplayLock displayLock,
  required List<AgentAwakeAssertion> assertions,
  DateTime Function()? now,
  final Duration statusStaleAfter = agentAwakeStatusStaleAfter,
  Logger? logger,
}) {
  this
    : _assertions = List<AgentAwakeAssertion>.unmodifiableOf(assertions),
      _now = now ?? (() => DateTime.now().toUtc()),
      _logger = logger ?? Logger('AgentAwakeService');

  final List<AgentAwakeAssertion> _assertions;
  final DateTime Function() _now;
  final Logger _logger;

  bool _enabled = false;
  bool _displayLockEnabled = false;
  bool _disposed = false;
  Future<void> _operationTail = Future<void>.value();
  Future<void>? _disposeFuture;
  Future<void>? _pendingRefresh;
  String? _pendingRefreshReason;
  bool _assertionsInactive = true;
  AgentStatusHookSettings _hookSettings = .defaults;
  List<AgentStatusEntry> _workingStatuses = const <AgentStatusEntry>[];
  Timer? _staleTimer;

  Future<void> setEnabled(bool enabled) async {
    if (_enabled == enabled) {
      return;
    }
    _enabled = enabled;
    await _refresh('settings-change');
  }

  Future<void> setHookSettings(AgentStatusHookSettings settings) async {
    if (_hookSettings == settings) {
      return;
    }
    _hookSettings = settings;
    if (_needsRefresh) {
      await _refresh('hook-settings-change');
    }
  }

  Future<void> setStatuses(Map<String, AgentStatusEntry> statuses) async {
    if (_disposed) return;
    // Retain a private snapshot of only the entries that can keep the
    // machine awake, including while disabled for a later settings change.
    _workingStatuses = [
      for (final entry in statuses.values)
        if (entry.state == AgentStatusState.working) entry,
    ];
    if (_needsRefresh) {
      await _refresh('status-change');
    }
  }

  bool get _needsRefresh =>
      _enabled || !_assertionsInactive || _displayLockEnabled;

  Future<void> dispose() async {
    final existingDispose = _disposeFuture;
    if (existingDispose != null) {
      return existingDispose;
    }
    _disposed = true;
    _clearStaleTimer();
    final disposeFuture = _enqueue(() async {
      await _setDisplayLock(false, 'dispose');
      for (final assertion in _assertions) {
        await _tryAssertion('dispose assertion', () => assertion.dispose());
      }
    });
    _disposeFuture = disposeFuture;
    await disposeFuture;
  }

  Future<void> _refresh(String reason) {
    if (_disposed) {
      return Future<void>.value();
    }
    // Refreshes read the latest settings and status snapshot when they run,
    // not the snapshot at submission time. Keep at most one queued refresh
    // behind the running operation instead of replaying every intermediate
    // status burst against the same final state.
    _pendingRefreshReason = reason;
    if (_pendingRefresh case final pending?) return pending;
    final pending = _enqueue(() async {
      final reason = _pendingRefreshReason!;
      _pendingRefresh = null;
      _pendingRefreshReason = null;
      await _performRefresh(reason);
    });
    _pendingRefresh = pending;
    return pending;
  }

  Future<void> _performRefresh(String reason) async {
    if (_disposed) {
      return;
    }
    final now = _now();
    final earliestExpiry = _enabled ? _earliestExpiry(now) : null;
    _scheduleStaleTimer(earliestExpiry, now);
    // The same scan determines both wake eligibility and the next expiry.
    if (earliestExpiry != null) {
      _assertionsInactive = false;
      await _setDisplayLock(true, reason);
      for (final assertion in _assertions) {
        await _tryAssertion('start assertion', () => assertion.start(reason));
      }
    } else {
      // There is nothing to stop before any start, or after a successful
      // stop. A failed stop remains unsettled and is retried on later input.
      if (!_assertionsInactive) {
        var stopped = true;
        for (final assertion in _assertions) {
          final success = await _tryAssertion(
            'stop assertion',
            () => assertion.stop(reason),
          );
          stopped = stopped && success;
        }
        _assertionsInactive = stopped;
      }
      await _setDisplayLock(false, reason);
    }
  }

  DateTime? _earliestExpiry(DateTime now) {
    DateTime? earliestExpiry;
    for (final entry in _workingStatuses) {
      if (!isAgentSettingEnabled(_hookSettings.values, entry.agentType)) {
        continue;
      }
      final expiry = entry.updatedAt.add(statusStaleAfter);
      // At the expiry instant the entry is stale. Treating it as eligible
      // while omitting its timer would leave the assertion active forever.
      if (!expiry.isAfter(now)) continue;
      if (earliestExpiry == null || expiry.isBefore(earliestExpiry)) {
        earliestExpiry = expiry;
      }
    }
    return earliestExpiry;
  }

  void _scheduleStaleTimer(DateTime? earliestExpiry, DateTime now) {
    _clearStaleTimer();
    if (earliestExpiry == null) {
      return;
    }
    _staleTimer = Timer(earliestExpiry.difference(now), () {
      _staleTimer = null;
      unawaited(_refresh('stale-expiry'));
    });
  }

  void _clearStaleTimer() {
    _staleTimer?.cancel();
    _staleTimer = null;
  }

  Future<void> _setDisplayLock(bool enabled, String reason) async {
    if (_displayLockEnabled == enabled) {
      return;
    }
    try {
      await displayLock.setEnabled(enabled);
      _displayLockEnabled = enabled;
    } catch (error, stackTrace) {
      _logger.warning(
        '[agent-awake] failed to ${enabled ? 'start' : 'stop'} display lock: $reason',
        error,
        stackTrace,
      );
    }
  }

  Future<bool> _tryAssertion(
    String description,
    Future<void> Function() action,
  ) async {
    try {
      await action();
      return true;
    } catch (error, stackTrace) {
      _logger.warning(
        '[agent-awake] failed to $description',
        error,
        stackTrace,
      );
      return false;
    }
  }

  Future<void> _enqueue(Future<void> Function() operation) {
    final result = _operationTail.then((_) => operation());
    _operationTail = result.then<void>(
      (_) {},
      onError: (Object _, StackTrace _) {},
    );
    return result;
  }
}
