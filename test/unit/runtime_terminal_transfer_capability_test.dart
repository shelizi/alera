import 'package:alera/src/features/workbench/infra/runtime_terminal_transfer_capability.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:flutter_test/flutter_test.dart';

class _CapabilityHost extends Fake implements RuntimeHostClient {
  _CapabilityHost(this.reply);

  final Object? reply;
  String? requestedVerb;

  @override
  Future<Object?> runtimeRequest(
    String type, [
    Map<String, Object?> payload = const <String, Object?>{},
    Duration? timeout,
  ]) async {
    requestedVerb = type;
    if (reply is Exception) throw reply as Exception;
    return reply;
  }
}

void main() {
  test(
    'Host-advertised T2 protocol enables transfer without OS filtering',
    () async {
      final client = _CapabilityHost(const <String, Object?>{
        'supported': true,
        'livePtyTransfer': true,
        'atomicLayouts': true,
      });
      expect(
        await RuntimeTerminalTransferCapability.supportsLiveTransfer(client),
        isTrue,
      );
      expect(client.requestedVerb, 'tab.transfer.capabilities');
    },
  );

  test(
    'old, incomplete or explicitly disabled hosts stay unavailable',
    () async {
      for (final reply in <Object?>[
        null,
        const <String, Object?>{},
        const <String, Object?>{
          'supported': false,
          'livePtyTransfer': true,
          'atomicLayouts': true,
        },
        const <String, Object?>{
          'supported': true,
          'livePtyTransfer': false,
          'atomicLayouts': true,
        },
        const <String, Object?>{
          'supported': true,
          'livePtyTransfer': true,
          'atomicLayouts': false,
        },
      ]) {
        expect(
          await RuntimeTerminalTransferCapability.supportsLiveTransfer(
            _CapabilityHost(reply),
          ),
          isFalse,
          reason: 'Host capabilities must be explicitly complete: $reply',
        );
      }
    },
  );

  test('disconnected host does not authorize a synthetic transfer', () async {
    expect(
      await RuntimeTerminalTransferCapability.supportsLiveTransfer(
        _CapabilityHost(StateError('Host unavailable')),
      ),
      isFalse,
    );
  });
}
