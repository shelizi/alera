final class const TerminalHostRequestTimeoutException(
  final String requestType,
  final Duration duration,
) implements Exception {
  @override
  String toString() {
    return 'Terminal host request "$requestType" timed out after '
        '${duration.inMilliseconds} ms.';
  }
}

final class const TerminalHostConnectionClosedException([final Object? reason])
    implements Exception {
  @override
  String toString() {
    final reason = this.reason?.toString();
    if (reason == null || reason.isEmpty) {
      return 'Terminal host connection closed.';
    }
    return 'Terminal host connection closed: $reason';
  }
}

final class const TerminalHostStartupException(final Object? cause)
    implements Exception {
  @override
  String toString() => 'Terminal host did not start in time.';
}
