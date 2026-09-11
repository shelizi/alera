import 'package:alera/src/features/runtime_host/application/runtime_host_lifecycle_service.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_status.dart';
import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';

final class SocketRuntimeHostLifecycleClient(
  final SocketTerminalHostClient _client,
) implements RuntimeHostLifecycleClient {
  @override
  void beginAppQuit() => _client.beginAppQuit();

  @override
  void cancelAppQuit() => _client.cancelAppQuit();

  @override
  void commitAppQuit() => _client.dispose();

  @override
  Future<Map<String, Object?>?> probeRuntimeStatus() =>
      _translateTransportErrors(_client.probeRuntimeStatus);

  @override
  Future<RuntimeHostShutdownResult> shutdownRuntime({bool force = false}) =>
      _translateTransportErrors(() => _client.shutdownRuntime(force: force));

  @override
  Future<void> ensureStarted({required TerminalHostConfig config}) =>
      _translateTransportErrors(() => _client.ensureStarted(config: config));

  Future<T> _translateTransportErrors<T>(Future<T> Function() operation) async {
    try {
      return await operation();
    } catch (error, stackTrace) {
      final mapped = mapRuntimeHostLifecycleTransportError(error);
      if (mapped != null) {
        Error.throwWithStackTrace(mapped, stackTrace);
      }
      rethrow;
    }
  }
}

Exception? mapRuntimeHostLifecycleTransportError(Object error) {
  if (error is StateError &&
      error.message.contains('No live Alera runtime host')) {
    return RuntimeHostLifecycleStoppedException(error);
  }
  if (error is TerminalHostRequestTimeoutException) {
    return RuntimeHostLifecycleOutcomeUnknownException(error);
  }
  if (error is TerminalHostConnectionClosedException ||
      error is StateError &&
          error.message.contains('Terminal host connection closed')) {
    return RuntimeHostLifecycleTransportException(error);
  }
  if (error is TerminalHostStartupException) {
    return RuntimeHostLifecycleStartupException(error);
  }
  return null;
}
