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

  test('traditional Chinese localizes recent workspace and agent settings', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(l10n.translate('Workspaces'), '工作區');
    expect(
      l10n.translate('Automatic archiving for inactive workspaces.'),
      '自動封存閒置的工作區。',
    );
    expect(l10n.translate('Auto-Archive After Inactivity'), '閒置後自動封存');
    expect(
      l10n.translate(
        'Days without activity before a workspace moves to the Archived section. Set to 0 to keep workspaces listed.',
      ),
      '工作區連續未活動達指定天數後移至「已封存」區段。設為 0 可讓工作區持續顯示。',
    );
    expect(
      l10n.translate('Move inactive workspaces into the Archived section.'),
      '將閒置工作區移至「已封存」區段。',
    );
    expect(l10n.translate('Agent Executables'), 'Agent 執行檔');
    expect(l10n.translate('Git Bash Executable'), 'Git Bash 執行檔');
    expect(l10n.translate('Codex Executable'), 'Codex 執行檔');
    expect(
      l10n.translate('Full path to the Codex executable. Default: codex'),
      'Codex 執行檔完整路徑。預設：codex',
    );
    expect(l10n.translate('Managed Options'), '受管理選項');
    expect(l10n.translate('Reasoning Effort'), '推理強度');
    expect(l10n.translate('Plan Mode Reasoning Effort'), 'Plan 模式推理強度');
    expect(l10n.translate('Extra High'), '極高');
    expect(l10n.translate('Workspace Write'), '工作區寫入');
    expect(l10n.translate('Approval Policy'), '核准政策');
    expect(l10n.translate('On Request'), '依要求');
    expect(l10n.translate('Accept Edits'), '接受編輯');
    expect(l10n.translate('Bypass Permissions'), '略過權限');
    expect(l10n.translate('Auto Review'), '自動審查');
    expect(l10n.translate('Dangerous'), '危險');
    expect(l10n.translate('Strict'), '嚴格');
    expect(l10n.translate('Custom: custom-model'), '自訂：custom-model');
  });

  test('traditional Chinese localizes concurrent settings conflicts', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(
      l10n.translate(
        'Settings changed elsewhere. Your change was not saved. Review the latest values and try again.',
      ),
      '設定已在其他位置變更。你的變更未儲存。請檢查最新值後再試一次。',
    );
    expect(
      l10n.translate(
        'Automation settings changed elsewhere. Your change was not saved. Review the latest values and try again.',
      ),
      '自動化設定已在其他位置變更。你的變更未儲存。請檢查最新值後再試一次。',
    );
  });

  test('traditional Chinese localizes Git history actions', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(l10n.translate('Copy Commit Hash'), '複製 Commit Hash');
    expect(l10n.translate('Create Branch Here…'), '在此建立 Branch…');
    expect(l10n.translate('Checkout Commit'), 'Checkout 此 Commit');
    expect(l10n.translate('Cherry Pick'), 'Cherry-pick');
    expect(l10n.translate('Revert Commit'), 'Revert 此 Commit');
    expect(l10n.translate('Drop Commit'), '移除 Commit');
    expect(l10n.translate('Reset Current Branch Here'), '在此重設目前 Branch');
    expect(l10n.translate('Current Branch'), '目前 Branch');
    expect(l10n.translate('Switch to Branch'), '切換到 Branch');
    expect(
      l10n.translate('Rebase Current Branch onto feature'),
      '將目前 Branch Rebase 到 feature',
    );
    expect(l10n.translate('Stash Changes'), 'Stash 變更');
    expect(l10n.translate('Discard All Changes'), '捨棄所有變更');
  });

  test(
    'traditional Chinese localizes workspace archive and commit graph chrome',
    () {
      final l10n = AleraLocalizations(const Locale('zh', 'TW'));
      expect(l10n.translate('Commit Graph'), 'Commit 圖譜');
      expect(l10n.translate('Open Commit Graph'), '開啟 Commit 圖譜');
      expect(l10n.translate('All Branches'), '所有 Branch');
      expect(l10n.translate('Archived'), '已封存');
      expect(l10n.translate('Open Project Settings'), '開啟專案設定');
      expect(l10n.translate('Remove Project'), '移除專案');
      expect(l10n.translate('Archive'), '封存');
      expect(l10n.translate('Archive Workspace?'), '封存工作區？');
      expect(l10n.translate('Workspace archived'), '工作區已封存');
      expect(
        l10n.translate('Could not archive workspace: disk error'),
        '無法封存工作區：disk error',
      );
      expect(
        l10n.translate('Could not restore workspace: disk error'),
        '無法還原工作區：disk error',
      );
    },
  );

  test('traditional Chinese localizes recent source control feedback', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(l10n.translate('Committed'), '已 Commit');
    expect(l10n.translate('Committed and pushed'), '已 Commit 並 Push');
    expect(l10n.translate('Committed and synced'), '已 Commit 並同步');
    expect(l10n.translate('Commit amended'), '已修訂 Commit');
    expect(l10n.translate('Source control refreshed'), '已重新整理原始碼控制');
    expect(l10n.translate('Staged'), '已暫存');
    expect(l10n.translate('Unstaged'), '已取消暫存');
    expect(l10n.translate('Fetched'), '已 Fetch');
    expect(l10n.translate('Pulled'), '已 Pull');
    expect(l10n.translate('Pushed'), '已 Push');
    expect(l10n.translate('Branch published'), 'Branch 已發布');
    expect(l10n.translate('Synced'), '已同步');
    expect(l10n.translate('Stashed'), '已 Stash');
    expect(l10n.translate('Stash popped'), '已套用 Stash');
    expect(l10n.translate('Changes discarded'), '已捨棄變更');
    expect(l10n.translate('Change discarded'), '已捨棄變更');
    expect(l10n.translate('Discard Changes?'), '捨棄變更？');
    expect(l10n.translate('Checkout Commit?'), 'Checkout Commit？');
    expect(l10n.translate('Revert Commit?'), 'Revert Commit？');
    expect(l10n.translate('Reset Current Branch?'), '重設目前 Branch？');
    expect(l10n.translate('Checked out abc1234'), '已 Checkout abc1234');
    expect(l10n.translate('Reverted abc1234'), '已 Revert abc1234');
    expect(l10n.translate('Reset to abc1234'), '已重設至 abc1234');
    expect(l10n.translate('Switched to feature/foo'), '已切換至 feature/foo');
    expect(
      l10n.translate('Created workspace feature/foo'),
      '已建立工作區 feature/foo',
    );
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

  test('traditional Chinese covers Settings user-facing static literals', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    final settingsRoot = Directory('lib/src/features/settings/presentation');
    final fieldPattern = RegExp(
      r'''(?:title|description|label|tooltip|hintText|buttonLabel|placeholder|message|confirmLabel|cancelLabel|sourceLabel)\s*:\s*'((?:\\'|[^'\r\n])*)',''',
    );
    final catalogTitlePattern = RegExp(
      r'''^\s*'((?:\\'|[^'\r\n])*)'\s*:\s*SettingsSearchEntryDetails\(''',
      multiLine: true,
    );
    const intentionallyEnglish = <String>{
      'Alera',
      'Agent',
      'AI Assist Agent',
      'Pull Requests',
      'Endpoint',
      'NetBird Endpoint',
      'Context',
      'Persona',
      'English',
      '繁體中文',
      'Claude',
      'macOS',
      'Linux',
      'Windows',
      'x64',
      'arm64',
      'en-US',
      'SF Mono',
      '127.0.0.1',
      r'~/.alera/workspaces',
      '.env',
      'make bootstrap',
      'llm --system commit-message',
      'ccwork',
      'work',
      'KIMI_API_KEY',
      'ZAI_API_KEY',
      'ZAI_BASE_URL',
      'MINIMAX_API_KEY',
      'MINIMAX_API_HOST',
      'px',
      'MB',
      's',
    };

    final missing = <String>{};
    for (final entity in settingsRoot.listSync(recursive: true)) {
      if (entity is! File || !entity.path.endsWith('.dart')) continue;
      final source = entity.readAsStringSync();
      final values = <String>{
        ...fieldPattern.allMatches(source).map((match) => match.group(1)!),
        ...catalogTitlePattern
            .allMatches(source)
            .map((match) => match.group(1)!),
      };
      for (final escaped in values) {
        final value = escaped.replaceAll(r"\'", "'");
        if (value.isEmpty ||
            intentionallyEnglish.contains(value) ||
            value.contains(r'${') ||
            value.startsWith('http://') ||
            value.startsWith('https://') ||
            RegExp(r'^[A-Za-z]:\\').hasMatch(value)) {
          continue;
        }
        if (l10n.translate(value) == value) missing.add(value);
      }
    }

    expect(
      missing,
      isEmpty,
      reason: 'Untranslated Settings literals:\n${missing.toList()..sort()}',
    );
  });

  test('traditional Chinese localizes Settings dynamic and multiline copy', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(
      l10n.translate(
        'If this is useful, consider starring the repo. It helps more developers discover it.',
      ),
      '如果 Alera 對你有幫助，可以考慮在 GitHub 為 Repository 加星，讓更多開發者能發現它。',
    );
    expect(
      l10n.translate(
        "Keeps this computer and display awake while agents are working. Lid-close behavior follows this device's power settings.",
      ),
      'Agent 工作期間讓此電腦與顯示器保持喚醒；闔上上蓋時仍依此裝置的電源設定運作。',
    );
    expect(
      l10n.translate(
        'Ceiling for terminal scrollback held in the app. Over it, terminals you have not looked at recently are unloaded and restored when you return. Their agents keep running. Only panes currently on screen stay loaded over it. Use 0 for no limit.',
      ),
      '應用程式中終端機回捲內容的記憶體上限。超過上限時，最近未檢視的終端機會卸載，返回時再還原；其中的 Agent 仍會繼續執行。只有目前畫面上的 Pane 可超過此上限保持載入。設為 0 表示不限制。',
    );
    expect(l10n.translate('Revoke My Phone'), '撤銷「My Phone」');
    expect(l10n.translate('Delete My Phone'), '刪除「My Phone」');
    expect(
      l10n.translate(
        'The device loses access and active sessions disconnect immediately. This cannot be undone.',
      ),
      '此裝置會失去存取權，作用中的工作階段會立即中斷連線。此操作無法復原。',
    );
    expect(
      l10n.translate(
        'Team Profile has no references. Deleting it cannot be undone.',
      ),
      '「Team Profile」沒有任何參照。刪除後無法復原。',
    );
    expect(
      l10n.translate(
        'Team Profile is referenced by 2 automations, 1 tab. Remove its automation and tab references before deleting it.',
      ),
      '「Team Profile」正被 2 個自動化、1 個分頁參照。請先移除相關的自動化與分頁參照再刪除。',
    );
    expect(l10n.translate('Agent Profile In Use'), 'Agent Profile 使用中');
    expect(l10n.translate('Delete Agent Profile?'), '刪除 Agent Profile？');
    expect(l10n.translate('Name is required.'), '名稱為必填。');
    expect(l10n.translate('Command is required.'), '指令為必填。');
    expect(
      l10n.translate(
        'This profile reduces Codex approval or sandbox protections.',
      ),
      '此 Profile 會降低 Codex 的核准或沙盒保護。',
    );
    expect(
      l10n.translate(
        'This profile will bypass Codex approvals and sandbox protections.',
      ),
      '此 Profile 會略過 Codex 的核准與沙盒保護。',
    );
    expect(l10n.translate('Check Zed'), '檢查 Zed');
    expect(
      l10n.translate(
        'After Alera creates a linked workspace, open that workspace in Zed automatically.',
      ),
      'Alera 建立連結工作區後，自動在 Zed 中開啟該工作區。',
    );
  });

  test('traditional Chinese localizes Settings status and quota copy', () {
    final l10n = AleraLocalizations(const Locale('zh', 'TW'));
    expect(l10n.translate('Move Codex Earlier'), '將 Codex 往前移');
    expect(l10n.translate('Move Work Later'), '將 Work 往後移');
    expect(l10n.translate('Usage: Personal'), '用量：Personal');
    expect(l10n.translate('Not shown in Usage'), '不顯示於用量');
    expect(l10n.translate('Codex Quotas'), 'Codex 配額');
    expect(
      l10n.translate('Alias and profile are required.'),
      '別名與 Profile 為必填。',
    );
    expect(
      l10n.translate('Alias and profile must be unique.'),
      '別名與 Profile 不可重複。',
    );
    expect(
      l10n.translate('Registration check failed: unavailable'),
      '註冊狀態檢查失敗：unavailable',
    );
    expect(
      l10n.translate('Registration failed: unavailable'),
      '註冊失敗：unavailable',
    );
    expect(l10n.translate('Model passed to Codex.'), '傳給 Codex 的模型。');
    expect(l10n.translate('Global (Codex)'), '全域（Codex）');
    expect(l10n.translate('Connected through relay'), '透過 Relay 連線');
    expect(l10n.translate('Revoked 2026-09-15 09:30'), '已撤銷：2026-09-15 09:30');
    expect(l10n.translate('Paired 2026-09-15 09:30'), '已配對：2026-09-15 09:30');
    expect(
      l10n.translate('Last seen 2026-09-15 09:30'),
      '最後出現：2026-09-15 09:30',
    );
    expect(l10n.translate('Expired'), '已過期');
    expect(l10n.translate('Expires in 2m'), '2 分鐘後到期');
    expect(l10n.translate('Expires in 12s'), '12 秒後到期');
    expect(l10n.translate('Expires in 2m 05s'), '2 分 05 秒後到期');
    expect(l10n.translate('Download size 128.0 MiB.'), '下載大小 128.0 MiB。');
    expect(
      l10n.translate('Download interrupted at 12.0 MiB. Resume when ready.'),
      '下載在 12.0 MiB 時中斷，可在準備好後繼續。',
    );
    expect(l10n.translate('12.0 MiB of 128.0 MiB'), '12.0 MiB / 128.0 MiB');
  });

  test('Settings direct text surfaces route through localization', () {
    final quotaSource = File(
      'lib/src/features/settings/presentation/panes/agent_quota_settings_controls.dart',
    ).readAsStringSync();
    final deviceSource = File(
      'lib/src/features/settings/presentation/panes/mobile_device_list_row.dart',
    ).readAsStringSync();
    final offerSource = File(
      'lib/src/features/settings/presentation/panes/mobile_pairing_offer_row.dart',
    ).readAsStringSync();
    final cliSource = File(
      'lib/src/features/settings/presentation/panes/agents_cli_skill_control.dart',
    ).readAsStringSync();

    expect(quotaSource, contains("context.tr('No quota providers enabled')"));
    expect(quotaSource, contains("context.tr('No CCS profiles configured')"));
    expect(quotaSource, contains('context.tr(error)'));
    expect(deviceSource, contains('context.tr(detail)'));
    expect(deviceSource, contains("label: context.tr('Revoked')"));
    expect(offerSource, contains('context.tr(_expiryLabel)'));
    expect(cliSource, contains('context.tr(summary)'));
    expect(cliSource, contains('context.tr(detail)'));
  });

  test('English and unknown strings fall back to source text', () {
    final en = AleraLocalizations(const Locale('en'));
    final zh = AleraLocalizations(const Locale('zh', 'TW'));
    expect(en.translate('Settings'), 'Settings');
    expect(zh.translate('Unmapped String'), 'Unmapped String');
  });
}
