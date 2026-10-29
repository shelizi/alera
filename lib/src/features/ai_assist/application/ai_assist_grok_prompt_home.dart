import 'dart:io';

import 'package:path/path.dart' as p;

/// Grok-only prompt-file environment policy. Reading Diff copies credentials
/// alone; ordinary calls also copy the small allowlist of configuration files.
Future<Map<String, String>> prepareGrokPromptFileEnvironment({
  required String promptDirectoryPath,
  required Map<String, String> environment,
  required bool authenticationOnly,
}) async {
  final isolatedHome = Directory(p.join(promptDirectoryPath, 'grok-home'));
  await isolatedHome.create();
  final configuredHome = environment['GROK_HOME']?.trim();
  final userHome = (environment['HOME'] ?? environment['USERPROFILE'])?.trim();
  final sourceHome = configuredHome != null && configuredHome.isNotEmpty
      ? configuredHome
      : userHome != null && userHome.isNotEmpty
      ? p.join(userHome, '.grok')
      : null;
  if (sourceHome != null) {
    final fileNames = authenticationOnly
        ? const <String>['auth.json']
        : const <String>[
            'auth.json',
            'config.toml',
            'managed_config.toml',
            'requirements.toml',
          ];
    for (final fileName in fileNames) {
      final source = File(p.join(sourceHome, fileName));
      if (await source.exists()) {
        await source.copy(p.join(isolatedHome.path, fileName));
      }
    }
  }
  return <String, String>{'GROK_HOME': isolatedHome.path};
}
