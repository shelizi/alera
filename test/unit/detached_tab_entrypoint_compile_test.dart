import 'package:alera/main.dart' as app;
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('detached startup and the main entrypoint compile together', () {
    // Importing the real entrypoint checks that both staged startup phases
    // and their native/database initialization tear-offs have valid types.
    expect(app.main, isNotNull);
  });

  test('Windows shared terminal windows default on', () {
    expect(app.experimentalSharedTerminalTabs, isTrue);
  });

  test('shared engine startup does not recenter a restored main window', () {
    expect(app.sharedEngineMainWindowOptions.alignment, isNull);
  });

  test('Windows main window enables the shared Engine by default', () {
    expect(
      app.shouldUseSharedEngineTabsInvocation(const [], isWindows: true),
      isTrue,
    );
    expect(
      app.shouldUseSharedEngineTabsInvocation(const [
        '--alera-shared-engine-tabs',
      ], isWindows: true),
      isTrue,
    ); // Previous opt-in CLI remains compatible.
  });

  test('explicit legacy mode never initializes shared Engine', () {
    for (final args in <List<String>>[
      const ['--alera-legacy-detached-tabs'],
      const ['--alera-detached-tab', '5123', 'validchildtoken'],
      const ['--alera-shared-engine-tabs=1'],
      const ['--alera-shared-engine-tabs', '--extra'],
      const ['--extra', '--alera-shared-engine-tabs'],
    ]) {
      expect(
        app.shouldUseSharedEngineTabsInvocation(args, isWindows: true),
        isFalse,
        reason: 'Fail closed to the legacy process runner for $args',
      );
    }
  });

  test('other desktop platforms remain on their existing window model', () {
    for (final args in <List<String>>[
      const [],
      const ['--alera-shared-engine-tabs'],
    ]) {
      expect(
        app.shouldUseSharedEngineTabsInvocation(args, isWindows: false),
        isFalse,
      );
    }
  });
}
