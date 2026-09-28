import 'dart:io';

import 'package:alera/src/features/language_intelligence/domain/language_extension_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/infra/builtin_language_extensions.dart';
import 'package:alera/src/features/language_intelligence/infra/local_workspace_marker_files.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:path/path.dart' as p;

LanguageExtensionDescriptor _language(List<String> markers) =>
    LanguageExtensionDescriptor(
      id: LanguageId('sample'),
      displayName: 'Sample',
      workspaceMarkers: markers,
    );

void main() {
  group('workspace markers', () {
    test('match file names case-insensitively and extension patterns', () {
      final language = _language(<String>['Cargo.toml', '*.csproj']);

      expect(language.isWorkspaceMarker('cargo.TOML'), isTrue);
      expect(language.isWorkspaceMarker('App.CsProj'), isTrue);
      expect(language.isWorkspaceMarker('Cargo.lock'), isFalse);
      expect(language.isWorkspaceMarker('csproj'), isFalse);
    });

    test('reject paths and wildcards other than a leading extension', () {
      for (final marker in <String>['rust/Cargo.toml', r'a\b', '*', 'a*.x']) {
        expect(
          () => _language(<String>[marker]),
          throwsArgumentError,
          reason: marker,
        );
      }
    });

    test('every builtin language with a semantic server can be detected', () {
      final registry = createBuiltinLanguageExtensionRegistry();
      for (final language in registry.languages) {
        if (language.semanticProviderIds.isEmpty) continue;
        expect(language.workspaceMarkers, isNotEmpty, reason: '${language.id}');
      }
    });
  });

  group('LocalWorkspaceMarkerFiles', () {
    late Directory root;

    setUp(() => root = Directory.systemTemp.createTempSync('markers-'));
    tearDown(() => root.deleteSync(recursive: true));

    void touch(List<String> parts) {
      final file = File(p.joinAll(<String>[root.path, ...parts]));
      file.parent.createSync(recursive: true);
      file.writeAsStringSync('');
    }

    test('lists the root and one level of source directories', () async {
      touch(<String>['pubspec.yaml']);
      touch(<String>['rust', 'Cargo.toml']);
      touch(<String>['landing', 'src', 'deep.ts']);
      touch(<String>['.worktrees', 'go.mod']);
      touch(<String>['node_modules', 'package.json']);

      final names = await const LocalWorkspaceMarkerFiles().markerCandidates(
        root.path,
      );

      expect(names, <String>{'pubspec.yaml', 'Cargo.toml'});
    });
  });
}
