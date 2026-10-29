import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';

/// Non-mutating feature negotiation: old hosts and partial T2 Host builds
/// must refuse cross-project drop targets rather than falling back to
/// dangerous tab.upsert + layout.upsert or close + recreate semantics.
final class RuntimeTerminalTransferCapability {
  const RuntimeTerminalTransferCapability._();

  static Future<bool> supportsLiveTransfer(RuntimeHostClient client) async {
    try {
      final response = await client.runtimeRequest(
        'tab.transfer.capabilities',
        const <String, Object?>{},
      );
      return response is Map &&
          response['supported'] == true &&
          response['livePtyTransfer'] == true &&
          response['atomicLayouts'] == true;
    } on Object {
      // An older sidecar lacks the verb; a disconnected/unavailable sidecar
      // must also keep drag/drop disabled.
      return false;
    }
  }
}
