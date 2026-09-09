import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('Alera localization', () {
    test('resolves configured language and system fallback', () {
      expect(
        resolveAleraLocale(AppLanguage.english, const Locale('zh', 'TW')),
        const Locale('en'),
      );
      expect(
        resolveAleraLocale(
          AppLanguage.traditionalChinese,
          const Locale('en', 'US'),
        ),
        const Locale('zh', 'TW'),
      );
      expect(
        resolveAleraLocale(AppLanguage.system, const Locale('zh', 'TW')),
        const Locale('zh', 'TW'),
      );
      expect(
        resolveAleraLocale(AppLanguage.system, const Locale('ja', 'JP')),
        const Locale('en'),
      );
    });

    test('traditional Chinese translates known UI strings', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Settings'), '設定');
      expect(l10n.translate('Add Project'), '新增專案');
      expect(l10n.translate('Open in Zed'), '在 Zed 中開啟');
    });

    test('traditional Chinese covers newly added terminal and worktree UI', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Refresh Worktrees'), '重新整理工作樹');
      expect(
        l10n.translate('Confirm Before Closing Busy Terminals'),
        '關閉忙碌終端機前確認',
      );
      expect(l10n.translate('Stop Running Agent?'), '停止執行中的代理程式？');
      expect(l10n.translate('Stop Running Command?'), '停止執行中的命令？');
      expect(l10n.translate('Stop And Close'), '停止並關閉');
    });
    test('traditional Chinese localizes side-by-side diff labels', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Original'), '原始');
      expect(l10n.translate('Modified'), '修改後');
      expect(l10n.translate('Empty'), '空白');
      expect(l10n.translate('Deleted'), '已刪除');
      expect(
        l10n.translate('Original (lib/src/app.dart)'),
        '原始（lib/src/app.dart）',
      );
    });

    test('traditional Chinese localizes dynamic close warnings', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(
        l10n.translate('README.md has unsaved changes.'),
        '「README.md」有尚未儲存的變更。',
      );
      expect(
        l10n.translate('3 editor tabs have unsaved changes.'),
        '有 3 個編輯器分頁包含尚未儲存的變更。',
      );
      expect(
        l10n.translate(
          'An agent is actively working in "Claude Code". Closing will terminate the session.',
        ),
        '代理程式正在「Claude Code」中執行工作。關閉後將終止此工作階段。',
      );
      expect(
        l10n.translate(
          'The process "dart" is still running in "Terminal 1". Closing will terminate it.',
        ),
        '程序「dart」仍在「Terminal 1」中執行。關閉後將終止該程序。',
      );
      expect(
        l10n.translate(
          'A command is still running in "Terminal 2". Closing will terminate it.',
        ),
        '「Terminal 2」中仍有命令正在執行。關閉後將終止該命令。',
      );
      expect(
        l10n.translate(
          '2 terminal tabs have running processes or active agents. Closing will terminate them.',
        ),
        '有 2 個終端機分頁仍有執行中程序或作用中代理程式。關閉後將終止它們。',
      );
    });

    test('traditional Chinese localizes runtime busy and ship dialogs', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Ship Changes?'), '送出變更？');
      expect(l10n.translate('Ship Staged Changes'), '送出已暫存變更');
      expect(l10n.translate('Ship All Changes'), '送出所有變更');
      expect(l10n.translate('Runtime Still Has Work'), '執行環境仍有工作進行中');
      expect(
        l10n.translate(
          'The runtime has 2 open agent(s) and 1 active terminal session(s).',
        ),
        '執行環境目前有 2 個開啟中的代理程式、1 個作用中的終端機工作階段。',
      );
      expect(
        l10n.translate(
          'The runtime has 1 active background job(s). You can quit and leave the runtime running, or force stop it.',
        ),
        '執行環境目前有 1 個作用中的背景工作。你可以結束 Alera 並讓執行環境保持運作，或強制停止它。',
      );
    });

    test('traditional Chinese keeps product and provider brand names in English', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Open in Zed'), contains('Zed'));
      expect(
        l10n.translate('Language used by the Alera interface.'),
        contains('Alera'),
      );
      expect(
        l10n.translate(
          'No Claude Code, Codex, or Grok Build usage was found in this range.',
        ),
        allOf(
          contains('Claude Code'),
          contains('Codex'),
          contains('Grok Build'),
        ),
      );
      expect(
        l10n.translate(
          'Local, Codex subscription, and OpenAI-compatible speech-to-text.',
        ),
        allOf(contains('Codex'), contains('OpenAI')),
      );
    });

    test('English and unknown strings fall back to source text', () {
      final en = AleraLocalizations(const Locale('en'));
      final zh = AleraLocalizations(const Locale('zh', 'TW'));
      expect(en.translate('Settings'), 'Settings');
      expect(zh.translate('Unmapped String'), 'Unmapped String');
    });
  });
}
