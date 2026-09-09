import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';

const List<Locale> supportedAleraLocales = <Locale>[
  Locale('en'),
  Locale('zh', 'TW'),
];

Locale resolveAleraLocale(AppLanguage language, Locale? systemLocale) {
  return switch (language) {
    AppLanguage.english => const Locale('en'),
    AppLanguage.traditionalChinese => const Locale('zh', 'TW'),
    AppLanguage.system =>
      systemLocale?.languageCode.toLowerCase() == 'zh'
          ? const Locale('zh', 'TW')
          : const Locale('en'),
  };
}

class AleraLocalizations {
  const AleraLocalizations(this.locale);

  final Locale locale;

  bool get isTraditionalChinese => locale.languageCode.toLowerCase() == 'zh';

  String translate(String source) {
    if (!isTraditionalChinese) {
      return source;
    }
    return _traditionalChinese[source] ??
        _translateDynamicTraditionalChinese(source) ??
        source;
  }

  static String? _translateDynamicTraditionalChinese(String source) {
    final singleDirty = RegExp(r'^(.+) has unsaved changes\.$')
        .firstMatch(source);
    if (singleDirty != null) {
      return '「${singleDirty.group(1)}」有尚未儲存的變更。';
    }
    final multipleDirty = RegExp(r'^(\d+) editor tabs have unsaved changes\.$')
        .firstMatch(source);
    if (multipleDirty != null) {
      return '有 ${multipleDirty.group(1)} 個編輯器分頁包含尚未儲存的變更。';
    }
    final activeAgent = RegExp(
      r'^An agent is actively working in "(.+)"\. Closing will terminate the session\.$',
    ).firstMatch(source);
    if (activeAgent != null) {
      return '代理程式正在「${activeAgent.group(1)}」中執行工作。關閉後將終止此工作階段。';
    }
    final runningProcess = RegExp(
      r'^The process "(.+)" is still running in "(.+)"\. Closing will terminate it\.$',
    ).firstMatch(source);
    if (runningProcess != null) {
      return '程序「${runningProcess.group(1)}」仍在「${runningProcess.group(2)}」中執行。關閉後將終止該程序。';
    }
    final runningCommand = RegExp(
      r'^A command is still running in "(.+)"\. Closing will terminate it\.$',
    ).firstMatch(source);
    if (runningCommand != null) {
      return '「${runningCommand.group(1)}」中仍有命令正在執行。關閉後將終止該命令。';
    }
    final busyTerminals = RegExp(
      r'^(\d+) terminal tabs have running processes or active agents\. Closing will terminate them\.$',
    ).firstMatch(source);
    if (busyTerminals != null) {
      return '有 ${busyTerminals.group(1)} 個終端機分頁仍有執行中程序或作用中代理程式。關閉後將終止它們。';
    }
    final preservedAttachments = RegExp(
      r'^(\d+) attached (?:item|items) will be preserved\.$',
    ).firstMatch(source);
    if (preservedAttachments != null) {
      return '將保留 ${preservedAttachments.group(1)} 個附件。';
    }

    final effortLabel = RegExp(r'^(.+) Effort$').firstMatch(source);
    if (effortLabel != null) {
      return '推理強度：${effortLabel.group(1)}';
    }
    final chunkCount = RegExp(r'^(\d+) (?:Chunk|Chunks)$').firstMatch(source);
    if (chunkCount != null) {
      return '${chunkCount.group(1)} 個區塊';
    }
    final retainedLines = RegExp(r'^Kept (\d+)/(\d+) Changed Lines$')
        .firstMatch(source);
    if (retainedLines != null) {
      return '保留 ${retainedLines.group(1)}/${retainedLines.group(2)} 行變更';
    }
    final chunkPosition = RegExp(r'^Chunk (\d+) of (\d+)$').firstMatch(source);
    if (chunkPosition != null) {
      return '區塊 ${chunkPosition.group(1)}/${chunkPosition.group(2)}';
    }
    final generatingChunk = RegExp(r'^Generating chunk (\d+) of (\d+)$')
        .firstMatch(source);
    if (generatingChunk != null) {
      return '正在產生區塊 ${generatingChunk.group(1)}/${generatingChunk.group(2)}';
    }
    final repairingChunk = RegExp(r'^Repairing chunk (\d+) of (\d+)$')
        .firstMatch(source);
    if (repairingChunk != null) {
      return '正在修復區塊 ${repairingChunk.group(1)}/${repairingChunk.group(2)}';
    }
    final combiningChunks = RegExp(r'^Combining (\d+) (?:chunk|chunks)$')
        .firstMatch(source);
    if (combiningChunks != null) {
      return '正在合併 ${combiningChunks.group(1)} 個區塊';
    }

    final shortcutConflict = RegExp(
      r'^(.+) is assigned to "(.+)"\. Reassign it to "(.+)"\?$',
    ).firstMatch(source);
    if (shortcutConflict != null) {
      final conflictLabel =
          _traditionalChinese[shortcutConflict.group(2)!] ??
          shortcutConflict.group(2)!;
      final targetLabel =
          _traditionalChinese[shortcutConflict.group(3)!] ??
          shortcutConflict.group(3)!;
      return '${shortcutConflict.group(1)} 已指派給「$conflictLabel」。要重新指派給「$targetLabel」嗎？';
    }
    final unsupportedKey = RegExp(r'^Unsupported key: (.+)\.$')
        .firstMatch(source);
    if (unsupportedKey != null) {
      return '不支援的按鍵：${unsupportedKey.group(1)}。';
    }

    final sortBy = RegExp(r'^Sort By (.+)$').firstMatch(source);
    if (sortBy != null) {
      final label = _traditionalChinese[sortBy.group(1)!] ?? sortBy.group(1)!;
      return '依 $label 排序';
    }
    final orphanTerminals = RegExp(r'^(\d+) orphan terminal(?:s)?$')
        .firstMatch(source);
    if (orphanTerminals != null) {
      return '${orphanTerminals.group(1)} 個孤立終端機';
    }
    final forceQuitTerminal = RegExp(
      r'^Force-quits (.+)\. Anything running in that terminal is lost\.$',
    ).firstMatch(source);
    if (forceQuitTerminal != null) {
      return '將強制關閉「${forceQuitTerminal.group(1)}」。該終端機中正在執行的所有工作都會遺失。';
    }

    final usageCount = RegExp(
      r'^(\d+) (assistant responses|transcript sources|unpriced responses)$',
    ).firstMatch(source);
    if (usageCount != null) {
      final noun = switch (usageCount.group(2)) {
        'assistant responses' => '則 Assistant 回覆',
        'transcript sources' => '個逐字稿來源',
        _ => '則未計價回覆',
      };
      return '${usageCount.group(1)} $noun';
    }
    final usageInputShare = RegExp(r'^([0-9.]+%) of input$').firstMatch(source);
    if (usageInputShare != null) {
      return '占輸入 ${usageInputShare.group(1)}';
    }
    final usageScanSummary = RegExp(
      r'^Scanned (\d+) files in (\d+) ms\. Transcript content stays on this host\.$',
    ).firstMatch(source);
    if (usageScanSummary != null) {
      return '已掃描 ${usageScanSummary.group(1)} 個檔案，耗時 ${usageScanSummary.group(2)} ms。逐字稿內容會保留在此 Host。';
    }
    final usagePartial = RegExp(r'^(.+) (.+) is partial\.$').firstMatch(source);
    if (usagePartial != null) {
      return '${usagePartial.group(1)} ${usagePartial.group(2)} 的資料不完整。';
    }
    final dailyUsageSemantics = RegExp(
      r'^Daily Claude Code, Codex, and Grok Build token usage\. (.+)$',
    ).firstMatch(source);
    if (dailyUsageSemantics != null) {
      return 'Claude Code、Codex 與 Grok Build 每日 Token 用量。${dailyUsageSemantics.group(1)}';
    }

    final pullRequestCount = RegExp(r'^(Checks|Comments) \((\d+)\)$')
        .firstMatch(source);
    if (pullRequestCount != null) {
      final label = pullRequestCount.group(1) == 'Checks' ? '檢查' : '留言';
      return '$label（${pullRequestCount.group(2)}）';
    }
    final checkGroup = RegExp(
      r'^(\d+) (failing|in progress|successful) Checks?$',
    ).firstMatch(source);
    if (checkGroup != null) {
      final state = switch (checkGroup.group(2)) {
        'failing' => '失敗',
        'in progress' => '進行中',
        _ => '成功',
      };
      return '${checkGroup.group(1)} 個$state檢查';
    }

    final installProviderCli = RegExp(
      r'^Install `(.+)` and ensure it is on your PATH\.$',
    ).firstMatch(source);
    if (installProviderCli != null) {
      return '請安裝 `${installProviderCli.group(1)}`，並確認它位於 PATH 中。';
    }
    final runProviderAuth = RegExp(r'^Run `(.+)` to sign in, then refresh\.$')
        .firstMatch(source);
    if (runProviderAuth != null) {
      return '請執行 `${runProviderAuth.group(1)}` 登入，然後重新整理。';
    }

    final accountRuntime = RegExp(r'^Runtime (.+)$').firstMatch(source);
    if (accountRuntime != null) {
      return '執行環境 ${accountRuntime.group(1)}';
    }
    final linkIdentityProvider = RegExp(r'^Link (Google|GitHub)$')
        .firstMatch(source);
    if (linkIdentityProvider != null) {
      return '連結 ${linkIdentityProvider.group(1)}';
    }
    final activeMobileSubscriptions = RegExp(
      r'^(\d+) active mobile subscription\(s\)\.$',
    ).firstMatch(source);
    if (activeMobileSubscriptions != null) {
      return '${activeMobileSubscriptions.group(1)} 個作用中的行動裝置訂閱。';
    }
    final transferRuntimeAccount = RegExp(
      r'^Transfer this runtime and its mobile subscriptions to account (.+)\? This installation will sign out\.$',
    ).firstMatch(source);
    if (transferRuntimeAccount != null) {
      return '要將此執行環境及其行動裝置訂閱移轉到帳號 ${transferRuntimeAccount.group(1)} 嗎？此安裝將會登出。';
    }
    final signInFailure = RegExp(r'^Sign in failed: (.+)$').firstMatch(source);
    if (signInFailure != null) {
      return '登入失敗：${signInFailure.group(1)}';
    }

    final allAutomationFilter = RegExp(r'^All (State|Project|Profile|Tag)$')
        .firstMatch(source);
    if (allAutomationFilter != null) {
      final label =
          _traditionalChinese[allAutomationFilter.group(1)!] ??
          allAutomationFilter.group(1)!;
      return '所有$label';
    }

    final unknownPromptVariable = RegExp(r'^Unknown prompt variable: (.+)$')
        .firstMatch(source);
    if (unknownPromptVariable != null) {
      return '未知的提示詞變數：${unknownPromptVariable.group(1)}';
    }

    final dictationDownloadProgress = RegExp(
      r'^(\d+(?:\.\d+)? (?:KiB|MiB)) of (\d+(?:\.\d+)? (?:KiB|MiB))$',
    ).firstMatch(source);
    if (dictationDownloadProgress != null) {
      return '${dictationDownloadProgress.group(1)} / ${dictationDownloadProgress.group(2)}';
    }
    final dictationInterrupted = RegExp(
      r'^Download interrupted at (.+)\. Resume when ready\.$',
    ).firstMatch(source);
    if (dictationInterrupted != null) {
      return '下載在 ${dictationInterrupted.group(1)} 時中斷，可在準備好後繼續。';
    }

    final originalPath = RegExp(r'^Original \((.+)\)$').firstMatch(source);
    if (originalPath != null) {
      return '原始（${originalPath.group(1)}）';
    }

    final runtimeBusyWithSuffix = RegExp(
      r'^The runtime has (.+)\. (Force stop terminates them\.|You can quit and leave the runtime running, or force stop it\.)$',
    ).firstMatch(source);
    if (runtimeBusyWithSuffix != null) {
      final details = _translateRuntimeBusyItems(
        runtimeBusyWithSuffix.group(1)!,
      );
      final suffix = runtimeBusyWithSuffix.group(2)!;
      final translatedSuffix = switch (suffix) {
        'Force stop terminates them.' => '強制停止會終止這些工作。',
        _ => '你可以結束 Alera 並讓執行環境保持運作，或強制停止它。',
      };
      return '執行環境目前有 $details。$translatedSuffix';
    }

    final runtimeBusy = RegExp(r'^The runtime has (.+)\.$').firstMatch(source);
    if (runtimeBusy != null) {
      final details = _translateRuntimeBusyItems(runtimeBusy.group(1)!);
      return '執行環境目前有 $details。';
    }

    return null;
  }

