import 'package:alera/main.dart' as app;
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('detached startup and the main entrypoint compile together', () {
    // Importing the real entrypoint checks that both staged startup phases
    // and their native/database initialization tear-offs have valid types.
    expect(app.main, isNotNull);
  });

  test('single-engine option must exactly match native runner invocation', () {
    expect(
      app.isExperimentalSharedEngineTabsInvocation(const [
        '--alera-shared-engine-tabs',
      ]),
      isTrue,
    );
    for (final arguments in <List<String>>[
      const [],
      const ['--alera-shared-engine-tabs=1'],
      const ['--alera-shared-engine-tabs', '--extra'],
      const ['--extra', '--alera-shared-engine-tabs'],
      const ['--alera-detached-tab', '5123', 'token'],
    ]) {
      expect(app.isExperimentalSharedEngineTabsInvocation(arguments), isFalse);
    }
  });
}
