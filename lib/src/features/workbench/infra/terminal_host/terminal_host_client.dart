import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:isolate';
import 'dart:math';

import 'package:alera/src/features/runtime_host/domain/runtime_host_status.dart';
import 'package:alera/src/features/diagnostics/infra/crash_reporting.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
import 'package:alera/src/platform/runtime_host/transport/terminal_host_frame_codec.dart';
import 'package:alera/src/platform/runtime_host/transport/terminal_host_socket_isolate.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_process_launcher.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/logging/app_logger.dart';
import 'package:alera/src/shared/infra/logging/log_redaction.dart';
import 'package:ghostty_vte_flutter/ghostty_vte_flutter.dart';
import 'package:logging/logging.dart';
import 'package:path/path.dart' as p;
import 'package:alera/src/shared/infra/storage/alera_support_directory.dart';
import 'package:uuid/uuid.dart';

export 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client_models.dart';
export 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_process_launcher.dart';

part 'terminal_host_client_types.dart';
part 'terminal_host_client_requests.dart';
part 'terminal_host_client_request_routing.dart';
part 'terminal_host_client_connection_state.dart';
part 'terminal_host_client_message_dispatch.dart';
part 'terminal_host_client_capabilities.dart';
part 'terminal_host_client_connections.dart';
part 'terminal_host_client_guarded_requests.dart';
part 'terminal_host_client_terminal_requests.dart';
part 'terminal_host_client_lifecycle.dart';
part 'terminal_host_client_heartbeat.dart';
part 'terminal_host_client_session_events.dart';
part 'terminal_host_client_terminal_pulse.dart';
part 'terminal_host_client_socket_reader.dart';
part 'terminal_host_control_file.dart';

const String _runtimeSettingsRevisionConflictCode =
    'runtime_settings_revision_conflict';

