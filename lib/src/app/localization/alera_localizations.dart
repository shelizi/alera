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
    return null;
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
    'Notify On Success': '成功時通知',
    'Prompt Preview': '提示詞預覽',
    'No active run or upcoming occurrence.': '目前沒有執行中或即將開始的工作。',
    'Scheduled occurrence': '排程執行',
    'No runs yet.': '尚無執行紀錄。',
    'No audit events yet.': '尚無稽核事件。',
    'Import Automations': '匯入自動化',
    'Map Imported Targets': '對應匯入目標',
    'App First': '應用程式優先',
    'Terminal First': '終端機優先',
    'From Prompt': '從提示詞',
    'Manual': '手動',
    'Daily Activity': '每日活動',
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
    'Create Options': '建立選項',
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
