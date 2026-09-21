part of 'app_localizations_test.dart';

void registerAppLocalizationTailTests() {
  test('traditional Chinese localizes language server prerequisite guidance', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(
      l10n.translate(
        'node and npm were not found on PATH. Alera can install '
        'pyright-langserver automatically after Node.js and npm are available. '
        'Install Node.js, make sure `node --version` and `npm --version` work '
        'on PATH, then choose Check Again. No executable was otherwise found '
        'for python.pyright; tried pyright-langserver.',
      ),
      allOf(
        contains('Node.js／npm'),
        contains('pyright-langserver'),
        contains('重新檢查'),
      ),
    );
    expect(
      l10n.translate(
        'dotnet was found, but no usable .NET SDK was detected. Alera can '
        'install csharp-ls automatically after a .NET SDK is available. '
        'Install the .NET SDK (runtime-only is not enough), make sure '
        '`dotnet --list-sdks` lists an SDK, then choose Check Again.',
      ),
      allOf(
        contains('.NET SDK'),
        contains('csharp-ls'),
        contains('只有 Runtime 不足'),
      ),
    );
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
    expect(l10n.translate('2 attached items will be preserved.'), '將保留 2 個附件。');
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

  test(
    'traditional Chinese keeps product and provider brand names in English',
    () {
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
    },
  );

  test('traditional Chinese localizes recent workbench and Git features', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(l10n.translate('PowerShell 7 Executable'), 'PowerShell 7 執行檔');
    expect(l10n.translate('Sleep Project'), '休眠專案');
    expect(l10n.translate('Open with Default Application'), '使用預設應用程式開啟');
    expect(l10n.translate('Refresh Outline'), '重新整理大綱');
    expect(l10n.translate('Diff Overview'), '差異總覽');
    expect(l10n.translate('All Branches'), '所有分支');
    expect(l10n.translate('Waiting for input'), '等待輸入');
    expect(l10n.translate('Done (Unread)'), '完成（未讀）');
    expect(l10n.translate('Copy Commit Hash'), '複製 Commit Hash');
    expect(l10n.translate('Current Branch'), '目前分支');
    expect(l10n.translate('Create Branch Here'), '在此建立分支');
    expect(l10n.translate('File changed on disk'), '檔案已在磁碟上變更');
  });

  test('traditional Chinese localizes recent dynamic workbench copy', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(
      l10n.translate(
        'This closes all tabs and terminal sessions for all 3 workspaces in "Alera". '
        'Worktrees, branches, and files will be preserved. '
        '2 editors have unsaved changes that will be discarded.',
      ),
      contains('「Alera」全部 3 個工作區'),
    );
    expect(l10n.translate('Could not sleep project: boom'), '無法讓專案休眠：boom');
    expect(
      l10n.translate('Diff overview, 4 removed, 7 added'),
      '差異總覽，移除 4 行，新增 7 行',
    );
    expect(
      l10n.translate('Rebase Current Branch onto feature/demo'),
      '將目前分支 Rebase 到 feature/demo',
    );
    expect(l10n.translate('Branch Name is required'), '分支名稱 為必填。');
    expect(
      l10n.translate('Autosave paused: File changed on disk'),
      '自動儲存已暫停：檔案已在磁碟上變更',
    );
  });

  test('English and unknown strings fall back to source text', () {
    final en = AleraLocalizations(const Locale('en'));
    final zh = AleraLocalizations(const Locale('zh', 'TW'));
    expect(en.translate('Settings'), 'Settings');
    expect(zh.translate('Unmapped String'), 'Unmapped String');
  });
}
