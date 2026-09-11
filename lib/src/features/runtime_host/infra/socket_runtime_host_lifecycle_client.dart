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
      if (error is StateError &&
          error.message.contains('No live Alera runtime host')) {
        Error.throwWithStackTrace(
          RuntimeHostLifecycleStoppedException(error),
          stackTrace,
        );
      }
      if (error is TerminalHostConnectionClosedException ||
          error is TerminalHostRequestTimeoutException ||
          error is StateError &&
              error.message.contains('Terminal host connection closed')) {
        Error.throwWithStackTrace(
          RuntimeHostLifecycleTransportException(error),
          stackTrace,
        );
      }
      if (error is TerminalHostStartupException) {
        Error.throwWithStackTrace(
          RuntimeHostLifecycleStartupException(error),
          stackTrace,
        );
      }
      rethrow;
    }
  }
}
