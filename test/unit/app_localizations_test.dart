import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
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

    test('traditional Chinese localizes reading diff UI', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Reading diff generation failed'), '閱讀 Diff 產生失敗');
      expect(l10n.translate('high Effort'), '推理強度：high');
      expect(l10n.translate('2 Chunks'), '2 個區塊');
      expect(l10n.translate('Kept 2/4 Changed Lines'), '保留 2/4 行變更');
      expect(l10n.translate('Chunk 1 of 2'), '區塊 1/2');
      expect(l10n.translate('Generating chunk 2 of 3'), '正在產生區塊 2/3');
      expect(l10n.translate('Combining 4 chunks'), '正在合併 4 個區塊');
      expect(l10n.translate('Repository Read Only'), 'Repository 唯讀');
      expect(l10n.translate('Diff Only'), '僅限 Diff');
    });

    test('traditional Chinese covers keyboard settings and registry metadata', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('When a Terminal Is Focused'), '終端機取得焦點時');
      expect(l10n.translate('Press keys… (Esc to cancel)'), '請按下快速鍵…（Esc 取消）');
      expect(
        l10n.translate('Include at least one modifier key.'),
        '請至少包含一個修飾鍵。',
      );
      expect(
        l10n.translate(
          'Ctrl+W is assigned to "Close Tab". Reassign it to "New Terminal Tab"?',
        ),
        'Ctrl+W 已指派給「關閉分頁」。要重新指派給「新增終端機分頁」嗎？',
      );
      for (final definition in keybindingDefinitions) {
        expect(
          l10n.translate(definition.label),
          isNot(definition.label),
          reason: 'Missing keyboard label translation: ${definition.label}',
        );
        expect(
          l10n.translate(definition.description),
          isNot(definition.description),
          reason:
              'Missing keyboard description translation: ${definition.description}',
        );
      }
    });

    test('traditional Chinese localizes pull request review chrome', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Checks'), '檢查');
      expect(l10n.translate('Checks (3)'), '檢查（3）');
      expect(l10n.translate('Comments (2)'), '留言（2）');
      expect(l10n.translate('2 failing Checks'), '2 個失敗檢查');
      expect(l10n.translate('1 in progress Check'), '1 個進行中檢查');
      expect(l10n.translate('No comments yet'), '目前還沒有留言');
      expect(l10n.translate('Edit Pull Request'), '編輯 Pull Request');
      expect(l10n.translate('Base Branch'), '基底分支');
      expect(
        l10n.translate('#123 or pull request URL'),
        '#123 或 Pull Request URL',
      );
      expect(l10n.translate('Suggested pull request'), '建議的 Pull Request');
      expect(l10n.translate('CLI not found'), '找不到 CLI');
      expect(
        l10n.translate('Install `gh` and ensure it is on your PATH.'),
        '請安裝 `gh`，並確認它位於 PATH 中。',
      );
      expect(
        l10n.translate('Run `gh auth login` to sign in, then refresh.'),
        '請執行 `gh auth login` 登入，然後重新整理。',
      );
      expect(l10n.translate('Not authenticated'), '尚未驗證身分');
      expect(l10n.translate('Provider not detected'), '未偵測到 Provider');
    });

    test('traditional Chinese localizes agent usage chrome', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Usage'), '用量');
      expect(l10n.translate('30 Days'), '30 天');
      expect(l10n.translate('Processed Tokens'), '已處理 Token');
      expect(l10n.translate('49 assistant responses'), '49 則 Assistant 回覆');
      expect(l10n.translate('3 transcript sources'), '3 個逐字稿來源');
      expect(l10n.translate('25.0% of input'), '占輸入 25.0%');
      expect(
        l10n.translate(
          'Scanned 4 files in 42 ms. Transcript content stays on this host.',
        ),
        '已掃描 4 個檔案，耗時 42 ms。逐字稿內容會保留在此 Host。',
      );
      expect(
        l10n.translate(
          'Daily Claude Code, Codex, and Grok Build token usage. 2026-08-10: 42 tokens',
        ),
        contains('Claude Code、Codex 與 Grok Build 每日 Token 用量。'),
      );
    });

    test('traditional Chinese localizes resource manager chrome', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Memory'), '記憶體');
      expect(l10n.translate('Sort By Memory'), '依 記憶體 排序');
      expect(l10n.translate('1 orphan terminal'), '1 個孤立終端機');
      expect(l10n.translate('3 orphan terminals'), '3 個孤立終端機');
      expect(
        l10n.translate(
          'Force-quits Codex Agent. Anything running in that terminal is lost.',
        ),
        '將強制關閉「Codex Agent」。該終端機中正在執行的所有工作都會遺失。',
      );
      expect(l10n.translate('Runtime Host'), 'Runtime Host');
    });

    test('traditional Chinese localizes runtime busy and ship dialogs', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Ship Changes?'), '送出變更？');
      expect(l10n.translate('Ship Staged Changes'), '送出已暫存變更');
      expect(l10n.translate('Ship All Changes'), '送出所有變更');
      expect(l10n.translate('Runtime Still Has Work'), '執行環境仍有工作進行中');
      expect(l10n.translate('Edit Message'), '編輯訊息');
      expect(l10n.translate('Check Delivery'), '檢查傳送狀態');
      expect(
        l10n.translate('2 attached items will be preserved.'),
        '將保留 2 個附件。',
      );
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