final class SocketTerminalHostClient._(
  final TerminalHostProcessLauncher _launcher,
  final Future<Directory> Function() _applicationSupportDirectory,
  final Duration _startupTimeout,
  var TerminalHostConfig _config,
  this._heartbeatInterval,
  final Duration _heartbeatTimeout,
  final Duration _connectRetryCooldown,
) with
        _TerminalHostClientHeartbeat,
        _TerminalHostClientSessionEvents,
        _TerminalPulseHostClientSupport,
        _RuntimeHostCapabilitySupport,
        _GuardedRuntimeHostClientSupport
    implements
        TerminalHostClient,
        TerminalHostViewLeaseClient,
        TerminalPulseHostClient,
        RuntimeHostClient,
        RuntimeHostCapabilityClient {
  factory({
    TerminalHostProcessLauncher? launcher,
    Future<Directory> Function()? applicationSupportDirectory,
    Duration startupTimeout = const Duration(seconds: 8),
    TerminalHostConfig initialConfig = TerminalHostConfig.defaults,
    Duration heartbeatInterval = _terminalHostHeartbeatInterval,
    Duration heartbeatTimeout = _terminalHostHeartbeatTimeout,
    Duration connectRetryCooldown = const Duration(seconds: 2),
  }) {
    return SocketTerminalHostClient._(
      launcher ?? DefaultTerminalHostProcessLauncher(),
      applicationSupportDirectory ?? aleraApplicationSupportDirectory,
      startupTimeout,
      initialConfig,
      heartbeatInterval,
      heartbeatTimeout,
      connectRetryCooldown,
    );
  }

  @override
  final Duration _heartbeatInterval;

  final StreamController<TerminalHostEvent> _events =
      StreamController<TerminalHostEvent>.broadcast();
  final StreamController<RuntimeHostEvent> _runtimeEvents =
      StreamController<RuntimeHostEvent>.broadcast();
  final Map<int, _PendingHostRequest> _pending = <int, _PendingHostRequest>{};
  final Map<String, String> _pendingTerminateMutationIds = <String, String>{};

  Future<_TerminalHostConnection>? _terminalConnectionFuture;
  @override
  _TerminalHostConnection? _terminalConnection;
  StreamSubscription<Object?>? _terminalLineSub;
  Future<_TerminalHostConnection>? _runtimeConnectionFuture;
  @override
  _TerminalHostConnection? _runtimeConnection;
  StreamSubscription<Object?>? _runtimeLineSub;
  // Pacing between independent connect attempts: a wedged host accepts the
  // TCP handshake but never answers hello, so without a cooldown every
  // failed attempt would let the next caller open another doomed socket.
  int _connectFailureStreak = 0;
  DateTime? _connectRetryNotBefore;
  int _nextRequestId = 1;
  int? _runtimeSettingsRevision;
  @override
  bool _disposed = false;
  bool _appQuitInProgress = false;

  @override
  StreamController<TerminalHostEvent> get _globalEvents => _events;

  @override
  StreamController<RuntimeHostEvent> get _runtimeEventSink => _runtimeEvents;

  @override
  Stream<TerminalHostEvent> get events => _events.stream;

  @override
  Stream<RuntimeHostEvent> get runtimeEvents => _runtimeEvents.stream;

  @override
  Future<void> ensureStarted({required TerminalHostConfig config}) async {
    if (_disposed) {
      throw StateError('Terminal host client is disposed.');
    }
    _throwIfAppQuitInProgress();
    _config = config;
    await _terminalRequest('configure', config.toJson());
  }

  /// Stops application traffic while the quit gate probes and shuts down the
  /// runtime. Requests already in flight cannot be allowed to reach their
  /// normal timeout: the host may close the socket as part of shutdown, so
  /// they are completed as an expected connection close instead.
  void beginAppQuit() {
    if (_disposed || _appQuitInProgress) {
      return;
    }
    _appQuitInProgress = true;
    _stopHeartbeat();
    _failPendingRequests(const TerminalHostConnectionClosedException());
  }

  /// Reopens application traffic when the user cancels the quit gate.
  void cancelAppQuit() {
    if (!_disposed) {
      _appQuitInProgress = false;
    }
  }

  @override
  Future<void> configure(TerminalHostConfig config) async {
    if (_disposed) {
      throw StateError('Terminal host client is disposed.');
    }
    _throwIfAppQuitInProgress();
    _config = config;
    final pendingConnections = <Future<_TerminalHostConnection>>[];
    if (_terminalConnection case final connection?) {
      pendingConnections.add(Future<_TerminalHostConnection>.value(connection));
    } else if (_terminalConnectionFuture case final future?) {
      pendingConnections.add(future);
    }
    if (_runtimeConnection case final connection?) {
      pendingConnections.add(Future<_TerminalHostConnection>.value(connection));
    } else if (_runtimeConnectionFuture case final future?) {
      pendingConnections.add(future);
    }
    if (pendingConnections.isEmpty) {
      return;
    }
    final connections = <_TerminalHostConnection>{};
    for (final future in pendingConnections) {
      connections.add(await future);
    }
    await Future.wait(<Future<Object?>>[
      for (final connection in connections)
        _requestOnConnection(connection, 'configure', config.toJson()),
    ]);
  }

  @override
  Future<TerminalHostAttachment> createOrAttach({
    required String sessionId,
    required String workspaceId,
    required String tabId,
    required String workingDirectory,
    required GhosttyTerminalShellLaunch launch,
    required int cols,
    required int rows,
    int? resumeCursor,
  }) => _createOrAttachTerminal(
    this,
    sessionId: sessionId,
    workspaceId: workspaceId,
    tabId: tabId,
    workingDirectory: workingDirectory,
    launch: launch,
    cols: cols,
    rows: rows,
    resumeCursor: resumeCursor,
  );

  @override
  Future<TerminalHostAttachment> createOrAttachView({
    required String sessionId,
    required String workspaceId,
    required String tabId,
    required String workingDirectory,
    required GhosttyTerminalShellLaunch launch,
    required int cols,
    required int rows,
    required String viewLeaseId,
    int? resumeCursor,
  }) => _createOrAttachTerminal(
    this,
    sessionId: sessionId,
    workspaceId: workspaceId,
    tabId: tabId,
    workingDirectory: workingDirectory,
    launch: launch,
    cols: cols,
    rows: rows,
    resumeCursor: resumeCursor,
    viewLeaseId: viewLeaseId,
  );

  @override
  Future<TerminalHostAttachment> restart({
    required String sessionId,
    required String workspaceId,
    required String tabId,
    required String workingDirectory,
    required GhosttyTerminalShellLaunch launch,
    required int cols,
    required int rows,
  }) => _restartTerminal(
    this,
    sessionId: sessionId,
    workspaceId: workspaceId,
    tabId: tabId,
    workingDirectory: workingDirectory,
    launch: launch,
    cols: cols,
    rows: rows,
  );

  @override
  Future<void> write({
    required String sessionId,
    required List<int> bytes,
    bool deferredEnter = false,
  }) async {
    if (bytes.isEmpty && !deferredEnter) {
      return;
    }
    await _terminalRequest('write', <String, Object?>{
      'sessionId': sessionId,
      'dataBase64': encodeTerminalHostBytes(bytes),
      if (deferredEnter) 'deferredEnter': true,
    });
  }

  @override
  Future<void> resize({
    required String sessionId,
    required int cols,
    required int rows,
  }) async {
    await _terminalRequest('resize', <String, Object?>{
      'sessionId': sessionId,
      'cols': cols,
      'rows': rows,
    });
  }

  @override
  Future<bool> reclaimTerminal(String sessionId) async {
    final payload = await _terminalRequestMap(
      'terminal.reclaim',
      <String, Object?>{'sessionId': sessionId},
    );
    return payload['restored'] == true;
  }

  @override
  Future<Map<String, TerminalSessionDriver>> listTerminalDrivers() async {
    return TerminalSessionDriver.mapFromListPayload(
      await _terminalRequest('terminal.driver.list', const <String, Object?>{}),
    );
  }

  @override
  Future<List<String>> getTerminalRunningProcesses(String sessionId) async {
    final payload = await _terminalRequestMap(
      'terminal.runningProcesses',
      <String, Object?>{'sessionId': sessionId},
    );
    final processes = payload['processes'];
    if (processes is! List) {
      return const <String>[];
    }
    return processes
        .map((p) {
          if (p is Map) {
            final name = p['name'];
            if (name is String && name.isNotEmpty) {
              return name;
            }
          } else if (p is String && p.isNotEmpty) {
            return p;
          }
          return null;
        })
        .whereType<String>()
        .toList(growable: false);
  }

  @override
  Future<TerminalHostResume> setOutputPaused({
    required String sessionId,
    required bool paused,
  }) async {
    final payload = await _terminalRequestMap(
      'setOutputPaused',
      <String, Object?>{'sessionId': sessionId, 'paused': paused},
    );
    return TerminalHostResume.fromJson(payload);
  }

  @override
  Future<void> detach(String sessionId) async {
    await _terminalRequest('detach', <String, Object?>{'sessionId': sessionId});
  }

  @override
  Future<void> detachView(
    String sessionId, {
    required String viewLeaseId,
  }) async {
    await _terminalRequest('detach', <String, Object?>{
      'sessionId': sessionId,
      'viewLeaseId': viewLeaseId,
    });
  }

  @override
  Future<void> terminate(
    String sessionId, {
    String? expectedWorkspaceId,
    String? expectedTabId,
    String? expectedTransferMutationId,
  }) => _terminateWithView(
    sessionId,
    expectedWorkspaceId: expectedWorkspaceId,
    expectedTabId: expectedTabId,
    expectedTransferMutationId: expectedTransferMutationId,
  );

  @override
  Future<void> terminateView(
    String sessionId, {
    required String viewLeaseId,
    String? expectedWorkspaceId,
    String? expectedTabId,
    String? expectedTransferMutationId,
  }) => _terminateWithView(
    sessionId,
    viewLeaseId: viewLeaseId,
    expectedWorkspaceId: expectedWorkspaceId,
    expectedTabId: expectedTabId,
    expectedTransferMutationId: expectedTransferMutationId,
  );

  @override
  Future<Map<String, Object?>> _terminalRequestMap(
    String type,
    Map<String, Object?> payload, {
    Duration? timeout,
  }) async {
    return asTerminalHostMap(
      await _terminalRequest(type, payload, timeout: timeout),
      'response',
    );
  }

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) {
    return _runtimeRequest(type, payload, timeout);
  }

  @override
  Future<_TerminalHostConnection> _connectRuntime({
    bool requireOrchestration = false,
    bool launchIfMissing = true,
  }) => _connectRuntimeImpl(
    requireOrchestration: requireOrchestration,
    launchIfMissing: launchIfMissing,
  );

  @override
  Future<void> _sendHeartbeatRequest(_TerminalHostConnection connection) async {
    await _sendTerminalHostRequest(
      this,
      connection,
      'status.get',
      const <String, Object?>{},
      timeout: _heartbeatTimeout,
    );
  }

  @override
  void _handleConnectionClosed(
    _TerminalHostConnection connection, [
    Object? error,
    String origin = 'unspecified',
  ]) => _handleConnectionClosedImpl(connection, error, origin);

  @override
  void dispose() {
    if (_disposed) {
      return;
    }
    _disposed = true;
    _appQuitInProgress = true;
    _pendingTerminateMutationIds.clear();
    _stopHeartbeat();
    _closeSessionEvents();
    _failPendingRequests(const TerminalHostConnectionClosedException());
    final connections = <_TerminalHostConnection>{};
    if (_terminalConnection case final connection?) {
      connections.add(connection);
    }
    if (_runtimeConnection case final connection?) {
      connections.add(connection);
    }
    for (final connection in connections) {
      _handleConnectionClosed(connection);
    }
    unawaited(_events.close());
    unawaited(_runtimeEvents.close());
  }

  @override
  void _throwIfAppQuitInProgress() {
    if (_appQuitInProgress) {
      throw const TerminalHostConnectionClosedException();
    }
  }
}
