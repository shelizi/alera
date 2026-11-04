import 'package:alera/src/shared/infra/runtime/runtime_host_providers.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'runtime_settings_changes.g.dart';

@Riverpod(keepAlive: true)
Stream<int> runtimeSettingsChanges(Ref ref) async* {
  final client = ref.watch(runtimeHostClientProvider);
  var revision = 0;
  await for (final event in client.runtimeEvents) {
    if (event.name == 'runtimeSettingsChanged') {
      // Riverpod may deduplicate equal AsyncData<void>(null) values. A
      // monotonic revision makes every Host settings change observable by
      // another Flutter window's SettingsController, including repeated
      // updates to the same preference.
      yield ++revision;
    }
  }
}
