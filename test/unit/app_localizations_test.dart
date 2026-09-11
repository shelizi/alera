import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/features/keyboard/domain/keyboard_action.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

part 'app_localizations_tail_test_cases.dart';

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
      expect(l10n.translate('Key'), '金鑰');
      expect(l10n.translate('Password'), '密碼');
      expect(l10n.translate('Add Project'), '新增專案');
      expect(l10n.translate('Open in Zed'), '在 Zed 中開啟');
    });

    test(
      'traditional Chinese localizes prompt workspace chrome and errors',
      () {
        final l10n = AleraLocalizations(const Locale('zh', 'TW'));
        expect(l10n.translate('Generating workspace identity'), '正在產生工作區識別');
        expect(l10n.translate('Starting agent'), '正在啟動 Agent');
        expect(
          l10n.translate('The generated branch "Settings" already exists.'),
          '產生的 Branch「Settings」已存在。',
        );
        expect(
          l10n.translate(
            'Bad state: The generated branch "Editor" already exists.',
          ),
          '產生的 Branch「Editor」已存在。',
        );
        expect(
          l10n.translate(
            'Bad state: AI Assist could not generate an available workspace identity.',
          ),
          'AI Assist 無法產生可用的工作區識別。',
        );
        expect(
          l10n.translate(
            'Unsupported operation: Update Alera on this host before retrying agent launch safely.',
          ),
          '請先更新此 Host 上的 Alera，再安全重試啟動 Agent。',
        );
        expect(
          l10n.translate(
            'Bad state: The original agent launch identity is unavailable.',
          ),
          '原始 Agent 啟動識別無法使用。',
        );
      },
    );

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

    test('traditional Chinese localizes automation catalog chrome', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Automations unavailable'), '無法取得自動化');
      expect(l10n.translate('No automations'), '沒有自動化');
      expect(l10n.translate('Select an automation'), '選擇自動化');
      expect(l10n.translate('State'), '狀態');
      expect(l10n.translate('Project'), '專案');
      expect(l10n.translate('Profile'), '設定檔');
      expect(l10n.translate('Tag'), '標籤');
      expect(l10n.translate('All State'), '所有狀態');
      expect(l10n.translate('All Project'), '所有專案');
      expect(l10n.translate('All Profile'), '所有設定檔');
      expect(l10n.translate('All Tag'), '所有標籤');
    });

    test('traditional Chinese localizes automation policy and settings', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Automation Policy Unavailable'), '無法取得自動化政策');
      expect(l10n.translate('Automation Permissions'), '自動化權限');
      expect(l10n.translate('May Execute Automations'), '可執行自動化');
      expect(l10n.translate('Automation Policy'), '自動化政策');
      expect(l10n.translate('Require Local Approval'), '需要本機核准');
      expect(l10n.translate('Loading Automation Settings'), '正在載入自動化設定');
      expect(l10n.translate('Automation Settings Unavailable'), '無法取得自動化設定');
      expect(l10n.translate('Automation History And Autostart'), '自動化歷程與自動啟動');
      expect(l10n.translate('Start Automations At Login'), '登入時啟動自動化');
      expect(l10n.translate('Run History Retention'), '執行歷程保留期限');
      expect(l10n.translate('days'), '天');
    });

    test('traditional Chinese localizes automation detail chrome', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Automation unavailable'), '無法取得自動化');
      expect(l10n.translate('Pause'), '暫停');
      expect(l10n.translate('Runs'), '執行紀錄');
      expect(l10n.translate('Audit'), '稽核');
      expect(l10n.translate('Schedule'), '排程');
      expect(l10n.translate('Cron / time'), 'Cron / 時間');
      expect(l10n.translate('Policies'), '政策');
      expect(l10n.translate('Limits'), '限制');
      expect(l10n.translate('Effective Policy'), '生效政策');
      expect(l10n.translate('Signature Timeline'), '簽章時間軸');
      expect(l10n.translate('Overlap'), '重疊處理');
    });

    test(
      'traditional Chinese localizes automation editor and import chrome',
      () {
        final l10n = AleraLocalizations(const Locale('zh', 'TW'));
        expect(l10n.translate('Edit Automation'), '編輯自動化');
        expect(l10n.translate('Project (Optional)'), '專案（選填）');
        expect(l10n.translate('Prompt Template'), '提示詞範本');
        expect(l10n.translate('Source Workspace'), '來源工作區');
        expect(l10n.translate('Agent Profile'), 'Agent 設定檔');
        expect(l10n.translate('Setup'), '設定');
        expect(l10n.translate('Misfire'), '錯過排程');
        expect(l10n.translate('Cleanup'), '清理');
        expect(l10n.translate('Versioned JSON Catalog'), '版本化 JSON Catalog');
        expect(
          l10n.translate('Source Key To Local Id JSON'),
          '來源 Key 到本機 ID 的 JSON',
        );
        expect(l10n.translate('Automation created'), '已建立自動化');
        expect(
          l10n.translate('Unknown prompt variable: {{workspace.foo}}'),
          '未知的提示詞變數：{{workspace.foo}}',
        );
      },
    );

    test('traditional Chinese localizes AI dictation settings', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Enable AI Dictation'), '啟用 AI 聽寫');
      expect(l10n.translate('Transcription Engine'), '轉錄引擎');
      expect(l10n.translate('Local Whisper'), '本機 Whisper');
      expect(
        l10n.translate('Codex Subscription (Experimental)'),
        'Codex 訂閱（實驗性）',
      );
      expect(l10n.translate('OpenAI-Compatible API'), 'OpenAI 相容 API');
      expect(l10n.translate('Runtime Update Required'), '需要更新執行環境');
      expect(l10n.translate('Allow Remote Audio Processing'), '允許遠端音訊處理');
      expect(l10n.translate('Request Timeout'), '請求逾時');
      expect(l10n.translate('Save Token'), '儲存 Token');
      expect(l10n.translate('Test Transcript'), '測試轉錄');
      expect(l10n.translate('Queue Download'), '排入下載佇列');
      expect(l10n.translate('Use Model'), '使用模型');
      expect(l10n.translate('12.5 MiB of 42.0 MiB'), '12.5 MiB / 42.0 MiB');
      expect(
        l10n.translate('Download interrupted at 12.5 MiB. Resume when ready.'),
        '下載在 12.5 MiB 時中斷，可在準備好後繼續。',
      );
      expect(l10n.translate('Download size 42.0 MiB.'), '下載大小 42.0 MiB。');
      expect(
        l10n.translate(
          'Balanced speed and accuracy. Recommended for most devices.',
        ),
        '兼顧速度與準確度，建議大多數裝置使用。',
      );
      expect(l10n.translate('Start Dictation'), '開始聽寫');
      expect(l10n.translate('Cancel Transcription'), '取消轉錄');
      expect(l10n.translate('Shell environment reloaded'), 'Shell 環境已重新載入');
      expect(l10n.translate('PATH entries'), '個 PATH 項目');
      expect(
        l10n.translate('Could not reload shell environment'),
        '無法重新載入 Shell 環境',
      );
      expect(l10n.translate('Could not open link'), '無法開啟連結');
      expect(l10n.translate('Resize Projects List'), '調整專案清單大小');
      expect(l10n.translate('Existing Branch *'), '現有分支 *');
      expect(l10n.translate('New Branch Name *'), '新分支名稱 *');
      expect(l10n.translate('Workspace Name (Optional)'), '工作區名稱（選填）');
      expect(l10n.translate('Search projects'), '搜尋專案');
      expect(l10n.translate('Search workspaces'), '搜尋工作區');
      expect(l10n.translate('Replace'), '取代');
      expect(l10n.translate('Files to include'), '要包含的檔案');
      expect(l10n.translate('Files to exclude'), '要排除的檔案');
      expect(l10n.translate('Match case'), '區分大小寫');
      expect(l10n.translate('Replace all'), '全部取代');
      expect(l10n.translate('Search Terminal'), '搜尋終端機');
      expect(l10n.translate('Search PDF'), '搜尋 PDF');
      expect(l10n.translate('Add tag…'), '新增標籤…');
      expect(l10n.translate('Add project…'), '新增專案…');
      expect(l10n.translate('Terminal Input'), '終端機輸入');
      expect(l10n.translate('Prompt Append'), '附加提示詞');
      expect(l10n.translate('Save Override'), '儲存覆寫設定');
      expect(l10n.translate('Custom Prompt'), '自訂提示詞');
      expect(l10n.translate('Quota Group'), '配額群組');
      expect(l10n.translate('CCS Profile'), 'CCS 設定檔');
      expect(l10n.translate('Usage Name'), '用量顯示名稱');
      expect(l10n.translate('Device Name'), '裝置名稱');
      expect(l10n.translate('My Phone'), '我的手機');
      expect(l10n.translate('Install Directory'), '安裝目錄');
      expect(l10n.translate('Default per platform'), '依平台使用預設值');
      expect(l10n.translate('Initial Prompt'), '初始提示詞');
      expect(l10n.translate('Select Agent Profile'), '選擇 Agent 設定檔');
      expect(l10n.translate('Create Another'), '繼續建立下一個');
      expect(l10n.translate('Working'), '處理中');
      expect(l10n.translate('SSH Targets'), 'SSH 目標');
      expect(l10n.translate('Connection'), '連線');
      expect(l10n.translate('Runtime Bootstrap'), '執行環境初始化');
      expect(l10n.translate('Remote runtime error'), '遠端執行環境錯誤');
      expect(l10n.translate('Installing'), '安裝中');
      expect(l10n.translate('Cursor Shape'), '游標形狀');
      expect(l10n.translate('Toolbar Corner'), '工具列位置');
      expect(l10n.translate('Top Right'), '右上');
      expect(l10n.translate('Selected: Alera Dark'), '已選取：Alera Dark');
      expect(l10n.translate('Showing 4 of 38'), '顯示 4 / 38');
      expect(l10n.translate('spaces'), '個空白');
      expect(l10n.translate('seconds'), '秒');
      expect(l10n.translate('External Editor'), '外部編輯器');
      expect(l10n.translate('Check Zed'), '檢查 Zed');
      expect(l10n.translate('Zed is available.'), 'Zed 可使用。');
      expect(l10n.translate('Zed is available: 0.210.0'), 'Zed 可使用：0.210.0');
      expect(l10n.translate('Zed is not available.'), 'Zed 無法使用。');
      expect(l10n.translate('Action'), '動作');
      expect(l10n.translate('Enabled'), '已啟用');
      expect(l10n.translate('Reasoning'), '推理');
      expect(l10n.translate('Global (Codex)'), '全域（Codex）');
      expect(l10n.translate('Running Settings.'), '正在執行「Settings」。');
      expect(l10n.translate('Action name is required.'), '動作名稱為必填。');
      expect(l10n.translate('Text action applied.'), '已套用文字操作。');
      expect(l10n.translate('No matching options'), '沒有符合的選項');
      expect(
        l10n.translate('Text action failed: provider unavailable'),
        '文字操作失敗：provider unavailable',
      );
      expect(
        l10n.translate('Describe the replacement to generate.'),
        '描述要產生的替換內容。',
      );
      expect(
        l10n.translate('Microphone permission is required for AI Dictation.'),
        'AI 聽寫需要麥克風權限。',
      );
      expect(
        l10n.translate('System speech recognition failed: provider detail'),
        '系統語音辨識失敗：provider detail',
      );
      expect(
        l10n.translate(
          'The transcript was inserted without speech processing: raw error',
        ),
        '語音處理失敗，已插入原始轉錄內容：raw error',
      );
    });

    test('traditional Chinese localizes account settings', () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Account unavailable'), '無法取得帳號');
      expect(l10n.translate('Continue With Google'), '使用 Google 繼續');
      expect(l10n.translate('Continue With GitHub'), '使用 GitHub 繼續');
      expect(l10n.translate('Alera Account'), 'Alera 帳號');
      expect(l10n.translate('Runtime runtime-123'), '執行環境 runtime-123');
      expect(l10n.translate('Link GitHub'), '連結 GitHub');
      expect(l10n.translate('Sign Out'), '登出');
      expect(l10n.translate('Mobile Push'), '行動推播');
      expect(
        l10n.translate('2 active mobile subscription(s).'),
        '2 個作用中的行動裝置訂閱。',
      );
      expect(l10n.translate('Move This Runtime'), '移轉此執行環境');
      expect(
        l10n.translate(
          'Transfer this runtime and its mobile subscriptions to account account-42? This installation will sign out.',
        ),
        '要將此執行環境及其行動裝置訂閱移轉到帳號 account-42 嗎？此安裝將會登出。',
      );
      expect(l10n.translate('Delete Alera Account'), '刪除 Alera 帳號');
      expect(
        l10n.translate('Sign in failed: provider rejected request'),
        '登入失敗：provider rejected request',
      );
    });

    registerAppLocalizationTailTests();
  });
}
