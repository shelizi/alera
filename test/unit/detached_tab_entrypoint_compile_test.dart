import 'package:alera/main.dart' as app;
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('detached startup and the main entrypoint compile together', () {
    // Importing the real entrypoint checks that both staged startup phases
    // and their native/database initialization tear-offs have valid types.
    expect(app.main, isNotNull);
  });
}
