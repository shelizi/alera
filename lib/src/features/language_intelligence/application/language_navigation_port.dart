import '../domain/source_location.dart';

abstract interface class LanguageNavigationPort {
  Future<List<SourceLocation>> definition({
    required String path,
    required SourcePosition position,
  });

  Future<List<SourceLocation>> declaration({
    required String path,
    required SourcePosition position,
  });

  Future<List<SourceLocation>> typeDefinition({
    required String path,
    required SourcePosition position,
  });

  Future<List<SourceLocation>> implementation({
    required String path,
    required SourcePosition position,
  });

  Future<List<SourceLocation>> references({
    required String path,
    required SourcePosition position,
    bool includeDeclaration = true,
  });
}

/// A document has a semantic provider, but no ready server session exists yet
/// (still starting, failed to start, or restarting after a crash).
final class LanguageServerNotReadyException implements Exception {
  const LanguageServerNotReadyException({required this.providerId});

  final String providerId;

  @override
  String toString() => 'Language server $providerId is not ready.';
}

/// A navigation request the language server answered with a JSON-RPC error,
/// such as `-32601` from a server that does not implement the method.
final class LanguageServerRequestException implements Exception {
  const LanguageServerRequestException({
    required this.method,
    required this.code,
    required this.message,
  });

  final String method;
  final int? code;
  final String message;

  bool get methodNotSupported => code == -32601;

  @override
  String toString() => 'Language server error for $method: $message';
}
