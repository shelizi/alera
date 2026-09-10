import 'package:alera/src/features/workbench/infra/terminal_host/terminal_host_client.dart';
import 'package:alera/src/platform/runtime_host/protocol/terminal_host_protocol.dart';
import 'package:alera/src/shared/infra/runtime/runtime_change_coalescer.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'runtime_host_providers.g.dart';

/// Owns the single local socket client shared by runtime RPC and terminal I/O.
///
/// Most features must depend on [runtimeHostClient] instead so the concrete
/// Workbench transport does not leak through the shared runtime API.
@Riverpod(keepAlive: true)
SocketTerminalHostClient socketTerminalHostClient(Ref ref) {
  final client = SocketTerminalHostClient();
  ref.onDispose(client.dispose);
  return client;
}

/// Neutral runtime RPC view over the shared socket client.
@Riverpod(keepAlive: true)
RuntimeHostClient runtimeHostClient(Ref ref) {
  return ref.watch(socketTerminalHostClientProvider);
}

/// Runtime capability view over the shared socket client.
@Riverpod(keepAlive: true)
RuntimeHostCapabilityClient runtimeHostCapabilityClient(Ref ref) {
  return ref.watch(socketTerminalHostClientProvider);
}

/// One coalescer for every runtime watcher, keyed by namespaced strings
/// (`tabs:<id>`, `workspaces:<id>`, `projects`, ...), so there is a single
/// place to instrument and tune how change events fan out into RPC.
@Riverpod(keepAlive: true)
RuntimeChangeCoalescer runtimeChangeCoalescer(Ref ref) {
  final coalescer = RuntimeChangeCoalescer();
  ref.onDispose(coalescer.dispose);
  return coalescer;
}
