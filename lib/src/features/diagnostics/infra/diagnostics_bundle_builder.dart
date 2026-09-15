import 'dart:convert';
import 'dart:io';

import 'package:alera/src/features/diagnostics/domain/diagnostics_bundle_metadata.dart';
import 'package:alera/src/rust/api/diagnostics.dart' as native;

typedef DiagnosticsBundleNativeWriter = Future<void> Function({
  required String outputPath,
  required String metadataJson,
  String? appLogDirectory,
  String? runtimeLogDirectory,
});

/// Streams app logs, runtime logs and build metadata into one native ZIP file.
///
/// Rust owns the archive writer so log contents never have to be materialized as
/// one large Dart `List<int>` before the file is saved.
class const DiagnosticsBundleBuilder({
  this.nativeWriter = native.writeDiagnosticsBundle,
}) {
  final DiagnosticsBundleNativeWriter nativeWriter;

  /// Writes the archive directly to [outputPath]. Missing log directories are
  /// skipped by the native writer.
  Future<void> writeToFile({
    required String outputPath,
    required DiagnosticsBundleMetadata metadata,
    Directory? appLogDirectory,
    Directory? runtimeLogDirectory,
  }) {
    return nativeWriter(
      outputPath: outputPath,
      metadataJson: const JsonEncoder.withIndent('  ')
          .convert(metadata.toJson()),
      appLogDirectory: appLogDirectory?.path,
      runtimeLogDirectory: runtimeLogDirectory?.path,
    );
  }

  /// Default file name for the saved bundle.
  static String suggestedFileName(DateTime now) {
    final stamp = now
        .toUtc()
        .toIso8601String()
        .replaceAll(':', '')
        .replaceAll('-', '')
        .split('.')
        .first;
    return 'alera-diagnostics-$stamp.zip';
  }
}
