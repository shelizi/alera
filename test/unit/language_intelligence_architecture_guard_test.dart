import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

void main() {
  test('generic editor surfaces do not branch on first-wave language ids', () {
    const genericEditorFiles = <String>[
      'lib/src/features/workbench/presentation/workspace_editor_surface.dart',
      'lib/src/features/workbench/presentation/workspace_editor_widgets.dart',
      'lib/src/features/workbench/presentation/workspace_editor_outline.dart',
      'lib/src/features/workbench/presentation/workspace_editor_language_intelligence.dart',
      'lib/src/features/workbench/presentation/workspace_editor_text_actions.dart',
      'lib/src/features/workbench/application/workspace_references_controller.dart',
      'lib/src/features/workbench/presentation/workspace_references_panel.dart',
      'lib/src/features/language_intelligence/application/language_intelligence_manager.dart',
      'lib/src/features/language_intelligence/application/language_server_session_manager.dart',
      'lib/src/features/language_intelligence/infra/code_forge_language_server_runtime.dart',
    ];
    final languageLiteral = RegExp(
      r'''['"](?:csharp|dart|python|rust|go|php|typescript|tsx|javascript|jsx)['"]''',
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

  test('language intelligence domain and application stay adapter-free', () {
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
        for (final forbidden in const <String>[
          'package:flutter_riverpod/',
          'package:code_forge/',
          "import 'dart:io'",
          'LspStdioConfig',
        ]) {
          if (source.contains(forbidden)) {
            leaks.add('${file.path}: $forbidden');
          }
        }
      }
    }

    expect(
      leaks,
      isEmpty,
      reason:
          'Language Intelligence domain/application code must stay directly '
          'constructable and adapter-neutral. Riverpod belongs at the '
          'composition root; CodeForge, dart:io, and LSP process details belong '
          'under infra.',
    );
  });
}
