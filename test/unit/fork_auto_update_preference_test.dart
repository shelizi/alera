import 'dart:io';

import 'package:alera/src/features/updater/application/fork_auto_update_preference.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('auto checks are opt-in and persist across app restarts', () async {
    final directory = await Directory.systemTemp.createTemp(
      'fork-update-pref-',
    );
    addTearDown(() => directory.delete(recursive: true));
    ProviderContainer container() => ProviderContainer(
      overrides: [
        forkUpdatePreferenceDirectoryProvider.overrideWithValue(
          () async => directory,
        ),
      ],
    );

    final initial = container();
    expect(
      await initial.read(forkAutoUpdatePreferenceProvider.future),
      isFalse,
    );
    await initial
        .read(forkAutoUpdatePreferenceProvider.notifier)
        .setEnabled(true);
    expect(
      initial.read(forkAutoUpdatePreferenceProvider).asData?.value,
      isTrue,
    );
    initial.dispose();

    final restarted = container();
    expect(
      await restarted.read(forkAutoUpdatePreferenceProvider.future),
      isTrue,
    );
    await restarted
        .read(forkAutoUpdatePreferenceProvider.notifier)
        .setEnabled(false);
    expect(
      restarted.read(forkAutoUpdatePreferenceProvider).asData?.value,
      isFalse,
    );
    restarted.dispose();

    final disabled = container();
    addTearDown(disabled.dispose);
    expect(
      await disabled.read(forkAutoUpdatePreferenceProvider.future),
      isFalse,
    );
  });

  test('unavailable preference storage fails closed', () async {
    final container = ProviderContainer(
      overrides: [
        forkUpdatePreferenceDirectoryProvider.overrideWithValue(
          () async => throw const FileSystemException('Unavailable'),
        ),
      ],
      retry: (_, _) => null,
    );
    addTearDown(container.dispose);
    await expectLater(
      container.read(forkAutoUpdatePreferenceProvider.future),
      throwsA(isA<FileSystemException>()),
    );
    expect(container.read(forkAutoUpdatePreferenceProvider).hasError, isTrue);
    expect(
      container.read(forkAutoUpdatePreferenceProvider).asData?.value == true,
      isFalse,
    );
  });
}
