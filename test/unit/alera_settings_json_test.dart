import 'package:alera/src/features/ai_assist/domain/ai_assist_settings.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_shortcut_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_id.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/settings/domain/editor_syntax_theme_catalog.dart';
import 'package:alera/src/features/settings/domain/terminal_theme_catalog.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('AleraSettings JSON', () {
    test('round-trips through json', () {
      const settings = AleraSettings(
        general: GeneralSettings(
          confirmProjectRemoval: false,
          confirmWorkspaceRemoval: false,
          keepAliveEnabled: true,
          showTrayIcon: false,
          showDockBadge: false,
          showTrayBadge: false,
        ),
        agents: AgentSettings(
          agentStatusHooks: AgentStatusHookSettings(
            values: {
              'codex': true,
              'claude': true,
              'cursor': true,
              'agy': true,
              'pi': true,
              'amp': true,
              'grok': true,
              'fx': true,
            },
          ),
          agentStatusNotificationsEnabled: true,
          keepComputerAwakeWhileAgentsWork: true,
          showTabTitlesInSidebar: true,
          defaultAgentProfileId: 'prof_1',
          gitBashExecutablePath: r'D:\PortableGit\git-bash.exe',
        ),
        editor: EditorSettings(
          tabSize: 2,
          themeName: EditorSyntaxThemeNames.nord,
          autosaveEnabled: true,
          autosaveDelaySeconds: 3,
          quickOpenExcludedDirectories: <String>['generated', 'cache'],
          languageIntelligence: LanguageIntelligenceSettings(
            languages: <String, LanguageActivationSettings>{
              'rust': LanguageActivationSettings(
                enabled: true,
                structuralParserEnabled: true,
                semanticProviderId: 'rust.rust-analyzer',
                executablePath: r'C:\Tools\rust-analyzer.exe',
                extraArgs: <String>['--log-file', 'ra.log'],
              ),
            },
          ),
        ),
        aiAssist: AiAssistSettings(
          agent: 'agy',
          selectedModelByAgent: <String, String>{
            'agy': 'Gemini 3.5 Flash (Medium)',
          },
          instructionsByOperation: <AiAssistOperation, String>{
            AiAssistOperation.commitMessage: 'Use conventional commits.',
          },
        ),
        terminal: TerminalSettings(
          fontFamily: 'SF Mono',
          fontSize: 15,
          fontWeight: 500,
          lineHeight: 1.4,
          paddingX: 8,
          paddingY: 10,
          cursorShape: .bar,
          cursorBlink: true,
          cursorOpacity: 0.75,
          themeName: TerminalThemeNames.dracula,
          backgroundOpacity: 0.9,
          wordSeparators: ' /',
          colorOverrides: TerminalColorOverrides(
            foreground: '#eeeeee',
            background: '#111111',
            cursor: '#ff00ff',
            selection: '#333333',
          ),
          scrollbackLines: 50000,
          hostEmptyShutdownDelaySeconds: 5,
          hostDetachedSessionShutdownDelaySeconds: 120,
          hostScrollbackBytes: 24 * 1000 * 1000,
          toolbarCorner: .bottomRight,
          powerShell7ExecutablePath: r'D:\Portable\PowerShell\pwsh.exe',
        ),
        keyboard: KeyboardShortcutSettings(
          overrides: <KeyboardActionId, List<String>>{
            KeyboardActionId.closeTab: <String>['Mod+Shift+W'],
          },
        ),
      );

      final encoded = settings.toMap();
      expect(encoded.containsKey('aiTextGeneration'), isTrue);
      expect(encoded.containsKey('aiAssist'), isFalse);

      final restored = AleraSettings.fromJson(
        Map<String, Object?>.from(encoded),
      );

      expect(restored.general.confirmProjectRemoval, isFalse);
      expect(restored.general.confirmWorkspaceRemoval, isFalse);
      expect(restored.general.keepAliveEnabled, isTrue);
      expect(restored.general.showTrayIcon, isFalse);
      expect(restored.general.showDockBadge, isFalse);
      expect(restored.general.showTrayBadge, isFalse);
      expect(restored.agents.agentStatusHooks.codex, isTrue);
      expect(
        restored.agents.gitBashExecutablePath,
        r'D:\PortableGit\git-bash.exe',
      );
      expect(restored.agents.agentStatusHooks.claude, isTrue);
      expect(restored.agents.agentStatusHooks.copilot, isFalse);
      expect(restored.agents.agentStatusHooks.cursor, isTrue);
      expect(restored.agents.agentStatusHooks.agy, isTrue);
      expect(restored.agents.agentStatusHooks.opencode, isFalse);
      expect(restored.agents.agentStatusHooks.pi, isTrue);
      expect(restored.agents.agentStatusHooks.amp, isTrue);
      expect(restored.agents.agentStatusHooks.grok, isTrue);
      expect(restored.agents.agentStatusHooks.fx, isTrue);
      expect(restored.agents.agentStatusNotificationsEnabled, isTrue);
      expect(restored.agents.keepComputerAwakeWhileAgentsWork, isTrue);
      expect(restored.agents.showTabTitlesInSidebar, isTrue);
      expect(restored.agents.defaultAgentProfileId, 'prof_1');
      expect(restored.editor.tabSize, 2);
      expect(restored.editor.themeName, EditorSyntaxThemeNames.nord);
      expect(restored.editor.autosaveEnabled, isTrue);
      expect(restored.editor.autosaveDelaySeconds, 3);
      expect(restored.editor.quickOpenExcludedDirectories, <String>[
        'generated',
        'cache',
      ]);
      final rustLanguageIntelligence = restored.editor.languageIntelligence
          .forLanguage(LanguageId('rust'));
      expect(rustLanguageIntelligence.enabled, isTrue);
      expect(rustLanguageIntelligence.structuralParserEnabled, isTrue);
      expect(rustLanguageIntelligence.semanticProviderId, 'rust.rust-analyzer');
      expect(
        rustLanguageIntelligence.executablePath,
        r'C:\Tools\rust-analyzer.exe',
      );
      expect(rustLanguageIntelligence.extraArgs, <String>[
        '--log-file',
        'ra.log',
      ]);
      expect(
        ((encoded['editor']! as Map)['languageIntelligence']
            as Map)['languages'],
        containsPair('rust', isA<Map>()),
      );
      expect(restored.aiAssist.agent, 'agy');
      expect(
        restored.aiAssist.modelForType(AgentType.agy),
        'Gemini 3.5 Flash (Medium)',
      );
      expect(
        restored.aiAssist.instructionsFor(.commitMessage),
        'Use conventional commits.',
      );
      expect(restored.terminal.fontFamily, 'SF Mono');
      expect(restored.terminal.fontSize, 15);
      expect(restored.terminal.fontWeight, 500);
      expect(restored.terminal.lineHeight, 1.4);
      expect(restored.terminal.paddingX, 8);
      expect(restored.terminal.paddingY, 10);
      expect(restored.terminal.cursorShape, TerminalCursorShape.bar);
      expect(restored.terminal.cursorBlink, isTrue);
      expect(restored.terminal.cursorOpacity, 0.75);
      expect(restored.terminal.themeName, TerminalThemeNames.dracula);
      expect(restored.terminal.backgroundOpacity, 0.9);
      expect(restored.terminal.wordSeparators, ' /');
      expect(restored.terminal.colorOverrides.foreground, '#eeeeee');
      expect(restored.terminal.colorOverrides.background, '#111111');
      expect(restored.terminal.colorOverrides.cursor, '#ff00ff');
      expect(restored.terminal.colorOverrides.selection, '#333333');
      expect(restored.terminal.scrollbackLines, 50000);
      expect(restored.terminal.hostEmptyShutdownDelaySeconds, 5);
      expect(restored.terminal.hostDetachedSessionShutdownDelaySeconds, 120);
      expect(restored.terminal.hostScrollbackBytes, 24 * 1000 * 1000);
      expect(
        restored.terminal.toolbarCorner,
        TerminalToolbarCorner.bottomRight,
      );
      expect(restored.keyboard.overrides[KeyboardActionId.closeTab], <String>[
        'Mod+Shift+W',
      ]);
    });

    test(
      'older editor json without language intelligence uses disabled defaults',
      () {
        final encoded = Map<String, Object?>.from(
          AleraSettings.defaults.toMap(),
        );
        final editor = Map<String, Object?>.from(encoded['editor']! as Map);
        editor.remove('languageIntelligence');
        encoded['editor'] = editor;

        final restored = AleraSettings.fromJson(encoded);

        expect(restored.editor.languageIntelligence.languages, isEmpty);
        expect(
          restored.editor.languageIntelligence.forLanguage(LanguageId('rust')),
          LanguageActivationSettings.defaults,
        );
      },
    );
  });
}
