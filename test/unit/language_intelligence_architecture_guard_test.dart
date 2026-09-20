import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('generic editor surfaces do not branch on first-wave language ids', () {
    const genericEditorFiles = <String>[
      'lib/src/features/workbench/presentation/workspace_editor_surface.dart',
      'lib/src/features/workbench/presentation/workspace_editor_widgets.dart',
      'lib/src/features/workbench/presentation/workspace_editor_outline.dart',
    ];
    final languageLiteral = RegExp(
      r'''['"](?:csharp|python|rust|go|php|typescript|tsx|javascript|jsx)['"]''',
    );

    final leaks = <String>[];
    for (final path in genericEditorFiles) {
      final source = File(path).readAsStringSync();
      for (final match in languageLiteral.allMatches(source)) {
        leaks.add('$path: ${match.group(0)}');
      }
    }

    expect(
      leaks,
      isEmpty,
      reason:
          'Generic editor code must use language-intelligence descriptors and '
          'capabilities instead of branching on concrete language ids. The '
          'legacy syntax-only workspace_editor_language_registry.dart remains '
          'outside this guard until the explicit parser activation migration.',
    );
  });

  test('language intelligence domain and application stay Riverpod-free', () {
    final roots = <Directory>[
      Directory('lib/src/features/language_intelligence/domain'),
      Directory('lib/src/features/language_intelligence/application'),
    ];
    final leaks = <String>[];

    for (final root in roots) {
      if (!root.existsSync()) {
        continue;
      }
      for (final file in root.listSync(recursive: true).whereType<File>()) {
        if (!file.path.endsWith('.dart')) {
          continue;
        }
        final source = file.readAsStringSync();
        if (source.contains('package:flutter_riverpod/')) {
          leaks.add('${file.path}: flutter_riverpod import');
        }
      }
    }

    expect(
      leaks,
      isEmpty,
      reason:
          'Language Intelligence domain/application code must be directly '
          'constructable; Riverpod belongs at the composition root.',
    );
  });
}
