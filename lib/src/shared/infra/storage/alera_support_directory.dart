import 'dart:io';

import 'package:path/path.dart' as p;
import 'package:path_provider/path_provider.dart';

/// Used only by desktop smoke builds with ALERA_E2E_SUPPORT_DIR explicitly set.
/// Production uses the system support directory and existing user settings.
Directory? aleraTestSupportDirectory(String configuredPath) {
  if (configuredPath.isEmpty) return null;
  if (!p.isAbsolute(configuredPath)) {
    throw ArgumentError.value(
      configuredPath,
      'ALERA_E2E_SUPPORT_DIR',
      'Desktop smoke data directory must be absolute',
    );
  }
  return Directory(p.normalize(configuredPath));
}

Future<Directory> aleraApplicationSupportDirectory() async {
  const testPath = String.fromEnvironment('ALERA_E2E_SUPPORT_DIR');
  return aleraTestSupportDirectory(testPath) ??
      await getApplicationSupportDirectory();
}
