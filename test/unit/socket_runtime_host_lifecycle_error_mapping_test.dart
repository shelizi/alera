import 'package:alera/src/features/runtime_host/application/runtime_host_lifecycle_service.dart';
import 'package:alera/src/features/runtime_host/infra/socket_runtime_host_lifecycle_client.dart';
import 'package:alera/src/platform/runtime_host/runtime_host_transport_errors.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('runtime host lifecycle transport error mapping', () {
    test('maps request timeout to unknown outcome', () {
      const cause = TerminalHostRequestTimeoutException(
        'host.shutdown',
        Duration(seconds: 10),
      );

      final mapped = mapRuntimeHostLifecycleTransportError(cause);

      expect(
        mapped,
        isA<RuntimeHostLifecycleOutcomeUnknownException>().having(
          (error) => error.cause,
          'cause',
          same(cause),
        ),
      );
    });

    test('maps connection close to transport interruption', () {
      const cause = TerminalHostConnectionClosedException();

      final mapped = mapRuntimeHostLifecycleTransportError(cause);

      expect(
        mapped,
        isA<RuntimeHostLifecycleTransportException>().having(
          (error) => error.cause,
          'cause',
          same(cause),
        ),
      );
    });

    test('leaves unrelated failures unmapped', () {
      final cause = StateError('permission denied');

      expect(mapRuntimeHostLifecycleTransportError(cause), isNull);
    });
  });
}
