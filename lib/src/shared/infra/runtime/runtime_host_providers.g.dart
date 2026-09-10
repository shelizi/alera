// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'runtime_host_providers.dart';

// **************************************************************************
// RiverpodGenerator
// **************************************************************************

// GENERATED CODE - DO NOT MODIFY BY HAND
// ignore_for_file: type=lint, type=warning
/// Owns the single local socket client shared by runtime RPC and terminal I/O.
///
/// Most features must depend on [runtimeHostClient] instead so the concrete
/// Workbench transport does not leak through the shared runtime API.

@ProviderFor(socketTerminalHostClient)
final socketTerminalHostClientProvider = SocketTerminalHostClientProvider._();

/// Owns the single local socket client shared by runtime RPC and terminal I/O.
///
/// Most features must depend on [runtimeHostClient] instead so the concrete
/// Workbench transport does not leak through the shared runtime API.

final class SocketTerminalHostClientProvider
    extends
        $FunctionalProvider<
          SocketTerminalHostClient,
          SocketTerminalHostClient,
          SocketTerminalHostClient
        >
    with $Provider<SocketTerminalHostClient> {
  /// Owns the single local socket client shared by runtime RPC and terminal I/O.
  ///
  /// Most features must depend on [runtimeHostClient] instead so the concrete
  /// Workbench transport does not leak through the shared runtime API.
  SocketTerminalHostClientProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'socketTerminalHostClientProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$socketTerminalHostClientHash();

  @$internal
  @override
  $ProviderElement<SocketTerminalHostClient> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  SocketTerminalHostClient create(Ref ref) {
    return socketTerminalHostClient(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(SocketTerminalHostClient value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<SocketTerminalHostClient>(value),
    );
  }
}

String _$socketTerminalHostClientHash() =>
    r'0b625bd3b8b1345621998aa3d455c421c36f8706';

/// Neutral runtime RPC view over the shared socket client.

@ProviderFor(runtimeHostClient)
final runtimeHostClientProvider = RuntimeHostClientProvider._();

/// Neutral runtime RPC view over the shared socket client.

final class RuntimeHostClientProvider
    extends
        $FunctionalProvider<
          RuntimeHostClient,
          RuntimeHostClient,
          RuntimeHostClient
        >
    with $Provider<RuntimeHostClient> {
  /// Neutral runtime RPC view over the shared socket client.
  RuntimeHostClientProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'runtimeHostClientProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$runtimeHostClientHash();

  @$internal
  @override
  $ProviderElement<RuntimeHostClient> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  RuntimeHostClient create(Ref ref) {
    return runtimeHostClient(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(RuntimeHostClient value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<RuntimeHostClient>(value),
    );
  }
}

String _$runtimeHostClientHash() => r'da55d4fe3cee069b25a56eb7619d4caed1cdf8cd';

/// Runtime capability view over the shared socket client.

@ProviderFor(runtimeHostCapabilityClient)
final runtimeHostCapabilityClientProvider =
    RuntimeHostCapabilityClientProvider._();

/// Runtime capability view over the shared socket client.

final class RuntimeHostCapabilityClientProvider
    extends
        $FunctionalProvider<
          RuntimeHostCapabilityClient,
          RuntimeHostCapabilityClient,
          RuntimeHostCapabilityClient
        >
    with $Provider<RuntimeHostCapabilityClient> {
  /// Runtime capability view over the shared socket client.
  RuntimeHostCapabilityClientProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'runtimeHostCapabilityClientProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$runtimeHostCapabilityClientHash();

  @$internal
  @override
  $ProviderElement<RuntimeHostCapabilityClient> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  RuntimeHostCapabilityClient create(Ref ref) {
    return runtimeHostCapabilityClient(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(RuntimeHostCapabilityClient value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<RuntimeHostCapabilityClient>(value),
    );
  }
}

String _$runtimeHostCapabilityClientHash() =>
    r'fae63709abdec501ab05ffef04c1659d50523598';

/// One coalescer for every runtime watcher, keyed by namespaced strings
/// (`tabs:<id>`, `workspaces:<id>`, `projects`, ...), so there is a single
/// place to instrument and tune how change events fan out into RPC.

@ProviderFor(runtimeChangeCoalescer)
final runtimeChangeCoalescerProvider = RuntimeChangeCoalescerProvider._();

/// One coalescer for every runtime watcher, keyed by namespaced strings
/// (`tabs:<id>`, `workspaces:<id>`, `projects`, ...), so there is a single
/// place to instrument and tune how change events fan out into RPC.

final class RuntimeChangeCoalescerProvider
    extends
        $FunctionalProvider<
          RuntimeChangeCoalescer,
          RuntimeChangeCoalescer,
          RuntimeChangeCoalescer
        >
    with $Provider<RuntimeChangeCoalescer> {
  /// One coalescer for every runtime watcher, keyed by namespaced strings
  /// (`tabs:<id>`, `workspaces:<id>`, `projects`, ...), so there is a single
  /// place to instrument and tune how change events fan out into RPC.
  RuntimeChangeCoalescerProvider._()
    : super(
        from: null,
        argument: null,
        retry: null,
        name: r'runtimeChangeCoalescerProvider',
        isAutoDispose: false,
        dependencies: null,
        $allTransitiveDependencies: null,
      );

  @override
  String debugGetCreateSourceHash() => _$runtimeChangeCoalescerHash();

  @$internal
  @override
  $ProviderElement<RuntimeChangeCoalescer> $createElement(
    $ProviderPointer pointer,
  ) => $ProviderElement(pointer);

  @override
  RuntimeChangeCoalescer create(Ref ref) {
    return runtimeChangeCoalescer(ref);
  }

  /// {@macro riverpod.override_with_value}
  Override overrideWithValue(RuntimeChangeCoalescer value) {
    return $ProviderOverride(
      origin: this,
      providerOverride: $SyncValueProvider<RuntimeChangeCoalescer>(value),
    );
  }
}

String _$runtimeChangeCoalescerHash() =>
    r'75b6461c1b37366d95491768ae3db2f2f8c75246';
