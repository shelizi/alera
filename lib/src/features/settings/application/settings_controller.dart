import 'dart:async';

import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/ai_dictation/domain/ai_dictation_settings.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
import 'package:alera/src/features/settings/application/runtime_settings_changes.dart';
import 'package:alera/src/features/settings/application/settings_providers.dart';
import 'package:alera/src/features/settings/application/settings_repository.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/text_actions/domain/text_actions_settings.dart';
import 'package:logging/logging.dart';
import 'package:riverpod_annotation/riverpod_annotation.dart';

part 'settings_controller.g.dart';
part 'settings_controller_local_ui.dart';
part 'settings_controller_runtime_operational.dart';
part 'settings_controller_portable.dart';

@Riverpod(keepAlive: true)
class SettingsController extends _$SettingsController
    with
        _SettingsControllerLocalUiSettings,
        _SettingsControllerRuntimeOperationalSettings,
        _SettingsControllerPortableSettings {
  bool _loadStarted = false;

  SettingsRepository get _repository => ref.read(settingsRepositoryProvider);

  @override
  AleraSettings build() {
    ref.listen(runtimeSettingsChangesProvider, (previous, next) {
      if (next.hasValue) {
        unawaited(load());
      }
    });
    if (!_loadStarted) {
      _loadStarted = true;
      unawaited(load());
    }
    return AleraSettings.defaults;
  }

  /// Loads persisted settings, keeping the defaults if that fails.
  ///
  /// Callers start this without awaiting it, so a throw here used to escape as
  /// an uncaught zone error that nothing recorded. Settings failing to load is
  /// worth a log line, not a crash: the defaults are a usable state.
  Future<void> load() => _serialize(() async {
    try {
      state = await _repository.load();
    } on Object catch (error, stackTrace) {
      Logger('SettingsController').warning(
        'failed to load settings; keeping the current values',
        error,
        stackTrace,
      );
    }
  });

  Future<void>? _operations;

  // Construct each mutation after prior persistence, including reloads from runtime events.
  Future<void> _serialize(Future<void> Function() operation) {
    final next = _operations == null
        ? operation()
        : _operations!.then((_) => operation());
    final tail = next.then<void>((_) {}, onError: (Object _, StackTrace _) {});
    _operations = tail;
    unawaited(
      tail.then((_) {
        if (identical(_operations, tail)) _operations = null;
      }),
    );
    return next;
  }

  Future<void> _save(AleraSettings settings) async {
    await _repository.save(settings);
    state = settings;
  }
}
