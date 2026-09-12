import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'workbench controller parts and owners do not compose Riverpod providers '
    'directly',
    () {
      final files = Directory('lib/src/features/workbench/application')
          .listSync()
          .whereType<File>()
          .where((file) {
            final name = file.uri.pathSegments.last;
            final isControllerPart = name.startsWith('workbench_controller');
            final isOwner =
                name.startsWith('workbench_') && name.contains('_owner');
            return (isControllerPart || isOwner) &&
                name.endsWith('.dart') &&
                name != 'workbench_controller_internals.dart' &&
                name != 'workbench_controller.g.dart';
          });

      final leaks = <String>[];
      final providerPattern = RegExp(r'\b[A-Za-z0-9_]+Provider\b');
      for (final file in files) {
        final source = file.readAsStringSync();
        for (final match in providerPattern.allMatches(source)) {
          leaks.add('${file.path}: ${match.group(0)}');
        }
      }

      expect(
        leaks,
        isEmpty,
        reason:
            'Controller parts and owners must use narrow capabilities '
            'composed in workbench_controller_internals.dart instead of '
            'reading providers.',
      );
    },
  );
}