  static String _translateRuntimeBusyItems(String source) {
    return source
        .replaceAllMapped(
          RegExp(r'(\d+) open agent\(s\)'),
          (match) => '${match.group(1)} 個開啟中的代理程式',
        )
        .replaceAllMapped(
          RegExp(r'(\d+) active terminal session\(s\)'),
          (match) => '${match.group(1)} 個作用中的終端機工作階段',
        )
        .replaceAllMapped(
          RegExp(r'(\d+) active background job\(s\)'),
          (match) => '${match.group(1)} 個作用中的背景工作',
        )
        .replaceAllMapped(
          RegExp(r'(\d+) active push subscription\(s\)'),
          (match) => '${match.group(1)} 個作用中的推播訂閱',
        )
        .replaceAll(', and ', '、')
        .replaceAll(' and ', '、')
        .replaceAll(', ', '、');
  }

  static const Map<String, String> _traditionalChinese = <String, String>{
    // Common actions and navigation.
    'Settings': '設定',
    'Close': '關閉',
    'Cancel': '取消',
    'Save': '儲存',
    'Delete': '刪除',
    'Remove': '移除',
    'Rename': '重新命名',
    'Refresh': '重新整理',
    'Retry': '重試',
    'Try Again': '再試一次',
    'Copy': '複製',
    'Cut': '剪下',
    'Paste': '貼上',
    'Browse': '瀏覽',
    'Search': '搜尋',
    'Clear': '清除',
    'Create': '建立',
    'Continue': '繼續',
    'Open': '開啟',
    'Export': '匯出',
    'Import': '匯入',
    'Apply': '套用',
    'Reset': '重設',
    'Reset to Default': '重設為預設值',
    'Preferences': '偏好設定',
    'Resources': '資源',
    'Search settings': '搜尋設定',
    'No matching settings.': '沒有符合的設定。',
    'No settings found.': '找不到設定。',

    // Workspace and editor shell.
    'Add Project': '新增專案',
    'New Workspace': '新增工作區',
    'Open Settings': '開啟設定',
    'Quick Start': '快速開始',
    'Keyboard Shortcuts': '鍵盤快速鍵',
    'Explorer': '檔案總管',
    'Source Control': '原始碼控制',
    'Pull Request': 'Pull Request',
    'Pull Requests': 'Pull Requests',
    'Automations': '自動化',
    'New Terminal': '新增終端機',
    'New Tab': '新增分頁',
    'Open in Zed': '在 Zed 中開啟',
    'Open in Alera': '在 Alera 中開啟',
    'Copy path': '複製路徑',
    'Copy relative path': '複製相對路徑',
    'New file': '新增檔案',
    'New folder': '新增資料夾',
    'Collapse All': '全部收合',
    'Save all files': '儲存所有檔案',

    'No projects yet': '尚無專案',
    'Add a git repository to create workspaces with terminal tabs.':
        '新增 Git 儲存庫以建立含終端機分頁的工作區。',
    'Add Your First Project': '新增第一個專案',
    'Unpin Workspace': '取消釘選工作區',
    'Pin Workspace': '釘選工作區',
    'Pin Workspace Tree': '釘選工作區樹',
    'Unpin Workspace Tree': '取消釘選工作區樹',
    'Manage Tags': '管理標籤',
    'Set Parent Workspace': '設定父工作區',
    'Clear Parent Workspace': '清除父工作區',
    'Set Section': '設定區段',
    'Clear Section': '清除區段',
    'Open in Browser': '在瀏覽器中開啟',
    'Open in Project Settings': '在專案設定中開啟',
    'Copy Path': '複製路徑',
    'Sleep': '休眠',
    'Folder name': '資料夾名稱',
    'File name': '檔案名稱',
    'Name': '名稱',
    'Path copied': '已複製路徑',
    'Relative path copied': '已複製相對路徑',
    'Duplicate': '建立副本',
    'Show ignored files': '顯示忽略的檔案',
    'Hide ignored files': '隱藏忽略的檔案',
    'Source control root': '原始碼控制根目錄',
    'Clear Source Control Root': '清除原始碼控制根目錄',
    'Use As Source Control Root': '設為原始碼控制根目錄',
    'Collapse folder': '收合資料夾',
    'Move this item to the trash?': '要將此項目移到資源回收筒嗎？',
    'Open File': '開啟檔案',
    'Reveal in Explorer': '在檔案總管中顯示',
    'Stage': '暫存',
    'Unstage': '取消暫存',
    'Discard': '捨棄',
    'All Changes': '所有變更',
    'Show Flat List': '顯示平面清單',
    'Show Tree': '顯示樹狀檢視',
    'Show All Changes': '顯示所有變更',
    'Group By Staged State': '依暫存狀態分組',
    'Hide File Filter': '隱藏檔案篩選',
    'Search Files': '搜尋檔案',
    'Expand All': '全部展開',
    'Filter files...': '篩選檔案…',
    'Stop generating commit message': '停止產生 Commit 訊息',
    'Generate commit message with AI': '使用 AI 產生 Commit 訊息',
    'Source Control Actions': '原始碼控制操作',
    'Commit': 'Commit',
    'Commit & Push': 'Commit 並 Push',
    'Commit & Sync': 'Commit 並同步',
    'Commit Amend': '修改上一筆 Commit',
    'Stage All': '全部暫存',
    'Unstage All': '全部取消暫存',
    'Discard All': '全部捨棄',
    'Open Changes in Zed': '在 Zed 中開啟變更',
    'Fetch': 'Fetch',
    'Pull': 'Pull',
    'Push': 'Push',
    'Sync': '同步',
    'Publish Branch': '發佈分支',
    'Stash': 'Stash',
    'Stash Pop': '套用 Stash',
    'Quick Open': '快速開啟',
    'Search workspace files': '搜尋工作區檔案',
    'Use Up and Down to navigate, Enter to open, or Escape to close.':
        '使用上、下方向鍵移動，Enter 開啟，Escape 關閉。',
    'Loading workspace files...': '正在載入工作區檔案…',
    'Could not load workspace files.': '無法載入工作區檔案。',
    'No files are available in this workspace.': '此工作區沒有可用的檔案。',
    'No files match': '沒有符合的檔案：',
    'Command Palette': '指令面板',
    'Search commands': '搜尋指令',
    'Use Up and Down to navigate, Enter to run, or Escape to close.':
        '使用上、下方向鍵移動，Enter 執行，Escape 關閉。',
    'No commands are available.': '目前沒有可用的指令。',
    'No commands match': '沒有符合的指令：',
    'No shortcut': '無快速鍵',
    'How shortcuts behave while a terminal is focused.': '設定終端機取得焦點時快速鍵的行為。',
    'When a Terminal Is Focused': '終端機取得焦點時',
    'App first lets Alera capture combinations the shell would otherwise receive. Terminal first defers to the shell.':
        '「應用程式優先」會讓 Alera 攔截原本會傳給 Shell 的組合鍵；「終端機優先」則優先交由 Shell 處理。',
    'Stop Recording': '停止錄製',
    'Change Shortcut': '變更快速鍵',
    'Press keys… (Esc to cancel)': '請按下快速鍵…（Esc 取消）',
    'Disabled': '已停用',
    'Unassigned': '未指派',
    'Shortcut already in use': '快速鍵已被使用',
    'Reassign': '重新指派',
    'Enter a shortcut like Ctrl+Shift+P.': '請輸入快速鍵，例如 Ctrl+Shift+P。',
    'A shortcut can only have one main key.': '快速鍵只能有一個主要按鍵。',
    'Add a main key, like P or Enter.': '請加入主要按鍵，例如 P 或 Enter。',
    'Use either Mod or a platform-specific modifier, not both.':
        '請使用 Mod 或平台專用修飾鍵其中一種，不要同時使用。',
    'Include at least one modifier key.': '請至少包含一個修飾鍵。',
    'Press a non-modifier key too.': '請再按下一個非修飾鍵。',
    'Unsupported key.': '不支援此按鍵。',
    'Go to Tab 1': '前往分頁 1',
    'Go to Tab 2': '前往分頁 2',
    'Go to Tab 3': '前往分頁 3',
    'Go to Tab 4': '前往分頁 4',
    'Go to Tab 5': '前往分頁 5',
    'Go to Tab 6': '前往分頁 6',
    'Go to Tab 7': '前往分頁 7',
    'Go to Tab 8': '前往分頁 8',
    'Go to Last Tab': '前往最後一個分頁',
    'Open the settings dialog.': '開啟設定對話框。',
    'Open the runtime-local automation manager.': '開啟此執行環境的自動化管理器。',
    'Search and open a file in the active workspace.': '搜尋並開啟目前工作區中的檔案。',
    'Search and run an Alera command.': '搜尋並執行 Alera 指令。',
    'Open the add-project dialog.': '開啟新增專案對話框。',
    'Collapse or expand the project sidebar.': '收合或展開專案側邊欄。',
    'Create a linked workspace for the active Git project.':
        '為目前的 Git 專案建立連結工作區。',
    'Open the active workspace in Zed.': '在 Zed 中開啟目前工作區。',
    'Go to the previously selected workspace.': '前往先前選取的工作區。',
    'Go to the next workspace in navigation history.': '前往導覽紀錄中的下一個工作區。',
    'Open workspace search.': '開啟工作區搜尋。',
    'Search the active terminal scrollback.': '搜尋目前終端機的捲動緩衝內容。',
    'Show or hide the prompt composer for the active terminal.':
        '顯示或隱藏目前終端機的提示詞輸入區。',
    'Open workspace search and replace.': '開啟工作區搜尋與取代。',
    'Save the active editor file.': '儲存目前編輯器檔案。',
    'Open a terminal tab in the active workspace.': '在目前工作區開啟終端機分頁。',
    'Close the active terminal tab.': '關閉目前的終端機分頁。',
    'Select the next tab in the active pane.': '選取目前窗格中的下一個分頁。',
    'Select the previous tab in the active pane.': '選取目前窗格中的上一個分頁。',
    'Select the first tab in the active pane.': '選取目前窗格中的第一個分頁。',
    'Select the second tab in the active pane.': '選取目前窗格中的第二個分頁。',
    'Select the third tab in the active pane.': '選取目前窗格中的第三個分頁。',
    'Select the fourth tab in the active pane.': '選取目前窗格中的第四個分頁。',
    'Select the fifth tab in the active pane.': '選取目前窗格中的第五個分頁。',
    'Select the sixth tab in the active pane.': '選取目前窗格中的第六個分頁。',
    'Select the seventh tab in the active pane.': '選取目前窗格中的第七個分頁。',
    'Select the eighth tab in the active pane.': '選取目前窗格中的第八個分頁。',
    'Select the last tab in the active pane.': '選取目前窗格中的最後一個分頁。',
    'Split the active pane to the right with a new terminal.':
        '在目前窗格右側分割並開啟新的終端機。',
    'Split the active pane downward with a new terminal.': '在目前窗格下方分割並開啟新的終端機。',
    'Merge the active pane back into its sibling.': '將目前窗格合併回相鄰窗格。',
    'Global': '全域',
    'Workspace': '工作區',
    'Tabs': '分頁',
    'Panes': '窗格',
    'Open Automations': '開啟自動化',
    'Open Workspace in Zed': '在 Zed 中開啟工作區',
    'Toggle Sidebar': '切換側邊欄',
    'Go Back': '返回',
    'Go Forward': '前進',
    'Find in Files': '在檔案中尋找',
    'Find in Terminal': '在終端機中尋找',
    'Toggle Terminal Composer': '切換終端機輸入區',
    'Replace in Files': '在檔案中取代',
    'Save File': '儲存檔案',
    'New Terminal Tab': '新增終端機分頁',
    'Close Tab': '關閉分頁',
    'Next Tab': '下一個分頁',
    'Previous Tab': '上一個分頁',
    'Split Right': '向右分割',
    'Split Down': '向下分割',
    'Close Split': '關閉分割窗格',
    'Select Folder': '選擇資料夾',
    'Select Parent Folder': '選擇父資料夾',
    'Choose Parent Folder': '選擇父資料夾',
    'Choose an existing local folder or clone a Git repository from a URL.':
        '選擇現有本機資料夾，或從 URL 複製 Git 儲存庫。',
    'Local Folder': '本機資料夾',
    'Clone From URL': '從 URL 複製',
    'Project Path': '專案路徑',
    'Destination Folder': '目的地資料夾',
    'Display Name (Optional)': '顯示名稱（選填）',
    'Alera will detect whether the folder is a Git repository. Non-Git folders only get a primary workspace.':
        'Alera 會偵測資料夾是否為 Git 儲存庫；非 Git 資料夾只會建立主要工作區。',
    'Alera will run git clone into the destination folder and register the cloned repository.':
        'Alera 會在目的地資料夾執行 git clone，並登錄複製完成的儲存庫。',
    'Native folder picker is not available; paste path manually.':
        '無法使用系統資料夾選擇器，請手動貼上路徑。',

    'Edit': '編輯',
    'Back': '返回',
    'Next': '下一步',
    'Done': '完成',
    'Add': '新增',
    'Start': '啟動',
    'Stop': '停止',
    'Select': '選擇',
    'Confirm': '確認',
    'Update': '更新',
    'Download': '下載',
    'Approve': '核准',
    'Reject': '拒絕',
    'Open Usage': '開啟用量',
    'Remove Token': '移除 Token',
    'Check For Updates': '檢查更新',
    'Resume': '繼續執行',
    'Extend': '延長',
    'Run': '執行',
    'Continue Active': '繼續目前執行',
    'Cancel Active': '取消目前執行',
    'Save Automation': '儲存自動化',
    'Run Now': '立即執行',
    'Restore': '還原',
    'Trash': '移至垃圾桶',
    'New Automation': '新增自動化',
    'Post Comment': '發佈留言',
    'Link': '連結',
    'Create Stack': '建立 Stack',
    'Link Existing Pull Request': '連結現有 Pull Request',
    'Create Pull Request': '建立 Pull Request',
    'Test Command': '測試指令',
    'Add CCS Profile': '新增 CCS 設定檔',
    'Save Profile': '儲存設定檔',
    'Check Environment': '檢查環境',
    'Take Back This Terminal': '取回此終端機',
    'Take Back All Terminals': '取回所有終端機',
    'Take Back': '取回',
    'Add Git Project': '新增 Git 專案',
    'Use Custom Command': '使用自訂指令',
    'Continue Manually': '手動繼續',
    'Open Workspace': '開啟工作區',
    'Retry Agent': '重試 Agent',
    'Create And Start Agent': '建立並啟動 Agent',
    'Cancel Download': '取消下載',
    'Remove Model': '移除模型',
    'Update Runtime': '更新 Runtime',
    'Kill All': '全部終止',
    'A reading diff is an AI-guided, non-applicable abbreviation of the original diff.':
        '閱讀 Diff 是由 AI 引導產生、不可直接套用的原始 Diff 精簡版本。',
    'This overview explains the behavioral changes selected while condensing the diff. It does not identify bugs or security findings. Open Condensed Diff to inspect the retained source changes, or return to the original diff for the complete patch.': '此總覽說明精簡 Diff 時保留的行為變更，不代表錯誤或資安檢查結果。請開啟「精簡 Diff」檢視保留的原始碼變更，或返回原始 Diff 查看完整 Patch。',
    'Cached Result': '快取結果',
    'Preparing reading diff': '正在準備閱讀 Diff',
    'Loading cached reading diff': '正在載入快取的閱讀 Diff',
    'Loading the immutable diff and splitting it at safe boundaries.':
        '正在載入不可變更的 Diff，並於安全邊界分割。',
    'Using a previously generated and validated result.': '使用先前已產生並驗證的結果。',
    'The agent is proposing safe elisions; Rust validates the plan.':
        'Agent 正在提出安全的省略方案；Rust 會驗證該方案。',
    'Rust rejected the plan; the agent is replacing it once.':
        'Rust 已拒絕此方案；Agent 正在重新產生一次替代方案。',
    'Rust is merging the validated chunks into the final reading diff.':
        'Rust 正在將已驗證的區塊合併為最終閱讀 Diff。',
    'Reading diff generation failed': '閱讀 Diff 產生失敗',
    'This manually runs the configured AI Assist agent and may consume subscription quota or other provider usage. The complete selected patch is provided, including portions hidden by preview truncation.': '這會手動執行已設定的 AI Assist Agent，可能消耗訂閱額度或其他 Provider 用量。系統會提供完整的所選 Patch，包括預覽截斷而隱藏的部分。',
    'The result opens with a behavioral overview and a condensed, non-applicable diff. It is not a bug or security review.':
        '結果會先顯示行為總覽及不可直接套用的精簡 Diff；這不是錯誤或資安審查。',
    'Agent': 'Agent',
    'Model': '模型',
    'Effort': '推理強度',
    'Access': '存取權限',
    'Diff Size': 'Diff 大小',
    'Chunks': '區塊數',
    'Agent Default': 'Agent 預設值',
    'Repository Read Only': 'Repository 唯讀',
    'Diff Only': '僅限 Diff',
    'Could not prepare': '無法準備',
    'condensed diff': '精簡 Diff',

    'Generate Reading Diff': '產生閱讀 Diff',
    'Install Update': '安裝更新',
    'Restart Alera': '重新啟動 Alera',
    'Run Update': '執行更新',
    'View Output': '檢視輸出',
    'New Action': '新增動作',
    'Cancel Offer': '取消配對邀請',
    'Add Copy Rule': '新增複製規則',
    'Add Setup Command': '新增設定指令',
    'Use Repo File': '使用儲存庫檔案',
    'Plan': '規劃',
    'Bootstrap': '初始化',
    'Copy Output': '複製輸出',
    'Restart Terminal': '重新啟動終端機',
    'Reconnect': '重新連線',
    'Amend': '修改 Commit',

    'Run Precheck': '執行前置檢查',
    'Audited Draft Test': '稽核草稿測試',
    'Approve Exact Revision': '核准指定版本',
    'Skip': '略過',
    'Queue': '排入佇列',
    'Run Latest Once': '執行最新版本一次',
    'Force Parallel': '強制平行執行',
    'Pause Automation': '暫停自動化',
    'Choose what to do with active runs.': '選擇如何處理目前執行中的工作。',
    'Loading automation policy...': '正在載入自動化政策…',
    'Automation Policy Unavailable': '無法取得自動化政策',
    'Automation Permissions': '自動化權限',
    'Choose whether this profile may administer active definitions and whether it may execute them.':
        '選擇此設定檔是否可管理作用中的自動化定義，以及是否可執行它們。',
    'May Activate Or Edit Active Automations': '可啟用或編輯作用中的自動化',
    'Allow a managed agent using this profile to activate or edit an active definition.':
        '允許使用此設定檔的受管理 Agent 啟用或編輯作用中的自動化定義。',
    'May Execute Automations': '可執行自動化',
    'Opt this profile into scheduled and manual automation execution.':
        '允許此設定檔執行排程與手動自動化。',
    'Project Automation Policy Unavailable': '無法取得專案自動化政策',
    'Automation Policy': '自動化政策',
    'Repository declaration is read from alera.toml. Local approval can only restrict execution.':
        'Repository 宣告會從 alera.toml 讀取；本機核准只能進一步限制執行。',
    'Repository Declares Automations': 'Repository 宣告自動化',
    'The repository declares automation use in alera.toml.':
        'Repository 已在 alera.toml 宣告使用自動化。',
    'Add an automation declaration to alera.toml before execution.':
        '執行前請先在 alera.toml 新增自動化宣告。',
    'Require Local Approval': '需要本機核准',
    'Require an explicit human approval in addition to the repository declaration.':
        '除了 Repository 宣告外，還需要明確的人工作業核准。',
    'Local Approval Granted': '已授予本機核准',
    'Grant the local approval required by a restrictive project policy.':
        '授予限制性專案政策所要求的本機核准。',
    'Loading Automation Settings': '正在載入自動化設定',
    'Reading runtime automation settings.': '正在讀取執行環境的自動化設定。',
    'Automation Settings Unavailable': '無法取得自動化設定',
    'Automation History And Autostart': '自動化歷程與自動啟動',
    'Keep scheduled work available without a window and control local retention.':
        '讓排程工作在沒有視窗時仍可執行，並控制本機保留期限。',
    'Start Automations At Login': '登入時啟動自動化',
    'Start the persistent local automation host when you sign in. This is off by default.':
        '登入時啟動常駐的本機自動化 Host；預設為關閉。',
    'Run History Retention': '執行歷程保留期限',
    'Keep final runs for at most this many days.': '最多保留已完成執行紀錄這麼多天。',
    'Audit Retention': '稽核保留期限',
    'Keep automation audit events for at most this many days.':
        '最多保留自動化稽核事件這麼多天。',
    'Trash Retention': '垃圾桶保留期限',
    'Permanently remove trashed definitions after this many days.':
        '垃圾桶中的定義超過此天數後永久刪除。',
    'days': '天',

    'Notify On Success': '成功時通知',
    'Prompt Preview': '提示詞預覽',
    'Automation unavailable': '無法取得自動化',
    'Pause': '暫停',
    'Runs': '執行紀錄',
    'Audit': '稽核',
    'Slug': 'Slug',
    'Schedule': '排程',
    'Cron / time': 'Cron / 時間',
    'Target': '目標',
    'Policies': '政策',
    'Limits': '限制',
    'Tags': '標籤',
    'Revision': '版本',
    'Prompt': '提示詞',
    'Effective Policy': '生效政策',
    'Signature Timeline': '簽章時間軸',
    'Overlap': '重疊處理',

    'No active run or upcoming occurrence.': '目前沒有執行中或即將開始的工作。',
    'Scheduled occurrence': '排程執行',
    'No runs yet.': '尚無執行紀錄。',
    'No audit events yet.': '尚無稽核事件。',
    'Import Automations': '匯入自動化',
    'Edit Automation': '編輯自動化',
    'Project (Optional)': '專案（選填）',
    'Tag Ids (Comma-separated)': '標籤 ID（以逗號分隔）',
    'Prompt Template': '提示詞範本',
    'Five-field Cron': '五欄 Cron',
    'Run At (UTC)': '執行時間（UTC）',
    'IANA Timezone': 'IANA 時區',
    'Start At (Optional ISO-8601 UTC)': '開始時間（選填，ISO-8601 UTC）',
    'End At (Optional ISO-8601 UTC)': '結束時間（選填，ISO-8601 UTC）',
    'Maximum Scheduled Runs (Optional)': '最大排程執行次數（選填）',
    'Source Workspace': '來源工作區',
    'Tab': '分頁',
    'Agent Conversation ID': 'Agent 對話 ID',
    'Agent Profile': 'Agent 設定檔',
    'Source Branch': '來源 Branch',
    'Workspace Name Template': '工作區名稱範本',
    'Precheck Command (Optional)': '前置檢查指令（選填）',
    'Precheck Timeout (Seconds)': '前置檢查逾時（秒）',
    'Setup': '設定',
    'Misfire': '錯過排程',
    'Cleanup': '清理',
    'Queue Cap (Maximum 10)': '佇列上限（最多 10）',
    'Inactivity Timeout (Seconds)': '閒置逾時（秒）',
    'Heartbeat Interval (Seconds)': 'Heartbeat 間隔（秒）',
    'Retry Attempts (Maximum 3)': '重試次數（最多 3）',
    'Retry Backoff (Seconds)': '重試退避（秒）',
    'Circuit Failure Threshold': 'Circuit 失敗門檻',
    'Circuit Open (Seconds)': 'Circuit 開啟時間（秒）',
    'Select...': '請選擇…',
    'One-time': '單次',
    'Existing Tab': '現有分頁',
    'Fresh Tab': '新分頁',
    'Managed Workspace': '受管理工作區',
    'Name, slug, and prompt template are required.': '名稱、slug 與提示詞範本為必填。',
    'The existing tab requires workspace, tab, and conversation ids.':
        '現有分頁需要 workspace、tab 與 conversation ID。',
    'The selected target requires its ids.': '所選目標需要填寫對應 ID。',
    'Prompt template contains an unmatched closing delimiter.':
        '提示詞範本包含未配對的結束分隔符。',
    'Prompt template contains an unterminated variable.': '提示詞範本包含未結束的變數。',
    'Versioned JSON Catalog': '版本化 JSON Catalog',
    'Source Key To Local Id JSON': '來源 Key 到本機 ID 的 JSON',
    'Map every source key to an existing local id.': '將每個來源 Key 對應到既有的本機 ID。',
    'Automation created': '已建立自動化',
    'Automation saved': '已儲存自動化',
    'Automation cloned': '已複製自動化',
    'Automation catalog copied to clipboard': '自動化 Catalog 已複製到剪貼簿',
    'Automation catalog imported as drafts': '自動化 Catalog 已以草稿匯入',
    'Automation approved': '已核准自動化',
    'Automation run started': '自動化執行已開始',
    'Automation paused': '自動化已暫停',
    'Automation resumed': '自動化已繼續',
    'Automation cancellation requested': '已要求取消自動化',
    'Waiting run resumed': '等待中的執行已繼續',
    'Waiting run extended': '已延長等待中的執行',

    'Map Imported Targets': '對應匯入目標',
    'App First': '應用程式優先',
    'Terminal First': '終端機優先',
    'From Prompt': '從提示詞',
    'Manual': '手動',
    'Daily Activity': '每日活動',
    'Usage': '用量',
    'Local Host': '本機 Host',
    'Updating': '更新中',
    'Showing saved usage while new data loads in the background.':
        '正在背景載入新資料，同時顯示已儲存的用量資料。',
    'Update Failed': '更新失敗',
    '7 Days': '7 天',
    '30 Days': '30 天',
    '90 Days': '90 天',
    'Usage Unavailable': '無法取得用量',
    'No usage data is available for this host.': '此 Host 目前沒有可用的用量資料。',
    'Processed Tokens': '已處理 Token',
    'API-Equivalent Cost': 'API 等值成本',
    'Sessions': '工作階段',
    'Cached Input': '快取輸入',
    'Cache Savings': '快取節省',
    'Compared with full input rates': '與完整輸入費率相比',
    'Tokens read from Claude Code, Codex, and Grok Build transcripts on this host.':
        '從此 Host 上的 Claude Code、Codex 與 Grok Build 逐字稿讀取的 Token。',
    'Breakdown': '明細',
    'No Activity': '沒有活動',
    'No Daily Activity': '沒有每日活動',
    'Provider-reported costs': 'Provider 回報的成本',
    'Current model rates': '目前模型費率',
    'Cached model rates': '快取的模型費率',
    'Pricing unavailable': '無法取得定價',
    'Some model costs may be unavailable because pricing could not be loaded.':
        '由於無法載入定價，部分模型成本可能無法取得。',

    'Profiles': '設定檔',
    'Grouped': '分組',
    'Models': '模型',
    'Tokens': 'Token',
    'Cost': '成本',
    'Responses': '回應數',
    'Terminal unavailable': '終端機無法使用',
    'Restart Terminal?': '重新啟動終端機？',
    'This Device': '此裝置',
    'IP Address': 'IP 位址',
    'DNS Hostname': 'DNS 主機名稱',
    'Private Interface': '私有介面',
    'All changes for file': '此檔案的所有變更',
    'Amend Commit': '修改 Commit',
    'Overview': '總覽',
    'Condensed Diff': '精簡 Diff',
    'What Changed': '變更內容',
    'Chunk Analysis': '區塊分析',
    'Side-by-Side View': '並排檢視',
    'Unified View': '統一檢視',
    'Switch to Side-by-Side View': '切換至並排檢視',
    'Switch to Unified View': '切換至統一檢視',
    'Original': '原始',
    'Modified': '修改後',
    'Empty': '空白',
    'Deleted': '已刪除',
    'Refresh Worktrees': '重新整理工作樹',
    'Confirm Before Closing Busy Terminals': '關閉忙碌終端機前確認',
    'Ask for confirmation before closing tabs with running processes or active agents.':
        '關閉仍有執行中程序或作用中代理程式的分頁前先要求確認。',
    'Close Unsaved Editor?': '關閉未儲存的編輯器？',
    'Close Unsaved Editors?': '關閉未儲存的編輯器分頁？',
    'Stop Running Agent?': '停止執行中的代理程式？',
    'Stop Running Command?': '停止執行中的命令？',
    'Close Busy Terminals?': '關閉忙碌的終端機？',
    'Stop And Close': '停止並關閉',
    'Closing will end the command and every process it started. Anything halfway through will stay halfway through.':
        '關閉後會終止此命令及它啟動的所有程序。任何尚未完成的操作都會停在目前狀態。',
    'New Branch': '新分支',
    'Existing Branch': '現有分支',
    'Loading files...': '正在載入檔案…',
    'No file changes': '沒有檔案變更',
    'None': '無',
    'Project': '專案',
    'Section': '區段',
    'All': '全部',
    'Default': '預設',
    'Non-Default': '非預設',
    'Copy Command': '複製指令',
    'Clone': '建立副本',
    'Collapse': '收合',
    'Expand': '展開',
    'Fonts': '字型',
    'No themes found.': '找不到佈景主題。',
    'Block': '方塊',
    'Bar': '直線',
    'Underline': '底線',
    'New Host': '新增主機',
    'No copy rules': '尚無複製規則',
    'No setup commands': '尚無設定指令',
    'Overwrite existing destination': '覆寫現有目的地',
    'Remove Copy Rule': '移除複製規則',
    'Remove Setup Command': '移除設定指令',
    'Delete Device': '刪除裝置',
    'Rename Device': '重新命名裝置',
    'Revoke Device': '撤銷裝置',
    'Select Runner': '選擇執行器',
    'Refresh models': '重新整理模型',
    'Refresh Models': '重新整理模型',
    'Refresh Personas': '重新整理 Persona',
    'Edit CCS Profile': '編輯 CCS 設定檔',
    'Remove CCS Profile': '移除 CCS 設定檔',
    'Reorder Agent Profile': '重新排序 Agent 設定檔',
    'Clone Profile': '複製設定檔',
    'New Profile': '新增設定檔',
    'Disable Shortcut': '停用快速鍵',
    'Close Image Preview': '關閉圖片預覽',
    'Close Terminal': '關閉終端機',
    'Copy Version': '複製版本',
    'Application Menu': '應用程式選單',
    'Refresh Usage': '重新整理用量',
    'Move Toolbar': '移動工具列',
    'Previous Match': '上一個符合項目',
    'Next Match': '下一個符合項目',
    'Close Search': '關閉搜尋',
    'Resource Manager': '資源管理員',
    'The runtime host is not responding. Use the host chip to restart it.':
        'Runtime Host 沒有回應。請使用 Host 狀態按鈕重新啟動。',
    'Total CPU across Alera and every terminal it spawned, as a share of everything this machine can run at once.':
        'Alera 與其啟動之所有終端機的 CPU 總用量，以此機器可同時執行的總容量為基準。',
    'Resident memory of Alera, the runtime host, and every terminal process.':
        'Alera、Runtime Host 與所有終端機程序目前占用的實體記憶體。',
    'Share of the machine memory these processes hold.': '這些程序占用整台機器記憶體的比例。',
    'Memory': '記憶體',
    'Measuring resource usage': '正在測量資源用量',
    'No terminal sessions are running': '目前沒有執行中的終端機工作階段',
    'remote': '遠端',
    'Unattributed Terminals': '未歸屬的終端機',
    'Kill Orphan Terminal': '終止孤立終端機',
    'Close Terminal Session': '關閉終端機工作階段',
    'App': '應用程式',
    'Runtime Host': 'Runtime Host',

    'Pane actions': '窗格操作',
    'Dismiss Error': '關閉錯誤',
    'New Workspace in This Project': '在此專案新增工作區',
    'Expand panel': '展開面板',
    'Collapse panel': '收合面板',
    'View Diff': '檢視 Diff',
    'Discard Changes': '捨棄變更',
    'Open Preview': '開啟預覽',
    'View options': '檢視選項',
    'Refresh Commits': '重新整理 Commits',
    'Previous match': '上一個符合項目',
    'Next match': '下一個符合項目',
    'Zoom out': '縮小',
    'Zoom in': '放大',
    'Hide outline': '隱藏大綱',
    'Replace in file': '在檔案中取代',
    'Replace match': '取代符合項目',
    'Delete Tag': '刪除標籤',
    'Commit Actions': 'Commit 操作',
    'Clear search results': '清除搜尋結果',
    'Move Up': '上移',
    'Move Down': '下移',
    'Remove Workspace': '移除工作區',
    'Open Pull Request Diff': '開啟 Pull Request Diff',
    'Open Comment': '開啟留言',
    'Open Comment Location': '開啟留言位置',
    'Match case': '區分大小寫',
    'Match whole word': '全字符合',
    'Use regular expression': '使用正規表示式',
    'Preserve case': '保留大小寫',
    'Replace all': '全部取代',
    'Open source file': '開啟來源檔',
    'Open Editor': '開啟編輯器',
    'Open Check': '開啟檢查',
    'Pull Request Actions': 'Pull Request 操作',
    'Checks': '檢查',
    'No checks reported': '尚未回報任何檢查結果',
    'Edit Pull Request': '編輯 Pull Request',
    'Open In Browser': '在瀏覽器中開啟',
    'Title': '標題',
    'Base Branch': '基底分支',
    'The base branch is managed by the pull request stack.':
        '基底分支由 Pull Request Stack 管理。',
    'Draft': '草稿',
    'Merged': '已合併',
    'Closed': '已關閉',
    'Comments': '留言',
    'Start Conversation': '開始對話',
    'Add Comment': '新增留言',
    'Add a comment': '新增留言',
    'No comments yet': '目前還沒有留言',
    'Resolved': '已解決',
    'No details available': '沒有可用的詳細資訊',
    'Workflow': '工作流程',
    'Event': '事件',
    'Description': '說明',
    'Started': '開始時間',
    'Completed': '完成時間',
    '#123 or pull request URL': '#123 或 Pull Request URL',
    'Suggested pull request': '建議的 Pull Request',
    'CLI not found': '找不到 CLI',
    'Not authenticated': '尚未驗證身分',
    'No remote': '沒有 Remote',
    'This repository has no remote to detect a provider from.':
        '此 Repository 沒有 Remote，無法據此偵測 Provider。',
    'Provider not detected': '未偵測到 Provider',
    'Could not detect the git hosting provider. Set it in project settings.':
        '無法偵測 Git Hosting Provider，請在專案設定中指定。',
    'Unsupported provider': '不支援的 Provider',
    'This hosting provider is not supported yet.': '目前尚不支援此 Hosting Provider。',

    'Create Options': '建立選項',
    'Automations unavailable': '無法取得自動化',
    'No automations': '沒有自動化',
    'Select an automation': '選擇自動化',
    'State': '狀態',
    'Profile': '設定檔',
    'Tag': '標籤',

    'No automation details available.': '沒有可用的自動化詳細資料。',
    'Create a schedule to run approved work in a runtime-owned target.':
        '建立排程，在執行環境管理的目標上執行已核准工作。',
    'Choose an automation to inspect its schedule, target, and runs.':
        '選擇自動化以檢視排程、目標與執行紀錄。',
    'Linked workspaces require a Git project. Add one to get started.':
        '連結工作區需要 Git 專案；請先新增專案。',
    'Create an action to replace selected text with AI.': '建立動作，使用 AI 取代選取文字。',
    'Choose an action or create a new one.': '選擇既有動作或建立新動作。',
    'This action and its settings will be removed.': '此動作及其設定將被移除。',
    'No usage was found in this range.': '此範圍內沒有用量資料。',
    'No Claude Code, Codex, or Grok Build usage was found in this range.':
        '此範圍內沒有 Claude Code、Codex 或 Grok Build 用量資料。',
    'Default workspace': '預設工作區',
    'Pinned workspace': '已釘選工作區',
    'Workspace path copied': '已複製工作區路徑',
    'Workspace slept': '工作區已休眠',
    'No git remote configured for this workspace.': '此工作區未設定 Git remote。',
    'Could not open the browser.': '無法開啟瀏覽器。',
    'This editor tab has no file.': '此編輯器分頁沒有檔案。',
    'No commits yet': '尚無 Commit',
    'Could not load diff.': '無法載入 Diff。',
    'No diff available.': '沒有可用的 Diff。',
    'No stashes to pop': '沒有可套用的 Stash',

    'Edit Message': '編輯訊息',
    'Message': '訊息',
    'Saving': '儲存中',
    'Save And Restart': '儲存並重新開始',
    'Save Message': '儲存訊息',
    'Saving stops the active turn and replaces this message and all later responses. Files and actions already performed are not undone. Queued messages remain paused.':
        '儲存會停止目前的回合，並取代此訊息及之後的所有回覆。已執行的檔案變更與操作不會復原，佇列中的訊息會維持暫停。',
    'The message could not be saved. Your edit is still here.':
        '無法儲存訊息，你的編輯內容仍保留在這裡。',
    'Queued': '已排入佇列',
    'Paused': '已暫停',
    'Resume Queue': '繼續佇列',
    'Pause Queue': '暫停佇列',
    'Collapse Queue': '收合佇列',
    'Expand Queue': '展開佇列',
    'Check Delivery': '檢查傳送狀態',
    'Attachment': '附件',
    'Sending': '傳送中',
    'Steer': '引導',
    'Remove Queued Message': '移除佇列訊息',
    'Message Actions': '訊息操作',

    'Ship Changes?': '送出變更？',
    'Ship only staged changes, or stage all changes first and include them in the commit.':
        '僅送出已暫存的變更，或先暫存所有變更並將其納入 Commit。',
    'Ship Staged Changes': '送出已暫存變更',
    'Ship All Changes': '送出所有變更',
    'Runtime Still Has Work': '執行環境仍有工作進行中',
    'Quit And Leave Runtime Open': '結束 Alera 並讓執行環境保持運作',
    'Force Stop And Quit': '強制停止並結束 Alera',
    'Force Stop Runtime': '強制停止執行環境',
    'Force Stop': '強制停止',
    'The runtime still has active work.': '執行環境仍有進行中的工作。',
    'The runtime still has active work. Force stop terminates them.':
        '執行環境仍有進行中的工作。強制停止會終止這些工作。',
    'The runtime still has active work. You can quit and leave the runtime running, or force stop it.':
        '執行環境仍有進行中的工作。你可以結束 Alera 並讓執行環境保持運作，或強制停止它。',

    'Active Run': '執行中工作',
    'Update With': '使用以下方式更新',

    // Settings sections.
    'Configuration Sync': '設定同步',
    'Account': '帳號',
    'Application': '應用程式',
    'Agents': '代理程式',
    'Quotas': '配額',
    'AI Assist': 'AI 輔助',
    'AI Dictation': 'AI 聽寫',
    'Text Actions': '文字操作',
    'Editor': '編輯器',
    'Terminal': '終端機',
    'Keyboard': '鍵盤',
    'Projects': '專案',
    'Mobile Devices': '行動裝置',
    'Remote Hosts': '遠端主機',
    'Agent Profiles': '代理程式設定檔',

    // Settings groups and common rows.
    'Language': '語言',
    'App Language': '應用程式語言',
    'Storage': '儲存空間',
    'Safety': '安全性',
    'Desktop': '桌面',
    'Runtime': '執行環境',
    'Diagnostics': '診斷',
    'Updates': '更新',
    'Support': '支援',
    'Identity': '身分',
    'Account unavailable': '無法取得帳號',
    'Your Alera identity protects cloud delivery and stays optional for local features.':
        '你的 Alera 身分可保護雲端傳送，本機功能仍可不登入使用。',
    'Continue With Google': '使用 Google 繼續',
    'Sign in through your default browser.': '透過預設瀏覽器登入。',
    'Continue With GitHub': '使用 GitHub 繼續',
    'Uses profile and verified email access only. Repository access is never requested.':
        '只會存取個人資料與已驗證的電子郵件，不會要求 Repository 存取權限。',
    'Alera Account': 'Alera 帳號',
    'Add another verified sign-in method to this account.':
        '為此帳號新增另一個已驗證的登入方式。',
    'Sign Out': '登出',
    'Stops cloud push delivery from this runtime until you sign in again.':
        '在你再次登入前，停止從此執行環境傳送雲端推播。',
    'Browser Sign In': '瀏覽器登入',
    'A provider authorization is waiting in your browser.':
        '瀏覽器中正等待 Provider 授權。',
    'Notifications are delivered only to mobile devices enrolled in this account.':
        '通知只會傳送到已加入此帳號的行動裝置。',
    'Enable Mobile Push': '啟用行動裝置推播',
    'Sign in before enabling cloud delivery.': '請先登入，再啟用雲端傳送。',
    'Attention Required': '需要注意',
    'Notify for waiting or blocked agents, decision gates, and escalations.':
        '當 Agent 等待中、受阻、需要決策或升級處理時通知。',
    'Agent Finished': 'Agent 已完成',
    'Notify when an agent finishes a turn.': 'Agent 完成一個回合時通知。',
    'Terminal Ended': '終端機已結束',
    'Notify when a terminal session exits or is closed.': '終端機工作階段結束或關閉時通知。',
    'Move this runtime to another account or remove your cloud identity.':
        '將此執行環境移轉到其他帳號，或移除你的雲端身分。',
    'Target Account ID': '目標帳號 ID',
    'Moving a runtime signs this installation out and requires authentication again.':
        '移轉執行環境會讓此安裝登出，之後需要重新驗證。',
    'Account ID': '帳號 ID',
    'Move This Runtime': '移轉此執行環境',
    'Transfer runtime ownership and its mobile subscriptions.':
        '移轉執行環境的擁有權及其行動裝置訂閱。',
    'Move Runtime': '移轉執行環境',
    'Delete Alera Account': '刪除 Alera 帳號',
    'Permanently removes provider identities, cloud sessions, subscriptions, and quota records.':
        '永久移除 Provider 身分、雲端工作階段、訂閱與配額紀錄。',
    'Delete Account': '刪除帳號',
    'This permanently removes your Alera cloud identity, active sessions, mobile subscriptions, and quota records. Recent sign-in may be required.':
        '這會永久移除你的 Alera 雲端身分、作用中的工作階段、行動裝置訂閱與配額紀錄。可能需要近期登入驗證。',

    'Mobile Push': '行動推播',
    'Ownership': '擁有權',
    'CLI And Skills': 'CLI 與技能',
    'Extra Skills': '額外技能',
    'Status Hooks': '狀態 Hook',
    'Behavior': '行為',
    'Providers': '供應商',
    'Credentials': '憑證',
    'Actions': '操作',
    'Generation': '生成',
    'Commit Messages': 'Commit 訊息',
    'Pull Request Details': 'Pull Request 詳細資料',
    'Reading Diffs': '閱讀差異',
    'Workspace Identity': '工作區識別',
    'Transcription': '轉錄',
    'Remote Transcription': '遠端轉錄',
    'Local Whisper Models': '本機 Whisper 模型',
    'Speech Processing': '語音處理',
    'Test AI Dictation': '測試 AI 聽寫',
    'Choose where speech is converted to text on this device.':
        '選擇要在此裝置的哪個位置將語音轉換成文字。',
    'Enable AI Dictation': '啟用 AI 聽寫',
    'Show microphone controls in supported composers.': '在支援的輸入區顯示麥克風控制項。',
    'Transcription Engine': '轉錄引擎',
    'Optional locale or language code. Leave blank for automatic detection.':
        '選填的地區或語言代碼；留空會自動偵測。',
    'Allow Online Speech Recognition': '允許線上語音辨識',
    'Windows may send microphone audio to Microsoft to create the transcription.':
        'Windows 可能會將麥克風音訊傳送給 Microsoft 以產生轉錄。',
    'The system recognizer may send microphone audio to its online speech service.':
        '系統辨識器可能會將麥克風音訊傳送到其線上語音服務。',
    'Install multiple multilingual models and select one for local transcription.':
        '可安裝多個多語言模型，並選擇一個用於本機轉錄。',
    'Optionally improve the transcript with the agent subscription configured for Speech Messages in AI Assist settings.':
        '可選擇使用 AI 輔助設定中「語音訊息」所設定的 Agent 訂閱來改善轉錄內容。',
    'Automatic Processing': '自動處理',
    'Raw text is always used if the selected agent is unavailable or fails.':
        '若所選 Agent 無法使用或處理失敗，會一律使用原始文字。',
    'Off': '關閉',
    'Clean Up': '整理',
    'Summarize': '摘要',
    'Local Whisper': '本機 Whisper',
    'Codex Subscription (Experimental)': 'Codex 訂閱（實驗性）',
    'OpenAI-Compatible API': 'OpenAI 相容 API',
    'System On-Device': '系統裝置端',
    'System Recognition': '系統語音辨識',
    'Record locally and transcribe with the selected Whisper model.':
        '在本機錄音，並使用所選 Whisper 模型轉錄。',
    'Use the experimental Codex app-server realtime API with your Codex subscription.':
        '使用 Codex 訂閱搭配實驗性的 Codex app-server realtime API。',
    'Send recordings to an OpenAI-compatible audio transcription endpoint.':
        '將錄音傳送到 OpenAI 相容的音訊轉錄端點。',
    'Use the platform recognizer only when it guarantees offline processing.':
        '僅在平台辨識器保證離線處理時使用。',
    'Use the platform speech service, which may process audio online.':
        '使用平台語音服務；音訊可能在線上處理。',
    'Send recordings to Codex or an OpenAI-compatible speech API. Transcription endpoints do not use reasoning effort.':
        '將錄音傳送到 Codex 或 OpenAI 相容語音 API；轉錄端點不使用推理強度。',
    'Runtime Update Required': '需要更新執行環境',
    'Restart Alera to replace the running sidecar before configuring remote transcription.':
        '設定遠端轉錄前，請重新啟動 Alera 以替換正在執行的 sidecar。',
    'Allow Remote Audio Processing': '允許遠端音訊處理',
    'Recordings may leave this device and are deleted locally after transcription.':
        '錄音可能會離開此裝置，並在轉錄完成後從本機刪除。',
    'Realtime Model': 'Realtime 模型',
    'Optional Codex realtime model override. Leave blank to use the subscription default. This Codex API is experimental.':
        '可選擇覆寫 Codex realtime 模型；留空使用訂閱預設值。此 Codex API 為實驗性。',
    'Subscription default': '訂閱預設值',
    'Base URL': 'Base URL',
    'Base API URL. Alera appends /audio/transcriptions when needed and preserves query parameters.': 'API Base URL。Alera 會在需要時附加 /audio/transcriptions，並保留 query parameters。',
    'Speech-to-text model accepted by the configured API.':
        '設定的 API 所接受的語音轉文字模型。',
    'Request Timeout': '請求逾時',
    'Maximum time allowed for remote transcription.': '遠端轉錄允許的最長時間。',
    'API Token': 'API Token',
    'Checking saved token...': '正在檢查已儲存的 Token…',
    'The saved token belongs to another API origin. Replace it before transcribing.':
        '已儲存的 Token 屬於另一個 API origin；轉錄前請先替換。',
    'A token is stored for this API origin.': '此 API origin 已儲存 Token。',
    'No token is stored. Tokenless local APIs are also supported.':
        '尚未儲存 Token；也支援不需要 Token 的本機 API。',
    'Replace saved token': '替換已儲存的 Token',
    'Replace Token': '替換 Token',
    'Save Token': '儲存 Token',
    'Test Transcript': '測試轉錄',
    'Record a short sample with the current configuration and review the transcript here.':
        '使用目前設定錄製短音訊，並在此檢視轉錄結果。',
    'Your test transcription appears here': '測試轉錄結果會顯示在這裡',
    'Enable AI Dictation before testing.': '測試前請先啟用 AI 聽寫。',
    'Restart Alera to update the runtime before testing remote transcription.':
        '測試遠端轉錄前，請重新啟動 Alera 以更新執行環境。',
    'Allow remote audio processing before testing this engine.':
        '測試此引擎前，請先允許遠端音訊處理。',
    'Select the microphone, speak, then select Stop Dictation.':
        '選取麥克風、開始說話，完成後選取「停止聽寫」。',
    'Queue Download': '排入下載佇列',
    'Selected': '已選取',
    'Use Model': '使用模型',
    'Queued. This download starts when the active transfer finishes.':
        '已排入佇列；目前的傳輸完成後會開始下載。',
    'Verifying downloaded model...': '正在驗證下載的模型…',
    'The model download failed.': '模型下載失敗。',
    'Installed and selected.': '已安裝並選取。',
    'Installed on this device.': '已安裝在此裝置。',
    's': '秒',

    'Typography': '字型',
    'Cursor': '游標',
    'Appearance': '外觀',
    'Interaction': '互動',
    'Advanced': '進階',
    'Mobile Gateway': '行動閘道',
    'Link A Device': '連結裝置',
    'Active Pairing Offers': '有效的配對邀請',
    'Paired Devices': '已配對裝置',
    'Indentation': '縮排',
    'Autosave': '自動儲存',
    'Tab Size': 'Tab 寬度',
    'Theme Preset': '主題預設',
    'Autosave Delay': '自動儲存延遲',
    'Follow System': '跟隨系統',
    'English': 'English',
    '繁體中文': '繁體中文',

    // Settings descriptions.
    'Language used by the Alera interface.': 'Alera 介面使用的語言。',
    'Follow the system language or choose a language for Alera.':
        '跟隨系統語言，或為 Alera 指定語言。',
    'Review, download and upload configuration across your devices.':
        '檢視、下載與上傳不同裝置間的設定。',
    'Identity, mobile push and runtime ownership.': '身分、行動推播與執行環境擁有權。',
    'Storage, safety, runtime, diagnostics and updates.':
        '儲存空間、安全性、執行環境、診斷與更新。',
    'Agent hooks, notifications and Alera skills.': '代理程式 Hook、通知與 Alera 技能。',
    'Provider usage, Claude profiles and credential environment.':
        '供應商用量、Claude 設定檔與憑證環境。',
    'Local agent assistance for commits, pull requests, diffs, workspace identity, and speech.':
        '用於 Commit、Pull Request、差異、工作區識別與語音的本機代理程式輔助。',
    'Local, Codex subscription, and OpenAI-compatible speech-to-text.':
        '本機、Codex 訂閱與 OpenAI 相容的語音轉文字。',
    'Create reusable replacements for selected text.': '為選取的文字建立可重複使用的替換操作。',
    'Code editor defaults.': '程式碼編輯器預設值。',
    'Appearance defaults for new terminal sessions.': '新終端機工作階段的外觀預設值。',
    'Shortcuts and key bindings.': '快速鍵與按鍵綁定。',
    'Per-project workspace setup.': '各專案的工作區設定。',
    'Pair and manage the mobile companion app.': '配對並管理行動版輔助應用程式。',
    'SSH runtime targets.': 'SSH 執行目標。',
    'Launch configurations orchestration can dispatch to.': '可由協調流程派送的啟動設定。',
    'Syntax highlighting defaults for editor tabs.': '編輯器分頁的語法醒目提示預設值。',
    'Defaults used by editor tabs.': '編輯器分頁使用的預設值。',
    'Spaces inserted when pressing tab.': '按下 Tab 時插入的空白數。',
    'Save dirty editor tabs after they have been idle.': '編輯器分頁閒置後自動儲存未儲存的變更。',
    'Automatically save editor changes after a pause.': '暫停操作一段時間後自動儲存編輯器變更。',
    'Idle time before saving editor changes.': '儲存編輯器變更前的閒置時間。',
    'Search and select a syntax highlighting theme.': '搜尋並選擇語法醒目提示主題。',
    'Search syntax themes': '搜尋語法主題',
  };
}

class AleraLocalizationsDelegate
    extends LocalizationsDelegate<AleraLocalizations> {
  const AleraLocalizationsDelegate();

  @override
  bool isSupported(Locale locale) =>
      locale.languageCode == 'en' || locale.languageCode == 'zh';

  @override
  Future<AleraLocalizations> load(Locale locale) =>
      SynchronousFuture<AleraLocalizations>(AleraLocalizations(locale));

  @override
  bool shouldReload(AleraLocalizationsDelegate old) => false;
}

extension AleraLocalizationContext on BuildContext {
  AleraLocalizations get aleraLocalizations =>
      Localizations.of<AleraLocalizations>(this, AleraLocalizations) ??
      const AleraLocalizations(Locale('en'));

  String tr(String source) => aleraLocalizations.translate(source);
}
