part of 'settings_dialog_test.dart';

void _registerSettingsDialogEditorTests() {
  testWidgets('edits and resets editor settings', (tester) async {
    final container = await _pumpSettingsDialog(tester);
    await tester.tap(find.text('Editor').first);
    await tester.pump();

    expect(find.text('Editor'), findsWidgets);
    expect(find.text('Tab Size'), findsOneWidget);
    expect(find.text('Theme Preset'), findsOneWidget);
    expect(find.text('Autosave'), findsWidgets);
    expect(find.text('Autosave Delay'), findsOneWidget);
    expect(container.read(settingsControllerProvider).editor.tabSize, 4);
    expect(
      container.read(settingsControllerProvider).editor.themeName,
      EditorSyntaxThemeNames.alera,
    );
    expect(
      container.read(settingsControllerProvider).editor.autosaveEnabled,
      isFalse,
    );
    expect(
      container.read(settingsControllerProvider).editor.autosaveDelaySeconds,
      EditorSettings.defaultAutosaveDelaySeconds,
    );

    await tester.tap(
      find.descendant(
        of: find.byKey(const ValueKey<String>('editor-autosave-row')),
        matching: find.byType(Switch),
      ),
    );
    await tester.pump(const Duration(milliseconds: 50));
    expect(
      container.read(settingsControllerProvider).editor.autosaveEnabled,
      isTrue,
    );

    await tester.enterText(
      find.byKey(const ValueKey<String>('editor-theme-search-field')),
      'monokai',
    );
    await tester.pump();
    await tester.tap(find.text(EditorSyntaxThemeNames.monokai));
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).editor.themeName,
      EditorSyntaxThemeNames.monokai,
    );

    await tester.ensureVisible(find.text('Tab Size'));
    await tester.pump();

    await tester.enterText(
      find.descendant(
        of: find.byKey(const ValueKey<String>('editor-tab-size-row')),
        matching: find.byType(TextField),
      ),
      '2',
    );
    await tester.testTextInput.receiveAction(.done);
    await tester.pump(const Duration(milliseconds: 50));

    expect(container.read(settingsControllerProvider).editor.tabSize, 2);

    await tester.tap(find.text('Reset Editor'));
    await tester.pump(const Duration(milliseconds: 50));

    expect(
      container.read(settingsControllerProvider).editor.tabSize,
      EditorSettings.defaults.tabSize,
    );
    expect(
      container.read(settingsControllerProvider).editor.themeName,
      EditorSettings.defaults.themeName,
    );
    expect(
      container.read(settingsControllerProvider).editor.autosaveEnabled,
      EditorSettings.defaults.autosaveEnabled,
    );
    expect(
      container.read(settingsControllerProvider).editor.autosaveDelaySeconds,
      EditorSettings.defaults.autosaveDelaySeconds,
    );
  });

  testWidgets('renders external editor settings in Traditional Chinese', (
    tester,
  ) async {
    await _pumpSettingsDialog(
      tester,
      initialSectionId: 'editor',
      locale: const Locale('zh', 'TW'),
    );

    expect(find.text('外觀'), findsOneWidget);
    expect(find.text('主題預設'), findsOneWidget);
    expect(find.text('縮排'), findsOneWidget);
    expect(find.text('自動儲存'), findsWidgets);
    await tester.ensureVisible(find.text('外部編輯器').first);
    await tester.pump();
    expect(find.text('外部編輯器'), findsWidgets);
    expect(find.text('預設程式碼開啟目標'), findsOneWidget);
    expect(find.text('自訂 Zed 執行檔'), findsOneWidget);
    expect(find.text('自動在外部編輯器開啟新工作區'), findsOneWidget);
    expect(find.text('檢查 Zed'), findsWidgets);
    expect(find.textContaining('本機命令環境中的 zed 指令'), findsOneWidget);
  });

  testWidgets('edits and restores Quick Open excluded directories', (
    tester,
  ) async {
    final container = await _pumpSettingsDialog(
      tester,
      initialSectionId: 'editor',
    );

    await tester.ensureVisible(find.text('Quick Open'));
    await tester.pump();
    expect(find.text('bin'), findsOneWidget);
    expect(find.text('obj'), findsOneWidget);
    expect(find.text('.vs'), findsOneWidget);

    await tester.enterText(
      find.byKey(const ValueKey<String>('quick-open-excluded-directory-input')),
      'generated',
    );
    await tester.tap(
      find.byKey(const ValueKey<String>('quick-open-excluded-directory-add')),
    );
    await tester.pump(const Duration(milliseconds: 50));
    expect(
      container
          .read(settingsControllerProvider)
          .editor
          .quickOpenExcludedDirectories,
      contains('generated'),
    );

    await tester.tap(
      find.byKey(const ValueKey<String>('quick-open-excluded-directory-reset')),
    );
    await tester.pump(const Duration(milliseconds: 50));
    expect(
      container
          .read(settingsControllerProvider)
          .editor
          .quickOpenExcludedDirectories,
      EditorSettings.defaultQuickOpenExcludedDirectories,
    );
  });

  testWidgets(
    'enables semantic intelligence per language from editor settings',
    (tester) async {
      final container = await _pumpSettingsDialog(
        tester,
        initialSectionId: 'editor',
        locale: const Locale('en'),
        extraOverrides: <dynamic>[
          languageIntelligenceStatusPortProvider.overrideWithValue(
            const _ReadyLanguageIntelligenceStatusPort(),
          ),
        ],
      );

      await tester.ensureVisible(find.text('Language Intelligence'));
      await tester.pump();

      for (final language in <String>[
        'C#',
        'Python',
        'Rust',
        'Go',
        'PHP',
        'TypeScript',
        'JavaScript',
      ]) {
        expect(find.text(language), findsOneWidget);
      }
      expect(
        container
            .read(settingsControllerProvider)
            .editor
            .languageIntelligence
            .forLanguage(LanguageId('rust'))
            .enabled,
        isFalse,
      );
      expect(
        container
            .read(settingsControllerProvider)
            .editor
            .languageIntelligence
            .forLanguage(
              LanguageId('rust'),
              structuralParserDefaultEnabled: true,
            )
            .structuralParserEnabled,
        isTrue,
      );
      final rustParserSwitch = find.byKey(
        const ValueKey<String>('language-intelligence-rust-parser'),
      );
      await tester.ensureVisible(rustParserSwitch);
      expect(tester.widget<Switch>(rustParserSwitch).value, isTrue);
      final csharpParserSwitch = find.byKey(
        const ValueKey<String>('language-intelligence-csharp-parser'),
      );
      await tester.ensureVisible(csharpParserSwitch);
      expect(tester.widget<Switch>(csharpParserSwitch).value, isFalse);
      await tester.tap(csharpParserSwitch);
      await tester.pump();
      expect(
        container
            .read(settingsControllerProvider)
            .editor
            .languageIntelligence
            .forLanguage(LanguageId('csharp'))
            .structuralParserEnabled,
        isTrue,
      );

      final rustEnabledSwitch = find.byKey(
        const ValueKey<String>('language-intelligence-rust-enabled'),
      );
      await tester.ensureVisible(rustEnabledSwitch);
      tester.widget<Switch>(rustEnabledSwitch).onChanged?.call(true);
      await tester.pump();
      await tester.pump(const Duration(milliseconds: 50));

      expect(
        container
            .read(settingsControllerProvider)
            .editor
            .languageIntelligence
            .forLanguage(LanguageId('rust'))
            .enabled,
        isTrue,
      );
      expect(find.text('Status: Ready'), findsOneWidget);
      expect(find.text('fake-language-server'), findsOneWidget);

      final executable = find.byKey(
        const ValueKey<String>('language-intelligence-rust-executable'),
      );
      await tester.ensureVisible(executable);
      await tester.enterText(executable, r'C:\Tools\rust-analyzer.exe');
      await tester.testTextInput.receiveAction(.done);
      await tester.pump(const Duration(milliseconds: 50));

      expect(
        container
            .read(settingsControllerProvider)
            .editor
            .languageIntelligence
            .forLanguage(LanguageId('rust'))
            .executablePath,
        r'C:\Tools\rust-analyzer.exe',
      );
    },
  );
}

final class _ReadyLanguageIntelligenceStatusPort
    implements LanguageIntelligenceStatusPort {
  const _ReadyLanguageIntelligenceStatusPort();

  @override
  Future<LanguageIntelligenceProviderStatus> resolve({
    required LanguageProviderDescriptor provider,
    required LanguageActivationSettings settings,
  }) async => const LanguageIntelligenceProviderStatus(
    kind: LanguageIntelligenceStatusKind.ready,
    executable: 'fake-language-server',
  );
}
