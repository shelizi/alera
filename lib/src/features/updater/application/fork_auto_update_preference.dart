import 'dart:io';

import 'package:alera/src/shared/infra/storage/alera_support_directory.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:path/path.dart' as p;

/// Local preference, separate from shared runtime/workspace configuration.
/// The marker is outside the installed application bundle.
final forkUpdatePreferenceDirectoryProvider =
    Provider<Future<Directory> Function()>(
      (ref) => aleraApplicationSupportDirectory,
    );

final forkAutoUpdatePreferenceProvider =
    AsyncNotifierProvider<ForkAutoUpdatePreference, bool>(
      ForkAutoUpdatePreference.new,
    );

class ForkAutoUpdatePreference extends AsyncNotifier<bool> {
  Future<File> _marker() async {
    final directory = await ref.read(forkUpdatePreferenceDirectoryProvider)();
    return File(p.join(directory.path, 'fork-classic-auto-update-enabled'));
  }

  @override
  Future<bool> build() async => (await _marker()).exists();

  Future<void> setEnabled(bool enabled) async {
    if (state.isLoading) return;
    state = const AsyncLoading();
    state = await AsyncValue.guard(() async {
      final marker = await _marker();
      if (enabled) {
        await marker.parent.create(recursive: true);
        await marker.writeAsString('', flush: true);
      } else if (await marker.exists()) {
        await marker.delete();
      }
      return enabled;
    });
  }
}
