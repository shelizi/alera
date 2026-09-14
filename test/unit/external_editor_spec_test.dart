import 'package:alera/src/features/external_editor/domain/external_editor_launcher.dart';
import 'package:alera/src/features/external_editor/domain/external_editor_spec.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('registry covers every external editor kind exactly once', () {
    expect(externalEditorSpecs.keys.toSet(), ExternalEditorKind.values.toSet());
    for (final entry in externalEditorSpecs.entries) {
      expect(entry.value.kind, entry.key);
    }
  });
}
