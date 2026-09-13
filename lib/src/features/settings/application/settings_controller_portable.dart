part of 'settings_controller.dart';

/// Mixin providing operations for the portable-cloud configuration tier.
///
/// These preferences are synchronized across devices and cloud via the portable
/// configuration document.
mixin _SettingsControllerPortableSettings on _$SettingsController {
  SettingsController get _controller => this as SettingsController;

  Future<void> updateTerminal(
    TerminalSettings Function(TerminalSettings) edit,
  ) => _controller._serialize(() async {
    await _controller._save(state.copyWith(terminal: edit(state.terminal)));
  });

  Future<void> resetTerminalSettings() => _controller._serialize(() async {
    await _controller._save(state.copyWith(terminal: .defaults));
  });

  Future<void> updateEditor(EditorSettings Function(EditorSettings) edit) =>
      _controller._serialize(() async {
        await _controller._save(state.copyWith(editor: edit(state.editor)));
      });

  Future<void> resetEditorSettings() => _controller._serialize(() async {
    await _controller._save(state.copyWith(editor: .defaults));
  });

  Future<void> updateAiDictation(
    AiDictationSettings Function(AiDictationSettings) edit,
  ) => _controller._serialize(() async {
    await _controller._save(
      state.copyWith(aiDictation: edit(state.aiDictation)),
    );
  });

  Future<void> resetAiDictation() => _controller._serialize(() async {
    await _controller._save(state.copyWith(aiDictation: .defaults));
  });

  Future<void> setActionBindings(KeyboardActionId id, List<String>? chords) =>
      _controller._serialize(() async {
        await _controller._save(
          state.copyWith(keyboard: state.keyboard.copyWithOverride(id, chords)),
        );
      });

  Future<void> applyBindingChanges(
    Map<KeyboardActionId, List<String>?> changes,
  ) => _controller._serialize(() async {
    var keyboard = state.keyboard;
    for (final entry in changes.entries) {
      keyboard = keyboard.copyWithOverride(entry.key, entry.value);
    }
    await _controller._save(state.copyWith(keyboard: keyboard));
  });

  Future<void> setTerminalShortcutPolicy(TerminalShortcutPolicy policy) =>
      _controller._serialize(() async {
        if (state.keyboard.terminalPolicy == policy) {
          return;
        }
        await _controller._save(
          state.copyWith(keyboard: state.keyboard.copyWithPolicy(policy)),
        );
      });

  Future<void> resetKeyboardShortcuts() => _controller._serialize(() async {
    await _controller._save(state.copyWith(keyboard: state.keyboard.cleared()));
  });

  Future<void> setShowTrayIcon(bool value) => _controller._serialize(() async {
    if (state.general.showTrayIcon == value) {
      return;
    }
    await _controller._save(
      state.copyWith(general: state.general.copyWith(showTrayIcon: value)),
    );
  });

  Future<void> setShowDockBadge(bool value) => _controller._serialize(() async {
    if (state.general.showDockBadge == value) {
      return;
    }
    await _controller._save(
      state.copyWith(general: state.general.copyWith(showDockBadge: value)),
    );
  });

  Future<void> setShowTrayBadge(bool value) => _controller._serialize(() async {
    if (state.general.showTrayBadge == value) {
      return;
    }
    await _controller._save(
      state.copyWith(general: state.general.copyWith(showTrayBadge: value)),
    );
  });

  Future<void> setAgentStatusNotificationsEnabled(bool value) =>
      _controller._serialize(() async {
        if (state.agents.agentStatusNotificationsEnabled == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            agents: state.agents.copyWith(
              agentStatusNotificationsEnabled: value,
            ),
          ),
        );
      });

  Future<void> setAgentStatusFinishedNotificationsEnabled(bool value) =>
      _controller._serialize(() async {
        if (state.agents.agentStatusFinishedNotificationsEnabled == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            agents: state.agents.copyWith(
              agentStatusFinishedNotificationsEnabled: value,
            ),
          ),
        );
      });

  Future<void> setShowTabTitlesInSidebar(bool value) =>
      _controller._serialize(() async {
        if (state.agents.showTabTitlesInSidebar == value) {
          return;
        }
        await _controller._save(
          state.copyWith(
            agents: state.agents.copyWith(showTabTitlesInSidebar: value),
          ),
        );
      });
}
