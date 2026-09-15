part of 'alera_localizations.dart';

const Map<String, String> _traditionalChineseSettingsExtra = <String, String>{
  // Settings navigation and application.
  'Workspaces': '工作區',
  'Workspace Directory': '工作區目錄',
  'Where new linked workspaces are created on disk. Existing workspaces are not moved. Leave empty to use the default (~/.alera/workspaces).':
      '新建立的連結工作區在磁碟上的存放位置。既有工作區不會移動。留空會使用預設位置（~/.alera/workspaces）。',
  'Automatic archiving for inactive workspaces.': '自動封存閒置的工作區。',
  'Auto-Archive After Inactivity': '閒置後自動封存',
  'Days without activity before a workspace moves to the Archived section. Set to 0 to keep workspaces listed.':
      '工作區多久未活動後移至「已封存」區段。設為 0 可讓工作區持續顯示。',
  'Confirmation prompts for destructive workspace actions.':
      '針對破壞性工作區操作顯示確認提示。',
  'Confirm Project Removal': '確認移除專案',
  'Ask before unregistering a project and deleting its workspace metadata.':
      '取消註冊專案並刪除其工作區中繼資料前先詢問。',
  'Confirm Workspace Removal': '確認移除工作區',
  'Always required because removal closes all tabs, stops running processes, and discards unsaved changes.':
      '移除會關閉所有分頁、停止執行中的程序並捨棄未儲存變更，因此一律需要確認。',
  'Tray icon and dock or taskbar badge while Alera is running.':
      'Alera 執行時的系統匣圖示與 Dock／工作列徽章。',
  'Show Tray Icon': '顯示系統匣圖示',
  'Keep Alera in the menu extra (macOS), notification area (Windows), or status bar (Ubuntu). Closing the window hides it; Quit from the tray or the app menu exits.': '讓 Alera 保留在 macOS 選單列、Windows 通知區或 Ubuntu 狀態列。關閉視窗只會隱藏；從系統匣或應用程式選單選擇結束才會退出。',
  'Show Dock Badge': '顯示 Dock／工作列徽章',
  'Show how many agents are waiting for review on the Dock, taskbar, or Ubuntu Dock.':
      '在 Dock、工作列或 Ubuntu Dock 顯示等待檢閱的 Agent 數量。',
  'Show Tray Badge': '顯示系統匣徽章',
  'Draw how many agents are waiting for review onto the tray icon itself. Linux only; macOS and Windows show that count on the Dock or taskbar.':
      '直接在系統匣圖示顯示等待檢閱的 Agent 數量。僅限 Linux；macOS 與 Windows 會顯示在 Dock 或工作列。',
  'Compact review and CI status for workspaces backed by a hosted Git repository.':
      '顯示由託管 Git Repository 支援之工作區的精簡檢閱與 CI 狀態。',
  'Show Pull Request Status': '顯示 Pull Request 狀態',
  'Show draft, ready, running, failed, merged, and closed state beside each workspace. Alera batches GitHub workspaces into one refresh per repository.':
      '在每個工作區旁顯示草稿、就緒、執行中、失敗、已合併與已關閉狀態。Alera 會依 Repository 合併 GitHub 工作區的重新整理。',
  'Notify When Checks Fail': '檢查失敗時通知',
  'Show one native notification when a pull request enters a failed-check state. Enabling this keeps the lightweight monitor active while Alera is hidden.':
      'Pull Request 進入檢查失敗狀態時顯示一次原生通知。啟用後，即使 Alera 隱藏仍會維持輕量監控。',
  'Lifecycle of the local runtime host that owns terminal sessions.':
      '管理終端機工作階段之本機 Runtime Host 的生命週期。',
  'Keep Computer Awake': '保持電腦喚醒',
  'Prevents idle sleep and display sleep while Alera is running. Closing the lid still follows this device\'s power settings.':
      'Alera 執行時防止閒置睡眠與螢幕休眠。闔上螢幕仍依裝置電源設定處理。',
  'Keep Runtime Open When App Quits': '應用程式結束後保持 Runtime 執行',
  'Leave the app-launched sidecar running after a clean quit. Persistent CLI runtimes are never stopped by quitting, and unexpected exits always leave the host up.':
      '正常結束應用程式後，保留由應用程式啟動的 Sidecar。持久型 CLI Runtime 不會因退出而停止，非預期退出也會保留 Host。',
  'Empty Host Shutdown': '空閒 Host 關閉',
  'Seconds to keep the host alive after the app closes with no running sessions.':
      '應用程式關閉且沒有執行中工作階段時，Host 繼續存活的秒數。',
  'Detached Session Shutdown': '分離工作階段關閉',
  'Seconds to keep detached running sessions alive after the app closes.':
      '應用程式關閉後，分離且仍在執行的工作階段繼續存活的秒數。',

  // Diagnostics, support, and updates.
  'Alera keeps rotating log files on this computer so an error can be investigated after it happens.':
      'Alera 會在這台電腦上輪替保存 Log，方便在錯誤發生後進行調查。',
  'Open Logs Folder': '開啟 Log 資料夾',
  'Export Diagnostics': '匯出診斷資料',
  'Save a zip with app and runtime logs plus version details.':
      '將應用程式與 Runtime Log 及版本資訊儲存為 ZIP。',
  'Log Level': 'Log 等級',
  'Send Crash Reports': '傳送當機報告',
  'Send crashes to Sentry, an external service. Off by default; enable only if you want to share crash diagnostics.':
      '將當機資訊傳送到外部服務 Sentry。預設關閉；只有在你願意分享當機診斷資料時才啟用。',
  'Could not open the logs folder.': '無法開啟 Log 資料夾。',
  'Zip Archive': 'ZIP 壓縮檔',
  'Diagnostics exported.': '診斷資料已匯出。',
  'Support Alera': '支持 Alera',
  'Star Alera on GitHub': '在 GitHub 為 Alera 加星',
  'Star': '加星',
  'Starring…': '正在加星…',
  'Try again': '再試一次',
  'Thanks for starring Alera': '感謝你在 GitHub 為 Alera 加星',
  'Thanks for the support!': '感謝你的支持！',
  'Update status': '更新狀態',
  'Checking for updates': '正在檢查更新',
  'No update available': '目前沒有可用更新',
  'Manual update available': '有可手動安裝的更新',
  'Update available': '有可用更新',
  'Downloading update': '正在下載更新',
  'Installing update': '正在安裝更新',
  'Restarting Alera': '正在重新啟動 Alera',
  'Restart Alera': '重新啟動 Alera',
  'Update failed': '更新失敗',
  'Checking': '檢查中',
  'Check for Updates': '檢查更新',
  'Download Manually': '手動下載',
  'Installation Guide': '安裝指南',
  'Update Alera': '更新 Alera',
  'The update runs here. Answer any prompt in the terminal.':
      '更新會在這裡執行；若終端機出現提示，請直接回應。',
  'Update checks are disabled in this privacy portable build.':
      '此隱私 Portable 版本已停用更新檢查。',
  'Update handoff complete. Alera will restart shortly.':
      '更新交接完成，Alera 即將重新啟動。',
  'Restart Alera to load any update installed by the command.':
      '重新啟動 Alera 以載入由命令安裝的更新。',
  'Restarting Alera.': '正在重新啟動 Alera。',
  'No update index is published yet.': '尚未發布更新索引。',
  'Alera is up to date.': 'Alera 已是最新版本。',

  // Agent profiles and managed options.
  'How this agent is launched for a dispatched task.':
      '設定此 Agent 接收到派送工作時的啟動方式。',
  'Adapter Type': 'Adapter 類型',
  'Command Preview': '命令預覽',
  'The host quotes these arguments for the actual platform shell.':
      'Host 會依實際平台 Shell 正確引用這些參數。',
  'Routing': '路由',
  'Signals the orchestrator reads when planning a run.':
      'Orchestrator 規劃執行時會讀取的訊號。',
  'Prompt Delivery': '提示詞傳遞方式',
  'Launch Mode': '啟動模式',
  'Managed': '受管理',
  'Command': '命令',
  'Exact Model ID': '指定模型 ID',
  'Use a model ID that is not in the discovered list.': '使用不在已偵測清單中的模型 ID。',
  'Persona': 'Persona',
  'Select a known agent persona or enter an exact name.':
      '選擇已知的 Agent Persona，或輸入確切名稱。',
  'Exact Persona': '指定 Persona',
  'Use a persona name that is not in the discovered list.':
      '使用不在已偵測清單中的 Persona 名稱。',
  'Managed Options': '受管理選項',
  'Alera builds the interactive command from these agent-specific settings.':
      'Alera 會依這些 Agent 專屬設定組合互動式命令。',
  'Leave empty to use the agent default.': '留空以使用 Agent 預設值。',
  'Reasoning Effort': '推理強度',
  'Plan Mode Reasoning Effort': 'Plan Mode 推理強度',
  'Applies only while Codex is in plan mode, which is entered with Shift+Tab or /plan. Codex has no way to start there.':
      '僅在 Codex 進入 Plan Mode 時套用；可透過 Shift+Tab 或 /plan 進入。Codex 無法直接以此模式啟動。',
  'Sandbox': 'Sandbox',
  'Approval Policy': '核准政策',
  'Web Search': '網路搜尋',
  'Allow Codex to search the web.': '允許 Codex 搜尋網路。',
  'Bypass All Protections': '略過所有保護',
  'Bypass both approval prompts and sandbox isolation.':
      '同時略過核准提示與 Sandbox 隔離。',
  'Allow Skip Permissions': '允許略過權限',
  'Mode': '模式',
  'Context': 'Context',
  'Allow All': '全部允許',
  'Allow tools and paths without individual prompts.': '允許工具與路徑，不需逐項提示。',
  'Maximum AI Credits': 'AI Credits 上限',
  'Maximum Autopilot Continues': 'Autopilot 自動繼續上限',
  'Do Not Ask User': '不要詢問使用者',
  'Continue without asking the user for input.': '不詢問使用者輸入並繼續執行。',
  'Permission Mode': '權限模式',
  'Review Mode': '檢閱模式',
  'Trust Workspace': '信任工作區',
  'Trust the workspace without an interactive prompt.': '不顯示互動提示，直接信任工作區。',
  'Skip Permissions': '略過權限檢查',
  'Run without Antigravity permission checks.': '執行時略過 Antigravity 權限檢查。',
  'Enable the Antigravity sandbox.': '啟用 Antigravity Sandbox。',
  'Auto Approve': '自動核准',
  'Approve OpenCode actions automatically.': '自動核准 OpenCode 操作。',
  'Thinking': '思考',
  'Project Trust': '專案信任',
  'Fast Mode': '快速模式',
  'Prefer lower latency responses.': '優先使用較低延遲的回應。',
  'Sandbox Devin exec-tool processes where supported.':
      '在支援的平台上將 Devin exec-tool 程序置於 Sandbox。',
  'Disable Web Search': '停用網路搜尋',
  'Disable Grok Build web search and web fetch tools.':
      '停用 Grok Build 的網路搜尋與 Web Fetch 工具。',
  'Resume Latest Session': '繼續最近工作階段',
  'Ignore Additional Directories': '忽略額外目錄',
  'Do not load additional directories configured by fx.': '不要載入 fx 設定的額外目錄。',
  'Record Session': '記錄工作階段',
  'Agent profiles unavailable': '無法取得 Agent 設定檔',
  'No agent profiles': '沒有 Agent 設定檔',
  'Declare a profile to let a run dispatch work to it.': '建立設定檔，讓執行工作可以派送給它。',
  'Agent profile order could not be saved': '無法儲存 Agent 設定檔順序',
  'Confirm Reduced Protections': '確認降低保護',
  'Agent profile saved': 'Agent 設定檔已儲存',
  'Test Agent Profile': '測試 Agent 設定檔',
  'The profile command runs here. It does not receive a dispatched task.':
      '設定檔命令會在這裡執行，但不會收到派送工作。',
  'Default agent profile updated': '預設 Agent 設定檔已更新',
  'Agent profile cloned': 'Agent 設定檔已複製',

  // Agent integration settings.
  'Alera CLI And Skills': 'Alera CLI 與 Skills',
  'Register the CLI command and install agent instructions.':
      '註冊 CLI 命令並安裝 Agent 操作指引。',
  'Alera CLI Command': 'Alera CLI 命令',
  'Register the Alera command on PATH for terminals and agents.':
      '將 Alera 命令註冊到 PATH，供終端機與 Agent 使用。',
  'All Alera Skills': '所有 Alera Skills',
  'Install or update CLI and orchestration skills. Reapplies selected status hooks.':
      '安裝或更新 CLI 與 Orchestration Skills，並重新套用已選擇的狀態 Hooks。',
  'Alera CLI Skill': 'Alera CLI Skill',
  'Install the Codex skill that teaches agents to use the Alera CLI.':
      '安裝教導 Agent 使用 Alera CLI 的 Codex Skill。',
  'Alera Orchestration Skill': 'Alera Orchestration Skill',
  'Install or update orchestration and reapply selected status hooks.':
      '安裝或更新 Orchestration，並重新套用已選擇的狀態 Hooks。',
  'Install optional skills for specialized Alera workflows.':
      '安裝適用於特定 Alera 工作流程的選用 Skills。',
  'Agent Profiles Skill': 'Agent Profiles Skill',
  'Research models and design, manage, and validate quota-aware Agent Profiles.':
      '研究模型，並設計、管理與驗證具配額感知能力的 Agent Profiles。',
  'Agent Executables': 'Agent 執行檔',
  'Override a supported agent CLI executable on this device. Leave a path blank to use the default command from PATH.':
      '覆寫此裝置上支援的 Agent CLI 執行檔。路徑留空則使用 PATH 中的預設命令。',
  'Git Bash Executable': 'Git Bash 執行檔',
  'Full path to git-bash.exe used when Windows Terminal is unavailable. Leave blank to auto-detect Git for Windows.':
      'Windows Terminal 無法使用時所使用的 git-bash.exe 完整路徑。留空會自動偵測 Git for Windows。',
  'Managed hooks let terminal tabs show agent state.':
      '受管理的 Hooks 可讓終端機分頁顯示 Agent 狀態。',
  'Codex Hooks': 'Codex Hooks',
  'Use an Alera-managed Codex runtime home with status hooks.':
      '使用由 Alera 管理且包含狀態 Hooks 的 Codex Runtime Home。',
  'Claude Code Hooks': 'Claude Code Hooks',
  'GitHub Copilot Hooks': 'GitHub Copilot Hooks',
  'Cursor Hooks': 'Cursor Hooks',
  'Antigravity Hooks': 'Antigravity Hooks',
  'OpenCode Hooks': 'OpenCode Hooks',
  'OpenCode 2 Hooks': 'OpenCode 2 Hooks',
  'Pi Hooks': 'Pi Hooks',
  'Amp Hooks': 'Amp Hooks',
  'Grok Build Hooks': 'Grok Build Hooks',
  'Devin Hooks': 'Devin Hooks',
  'fx Status': 'fx 狀態',
  'How Alera reacts while agents are running.': '設定 Agent 執行時 Alera 的行為。',
  'Show Tab Titles in Sidebar': '在側邊欄顯示分頁標題',
  'Agent Status Notifications': 'Agent 狀態通知',
  'Show native notifications when an agent needs attention. Bursts are grouped into one notification.':
      'Agent 需要注意時顯示原生通知；短時間大量通知會合併成一則。',
  'Agent Finished Notifications': 'Agent 完成通知',
  'Also notify when an agent finishes. Most agents report the end of a turn, not the end of a task, so this notifies on every reply.':
      'Agent 完成時也發出通知。多數 Agent 回報的是一輪對話結束，而非整個工作結束，因此每次回覆都可能通知。',
  'Keep Computer Awake While Agents Are Working': 'Agent 工作時保持電腦喚醒',

  // Quotas.
  'Provider Quotas': 'Provider 配額',
  'Choose which usage sources appear for the active workspace host.':
      '選擇作用中工作區 Host 要顯示哪些用量來源。',
  'Active Quota Host': '作用中的配額 Host',
  'Run quota commands locally or through the installed Alera runtime for this workspace.':
      '在本機或透過此工作區已安裝的 Alera Runtime 執行配額命令。',
  'Quota Display Order': '配額顯示順序',
  'Set the left-to-right order of enabled providers in the status bar.':
      '設定已啟用 Provider 在狀態列由左至右的順序。',
  'Configure the default Claude account and every CCS profile together.':
      '一起設定預設 Claude 帳號與所有 CCS 設定檔。',
  'Claude Code Quotas': 'Claude Code 配額',
  'Claude Default Quotas': 'Claude 預設帳號配額',
  'Query the default Claude account separately from configured CCS profiles.':
      '將預設 Claude 帳號與已設定的 CCS Profiles 分開查詢。',
  'Claude Default in Usage': '在 Usage 顯示 Claude 預設帳號',
  'Include the default Claude account in Usage independently of quota polling.':
      '不受配額輪詢設定影響，獨立決定是否在 Usage 顯示預設 Claude 帳號。',
  'Claude CCS Profiles': 'Claude CCS Profiles',
  'Add CCS profiles and choose which ones appear in Usage.':
      '新增 CCS Profiles，並選擇哪些要顯示在 Usage。',
  'Credential Environment': '憑證環境變數',
  'Configure environment variable names for the active workspace host.':
      '設定作用中工作區 Host 使用的環境變數名稱。',
  'Kimi API Key Variable': 'Kimi API Key 變數',
  'Environment variable read on the active host. The secret value is never stored by Alera.':
      '在作用中 Host 讀取的環境變數；Alera 永遠不會儲存其 Secret 值。',
  'Z.ai API Key Variable': 'Z.ai API Key 變數',
  'Z.ai Base URL Variable': 'Z.ai Base URL 變數',
  'Optional environment variable for the coding plan API base URL.':
      '選用的 Coding Plan API Base URL 環境變數。',
  'MiniMax API Key Variable': 'MiniMax API Key 變數',
  'MiniMax API Host Variable': 'MiniMax API Host 變數',
  'Optional environment variable selecting the global or china token plan endpoint.':
      '選用的環境變數，用來選擇全球或中國 Token Plan Endpoint。',
  'Credential Availability': '憑證可用性',
  'Check whether each configured variable exists without reading its secret value.':
      '只檢查各設定變數是否存在，不讀取其 Secret 值。',
  'No quota providers enabled': '尚未啟用任何配額 Provider',
  'No CCS profiles configured': '尚未設定 CCS Profile',
  'Shown in status bar': '顯示於狀態列',
  'Hidden from status bar - available in the quota panel': '不顯示於狀態列，但仍可在配額面板查看',
  'Not shown in Usage': '不顯示於 Usage',
  'Show in Usage': '顯示於 Usage',

  // AI Assist.
  'Custom Command': '自訂命令',
  'Enter a command before selecting this agent. Use {prompt} to pass the prompt as an argument; otherwise Alera sends it on stdin.':
      '選擇此 Agent 前先輸入命令。使用 {prompt} 將提示詞當作參數傳入；否則 Alera 會透過 stdin 傳送。',
  'Local agent CLIs run short background jobs from source control and workspace context.':
      '本機 Agent CLI 會利用版本控制與工作區 Context 執行短時間背景工作。',
  'Enable AI Assist': '啟用 AI Assist',
  'Generate text for source control, workspaces, and agent conversations.':
      '為版本控制、工作區與 Agent 對話產生文字。',
  'Auto-Generate Agent Titles': '自動產生 Agent 標題',
  'Name new agent conversations from their first prompt or recent context.':
      '依第一個提示詞或近期 Context 為新的 Agent 對話命名。',
  'Use {prompt} to pass the prompt as an argument; otherwise Alera sends it on stdin.':
      '使用 {prompt} 將提示詞當作參數傳入；否則 Alera 會透過 stdin 傳送。',
  'Used by prompts that override the global agent with custom command.':
      '供以自訂命令覆寫全域 Agent 的提示詞使用。',
  'Configure the agent, model, reasoning and instructions for this prompt.':
      '設定此提示詞使用的 Agent、模型、推理與指示。',
  'Instructions': '指示',
  'CLI used for AI Assist jobs.': 'AI Assist 工作所使用的 CLI。',
  'Reasoning effort for models that support it.': '支援此功能的模型所使用的推理強度。',
  'Override the global agent for this prompt.': '針對此提示詞覆寫全域 Agent。',
  'Override the global model for this prompt.': '針對此提示詞覆寫全域模型。',
  'Optional prompt guidance.': '選用的提示詞指引。',
  'Optional instructions': '選用指示',

  // Mobile access.
  'Mobile access unavailable': '行動裝置存取無法使用',
  'Connected Remote Devices': '已連線的遠端裝置',
  'Connected through your Alera account. Disable Remote Access to disconnect these devices.':
      '透過你的 Alera 帳號連線。停用「遠端存取」即可中斷這些裝置。',
  'Endpoint': 'Endpoint',
  'Optional expected name for the new device.': '新裝置的預期名稱（選填）。',
  'Expires In': '到期時間',
  'Minutes before the offer expires.': '配對邀請到期前的分鐘數。',
  'Generate Pairing QR': '產生配對 QR Code',
  'Enables the gateway if it is disabled.': '若 Gateway 尚未啟用，會一併啟用。',
  'No active offers': '沒有作用中的配對邀請',
  'Generate a pairing QR to link a new device.': '產生配對 QR Code 以連結新裝置。',
  'Devices that can connect to this runtime.': '可連線到此 Runtime 的裝置。',
  'No paired devices': '沒有已配對裝置',
  'Link a device to see it here.': '連結裝置後會顯示在這裡。',
  'Cancel Pairing Offer': '取消配對邀請',
  'The offer becomes unusable immediately.': '此邀請會立即失效。',
  'Enable Mobile Access': '啟用行動裝置存取',
  'Accept connections from paired mobile devices.': '接受已配對行動裝置的連線。',
  'Enable Remote Access': '啟用遠端存取',
  'Allow signed-in Alera mobile devices to discover this runtime and use the encrypted relay.':
      '允許已登入的 Alera 行動裝置探索此 Runtime 並使用加密 Relay。',
  'Relay Status': 'Relay 狀態',
  'Connection Mode': '連線模式',
  'Windows Firewall': 'Windows 防火牆',
  'Bind Host': '綁定 Host',
  'Interface the gateway listens on.': 'Gateway 監聽的網路介面。',
  'Network Hint': '網路提示',
  'Gateway listener port.': 'Gateway 監聽連接埠。',
  'Apply Gateway Settings': '套用 Gateway 設定',
  'Persist gateway changes.': '儲存 Gateway 變更。',
  'Tailscale Status': 'Tailscale 狀態',
  'NetBird Status': 'NetBird 狀態',
  'NetBird Endpoint': 'NetBird Endpoint',
  'Address included in new pairing offers.': '新配對邀請中包含的位址。',
  'Offer expired - generate a new one': '配對邀請已過期，請產生新的邀請',
  'Scan with the Alera mobile app': '使用 Alera 行動版 App 掃描',
  'Offer expired': '配對邀請已過期',
  'Copied': '已複製',
  'Copy Pairing JSON': '複製配對 JSON',
  'Revoked': '已撤銷',
  'Delete Device': '刪除裝置',
  'Rename Device': '重新命名裝置',
  'Revoke Device': '撤銷裝置',
  'Cancel Offer': '取消邀請',

  // Project/worktree configuration.
  'UI overrides take precedence over repo files.':
      'UI 覆寫設定的優先順序高於 Repository 檔案。',
  'Config Source': '設定來源',
  'Project instructions appended to prompts that start an agent.':
      '附加到啟動 Agent 提示詞後方的專案指示。',
  'Git hosting provider used for pull requests and checks.':
      'Pull Request 與檢查所使用的 Git 託管 Provider。',
  'Hosting Provider': '託管 Provider',
  'Auto-detect uses public hosts. Select GitHub for GitHub Enterprise Server.':
      '自動偵測只使用公開 Host。GitHub Enterprise Server 請選擇 GitHub。',
  'Auto-Detect': '自動偵測',
  'Copy Rules': '複製規則',
  'Files copied from the main worktree. Gitignored matches from .worktreeinclude are copied too.':
      '從主要 Worktree 複製檔案；`.worktreeinclude` 中符合但被 Gitignore 的檔案也會複製。',
  'Setup Commands': '設定命令',
  'Commands run from the new linked workspace.': '在新建立的連結工作區中執行的命令。',
  'No projects': '沒有專案',
  'Add a project before configuring workspace setup.': '請先新增專案，再設定工作區初始化。',

  // Terminal/editor.
  'Default terminal typography for new sessions.': '新終端機工作階段的預設字型設定。',
  'Default cursor appearance for terminal sessions.': '終端機工作階段的預設游標外觀。',
  'Blink the cursor while the terminal has focus.': '終端機取得焦點時讓游標閃爍。',
  'Terminal colors, theme and spacing.': '終端機色彩、主題與間距。',
  'Foreground Color': '前景色',
  'Override the terminal text color.': '覆寫終端機文字顏色。',
  'Background Color': '背景色',
  'Override the terminal background color.': '覆寫終端機背景顏色。',
  'Cursor Color': '游標顏色',
  'Override the terminal cursor color.': '覆寫終端機游標顏色。',
  'Selection Color': '選取範圍顏色',
  'Override the terminal selection color.': '覆寫終端機選取範圍顏色。',
  'Mouse, scrolling and clipboard behavior for TUIs.': 'TUI 的滑鼠、捲動與剪貼簿行為。',
  'Mouse reports sent per wheel step while a TUI owns scrolling.':
      'TUI 接管捲動時，每個滾輪步進傳送的滑鼠事件數。',
  'Copy local terminal selections to the system clipboard.':
      '將本機終端機選取內容複製到系統剪貼簿。',
  'Let terminal applications replace the system clipboard.':
      '允許終端機應用程式覆寫系統剪貼簿。',
  'History, shell startup and double-click selection behavior.':
      '歷程、Shell 啟動與雙擊選取行為。',
  'Use Login Shell': '使用 Login Shell',
  'Start shells as login shells so profile files such as ~/.zprofile and ~/.profile are loaded.':
      '以 Login Shell 啟動 Shell，以載入 ~/.zprofile、~/.profile 等設定檔。',
  'Reload Shell Environment': '重新載入 Shell 環境',
  'Re-read the login shell PATH so tools installed since the runtime started resolve in new terminals.':
      '重新讀取 Login Shell 的 PATH，讓 Runtime 啟動後新安裝的工具可在新終端機中找到。',
  'Reload': '重新載入',
  'Terminal Memory Budget': '終端機記憶體預算',
  'No matching fonts.': '沒有符合的字型。',
  'Font Family': '字型',
  'Font Size': '字型大小',
  'Font Weight': '字重',
  'Line Height': '行高',
  'Background Opacity': '背景透明度',
  'Horizontal Padding': '水平內距',
  'Vertical Padding': '垂直內距',
  'Blinking Cursor': '游標閃爍',
  'Cursor Opacity': '游標透明度',
  'Color Overrides': '色彩覆寫',
  'TUI Scroll Speed': 'TUI 捲動速度',
  'Copy On Select': '選取時複製',
  'Allow OSC 52 Clipboard Writes': '允許 OSC 52 寫入剪貼簿',
  'Show Terminal Composer By Default': '預設顯示終端機 Composer',
  'Scrollback Lines': 'Scrollback 行數',
  'Host Scrollback Size': 'Host Scrollback 大小',
  'Word Separators': '單字分隔字元',
  'Terminal Shortcut Behavior': '終端機快速鍵行為',

  // Settings search copy.
  'Where new linked workspaces are created on disk.': '新建立的連結工作區在磁碟上的存放位置。',
  'Move inactive workspaces into the Archived section.': '將閒置工作區移至「已封存」區段。',
  'Ask before unregistering a project.': '取消註冊專案前先詢問。',
  'Ask before removing a workspace worktree.': '移除工作區 Worktree 前先詢問。',
  'Show the folder holding the app log files.': '顯示保存應用程式 Log 的資料夾。',
  'Save app and runtime logs with version details as a zip.':
      '將應用程式與 Runtime Log 及版本資訊儲存為 ZIP。',
  'How much detail is written to the log files.': '設定寫入 Log 的詳細程度。',
  'Show your support for the project.': '表達你對此專案的支持。',
  'Install or update every core Alera agent skill.':
      '安裝或更新所有核心 Alera Agent Skill。',
  'Install agent instructions for the Alera CLI.': '安裝 Alera CLI 的 Agent 操作指引。',
  'Install agent instructions for Alera orchestration.':
      '安裝 Alera Orchestration 的 Agent 操作指引。',
  'Install specialized instructions for Agent Profile catalogs.':
      '安裝 Agent Profile Catalog 專用指引。',
  'Use Alera-managed Codex runtime hooks.':
      '使用由 Alera 管理的 Codex Runtime Hooks。',
  'Show native notifications when agents need attention.': 'Agent 需要注意時顯示原生通知。',
  'Also notify when an agent finishes a turn.': 'Agent 完成一輪回覆時也通知。',
  'Keep this computer and display awake during agent work.':
      'Agent 工作期間保持電腦與螢幕喚醒。',
  'View and remap app-wide key bindings.': '檢視並重新設定全域快速鍵。',
  'Syntax highlighting theme used by editor tabs.': '編輯器分頁使用的語法醒目提示主題。',
  'Spaces inserted when pressing tab in editor tabs.': '在編輯器分頁按下 Tab 時插入的空白數。',
  'Automatically save dirty editor tabs after a pause.': '編輯器變更一段時間沒有操作後自動儲存。',
  'AI Assist Agent': 'AI Assist Agent',
  'AI Assist Commit Messages': 'AI Assist Commit 訊息',
  'AI Assist Pull Request Details': 'AI Assist Pull Request 詳細資料',
  'AI Assist Agent Titles': 'AI Assist Agent 標題',
  'AI Assist Workspace Identity': 'AI Assist 工作區識別',
  'AI Assist Reading Diffs': 'AI Assist 閱讀 Diff',
  'Project Worktree Setup': '專案 Worktree 設定',
  'Link Mobile Device': '連結行動裝置',
};
