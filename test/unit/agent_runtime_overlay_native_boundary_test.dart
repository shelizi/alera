import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

const List<String> _overlayLaunchFiles = <String>[
  'lib/src/features/agent_status/infra/agent_runtime_overlay_prepare.dart',
  'lib/src/features/agent_status/infra/agent_runtime_overlay_service.dart',
  'lib/src/features/agent_status/infra/agent_runtime_overlay_shell.dart',
  'lib/src/features/agent_status/infra/agent_runtime_overlay_sources.dart',
  'lib/src/features/agent_status/infra/agent_runtime_overlay_wrappers.dart',
];

const List<String> _recursiveDartFilesystemPatterns = <String>[
  '.listSync(',
  '.copySync(',
  'deleteSync(recursive: true)',
  'list(recursive: true)',
  '_copyEntity(',
  '_mirrorSourceDirectory(',
  '_safeRemoveTree(',
];

void main() {
  test('agent overlay launch path keeps recursive filesystem work native', () {
    final offenders = <String>[];

    for (final path in _overlayLaunchFiles) {
      final source = File(path).readAsStringSync();
      for (final pattern in _recursiveDartFilesystemPatterns) {
        if (source.contains(pattern)) {
          offenders.add('$path contains $pattern');
        }
      }
    }

    expect(
      offenders,
      isEmpty,
      reason:
          'Agent runtime overlay launch preparation must stay behind the coarse '
          'native bridge. Recursive Dart filesystem traversal would put source '
          'tree work back on the UI isolate.\n${offenders.join('\n')}',
    );

    expect(
      File(
        'lib/src/features/agent_status/infra/agent_runtime_overlay_service.dart',
      ).readAsStringSync(),
      contains('native.prepareAgentRuntimeOverlay'),
    );
  });
}
