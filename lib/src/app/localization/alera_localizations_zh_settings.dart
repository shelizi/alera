part of 'alera_localizations.dart';

const Map<String, String> _traditionalChineseSettings = <String, String>{
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
  'Add another verified sign-in method to this account.': '為此帳號新增另一個已驗證的登入方式。',
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
  'Base API URL. Alera appends /audio/transcriptions when needed and preserves query parameters.':
      'API Base URL。Alera 會在需要時附加 /audio/transcriptions，並保留 query parameters。',
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
  'Fastest, with lower transcription accuracy.': '速度最快，但轉錄準確度較低。',
  'Balanced speed and accuracy. Recommended for most devices.':
      '兼顧速度與準確度，建議大多數裝置使用。',
  'Improved accuracy with slower transcription.': '準確度較高，但轉錄速度較慢。',
  'Highest curated accuracy with the largest memory cost.':
      '提供最高的精選準確度，但記憶體占用也最大。',
  'The model download could not finish. Try again.': '模型下載未能完成，請再試一次。',
  'Select another installed model before removing this one.':
      '移除此模型前，請先選擇另一個已安裝的模型。',
  'Improving Transcript': '正在改善轉錄內容',
  'Cancel Transcription': '取消轉錄',
  'Stop Dictation': '停止聽寫',
  'Start Dictation': '開始聽寫',
  'Remote audio processing was disabled before transcription.':
      '遠端音訊處理已在轉錄前停用。',
  'Enable AI Dictation in Settings before recording.': '錄音前請先在設定中啟用 AI 聽寫。',
  'The dictation text field is no longer available.': '聽寫文字欄位已無法使用。',
  'Download the selected Whisper model in Settings before recording.':
      '錄音前請先在設定中下載所選的 Whisper 模型。',
  'Allow remote audio processing in AI Dictation settings first.':
      '請先在 AI 聽寫設定中允許遠端音訊處理。',
  'Microphone permission is required for AI Dictation.': 'AI 聽寫需要麥克風權限。',
  'On-device speech recognition is unavailable for this locale.':
      '此地區設定無法使用裝置端語音辨識。',
  'Allow online speech recognition in AI Dictation settings first.':
      '請先在 AI 聽寫設定中允許線上語音辨識。',
  'The system recognizer did not produce a transcription.': '系統辨識器未產生轉錄內容。',
  'The microphone did not produce an audio recording.': '麥克風未產生音訊錄音。',
  'The text field was closed before dictation finished.': '聽寫完成前文字欄位已關閉。',
  's': '秒',

  'Typography': '字型',
  'Cursor': '游標',
  'Appearance': '外觀',
  'Interaction': '互動',
  'Advanced': '進階',
  'Mobile Gateway': '行動閘道',
  'Applying…': '正在套用…',
  'Unknown': '未知',
  'Not detected': '未偵測到',
  'Not Detected': '未偵測到',
  'Not running': '未執行',
  'Not Connected': '未連線',
  'No Tailnet IP': '沒有 Tailnet IP',
  'No NetBird IP': '沒有 NetBird IP',
  'Link A Device': '連結裝置',
  'Active Pairing Offers': '有效的配對邀請',
  'Paired Devices': '已配對裝置',
  'Indentation': '縮排',
  'Autosave': '自動儲存',
  'Tab Size': 'Tab 寬度',
  'Control dependency and build directories that Quick Open never indexes.':
      '管理快速開啟永遠不建立索引的套件庫與建置目錄。',
  'Excluded Directory Names': '排除的目錄名稱',
  'These directory names are never indexed, even when Git-ignored files are included. Matching is case-insensitive.':
      '即使包含 Git 忽略檔案，這些目錄名稱也永遠不會建立索引；比對不分大小寫。',
  'Directory name, e.g. generated': '目錄名稱，例如 generated',
  '.git, .hg, and .svn are always excluded.': '.git、.hg、.svn 永遠固定排除。',
  'Restore Defaults': '還原預設值',
  'PowerShell 7 Executable': 'PowerShell 7 執行檔',
  'Optional full path to pwsh.exe on Windows. Leave blank to auto-detect standard, Scoop, LocalAppData, and PATH locations.':
      'Windows 上 pwsh.exe 的完整路徑（選填）。留白會自動偵測標準安裝位置、Scoop、LocalAppData 與 PATH。',
  'Override or auto-detect the Windows pwsh.exe path.':
      '指定 Windows pwsh.exe 路徑，或使用自動偵測。',
  'Theme Preset': '主題預設',
  'Search and select a built-in terminal color theme.': '搜尋並選擇內建的終端機配色主題。',
  'Cursor Shape': '游標形狀',
  'Cursor style for new terminal sessions.': '新終端機工作階段使用的游標樣式。',
  'Toolbar Corner': '工具列位置',
  'Where the pulse, composer, and refresh buttons sit on the terminal tab.':
      '設定終端機分頁中的活動指示器、輸入區與重新整理按鈕位置。',
  'Top Left': '左上',
  'Top Right': '右上',
  'Bottom Left': '左下',
  'Bottom Right': '右下',
  'Select color': '選擇顏色',
  'Choose color': '選擇顏色',
  'spaces': '個空白',
  'seconds': '秒',
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
  'Storage, safety, runtime, diagnostics and updates.': '儲存空間、安全性、執行環境、診斷與更新。',
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
  'External Editor': '外部編輯器',
  'Open workspaces and files in an external editor without changing Alera\'s built-in editor behavior.':
      '在外部編輯器中開啟工作區與檔案，不變更 Alera 內建編輯器的行為。',
  'Editor used by Open In menu entries, keyboard shortcuts, and external file targets.':
      'Open In 選單項目、鍵盤快捷鍵與外部檔案目標所使用的編輯器。',
  'Default Code Open Target': '預設程式碼開啟目標',
  'Choose where normal editable source and text files open. Dedicated Alera previews stay internal.':
      '選擇一般可編輯原始碼與文字檔的開啟位置；Alera 專用預覽仍保留在內部。',
  'Open Workspaces in New Window': '在新視窗開啟工作區',
  'Auto-open New Workspaces Externally': '自動在外部編輯器開啟新工作區',
  'Run a non-destructive version check with the current executable setting.':
      '使用目前的執行檔設定執行不會修改資料的版本檢查。',
  'Search syntax themes': '搜尋語法主題',
  'Prompt Append': '附加提示詞',
  'Add project-specific agent instructions': '新增專案專屬的 Agent 指示',
  'From': '來源',
  'To': '目的地',
  'Defaults to from': '預設與來源相同',
  'Save Override': '儲存覆寫設定',
  'Custom Prompt': '自訂提示詞',
  'Optional instructions for every dispatched task': '每個派送工作可選用的額外指示',
  'Quota Group': '配額群組',
  'Command mode is for advanced or unsupported CLI options. Use an interactive command that can accept a dispatch and report completion.':
      'Command 模式適用於進階或尚未支援的 CLI 選項。請使用可接收派送工作並回報完成狀態的互動式指令。',
  'Profiles sharing a quota group drain the same usage bucket. Alera never measures this; it only avoids falling back inside the same group. Leave empty if unsure.':
      '共用同一配額群組的設定檔會消耗相同的用量額度。Alera 不會量測額度，只會避免在同群組內進行 fallback；若不確定請留空。',

  // Recent workspace and agent settings.
  'Workspaces': '工作區',
  'Automatic archiving for inactive workspaces.': '自動封存閒置的工作區。',
  'Auto-Archive After Inactivity': '閒置後自動封存',
  'Days without activity before a workspace moves to the Archived section. Set to 0 to keep workspaces listed.':
      '工作區連續未活動達指定天數後移至「已封存」區段。設為 0 可讓工作區持續顯示。',
  'Move inactive workspaces into the Archived section.': '將閒置工作區移至「已封存」區段。',
  'Agent Executables': 'Agent 執行檔',
  'Override a supported agent CLI executable on this device. Leave a path blank to use the default command from PATH.':
      '覆寫此裝置上支援的 Agent CLI 執行檔。路徑留空會使用 PATH 中的預設指令。',
  'Git Bash Executable': 'Git Bash 執行檔',
  'Full path to git-bash.exe used when Windows Terminal is unavailable. Leave blank to auto-detect Git for Windows.':
      'Windows Terminal 無法使用時所用 git-bash.exe 的完整路徑。留空會自動偵測 Git for Windows。',
  'Managed Options': '受管理選項',
  'Alera builds the interactive command from these agent-specific settings.':
      'Alera 會依這些 Agent 專屬設定建立互動式指令。',
  'Leave as default to use the agent configuration.': '保留預設值以使用 Agent 設定。',
  'Exact Model ID': '精確模型 ID',
  'Use a model ID that is not in the discovered list.': '使用不在已偵測清單中的模型 ID。',
  'Persona': 'Persona',
  'Select a known agent persona or enter an exact name.':
      '選擇已知的 Agent Persona，或輸入精確名稱。',
  'Exact Persona': '精確 Persona',
  'Use a persona name that is not in the discovered list.':
      '使用不在已偵測清單中的 Persona 名稱。',
  'Leave empty to use the agent default.': '留空以使用 Agent 預設值。',
  'Reasoning Effort': '推理強度',
  'Plan Mode Reasoning Effort': 'Plan 模式推理強度',
  'Applies only while Codex is in plan mode, which is entered with Shift+Tab or /plan. Codex has no way to start there.':
      '僅在 Codex 進入 Plan 模式時套用；可用 Shift+Tab 或 /plan 進入，Codex 無法直接以此模式啟動。',
  'Sandbox': '沙盒',
  'Full Access': '完整存取',
  'Workspace Write': '工作區寫入',
  'Approval Policy': '核准政策',
  'On Request': '依要求',
  'Never Ask': '永不詢問',
  'Web Search': '網頁搜尋',
  'Allow Codex to search the web.': '允許 Codex 搜尋網頁。',
  'Bypass All Protections': '略過所有保護',
  'Bypass both approval prompts and sandbox isolation.': '同時略過核准提示與沙盒隔離。',
  'Allow Skip Permissions': '允許略過權限',
  'Make bypass available during the session without starting in it. Use the Bypass Permissions mode above to start in it.':
      '讓工作階段中可切換為略過權限，而不必以該模式啟動。若要直接以該模式啟動，請使用上方的「略過權限」模式。',
  'Mode': '模式',
  'Context': 'Context',
  'Default Context': '預設 Context',
  'Long Context': '長 Context',
  'Allow All': '全部允許',
  'Allow tools and paths without individual prompts.': '允許工具與路徑，不逐項提示。',
  'Maximum AI Credits': 'AI Credits 上限',
  'Maximum Autopilot Continues': 'Autopilot 繼續次數上限',
  'Do Not Ask User': '不要詢問使用者',
  'Continue without asking the user for input.': '不詢問使用者輸入並繼續執行。',
  'Permission Mode': '權限模式',
  'Do Not Ask': '不詢問',
  'Review Mode': '審查模式',
  'Trust Workspace': '信任工作區',
  'Trust the workspace without an interactive prompt.': '不顯示互動提示並信任工作區。',
  'Skip Permissions': '略過權限',
  'Run without Antigravity permission checks.': '執行時略過 Antigravity 權限檢查。',
  'Auto Approve': '自動核准',
  'Approve OpenCode actions automatically.': '自動核准 OpenCode 操作。',
  'Thinking': '思考',
  'Project Trust': '專案信任',
  'Amp permission rules continue to come from the global Amp configuration.':
      'Amp 權限規則仍沿用全域 Amp 設定。',
  'Fast Mode': '快速模式',
  'Prefer lower latency responses.': '優先使用較低延遲的回應。',
  'Enable the Antigravity sandbox.': '啟用 Antigravity 沙盒。',
  'Sandbox Devin exec-tool processes where supported.':
      '在支援的平台上，將 Devin exec-tool 程序置於沙盒中執行。',
  'Disable Web Search': '停用網頁搜尋',
  'Disable Grok Build web search and web fetch tools.':
      '停用 Grok Build 的網頁搜尋與網頁擷取工具。',
  'Resume Latest Session': '繼續最近的工作階段',
  'Ignore Additional Directories': '忽略額外目錄',
  'Do not load additional directories configured by fx.': '不要載入 fx 設定的額外目錄。',
  'Record Session': '記錄工作階段',
  'Minimal': '最低',
  'Low': '低',
  'Medium': '中',
  'High': '高',
  'Extra High': '極高',
  'Max': '最高',
  'Ultra': '超高',
  'Read Only': '唯讀',
  'Untrusted': '不受信任',
  'Accept Edits': '接受編輯',
  'Bypass Permissions': '略過權限',
  'Interactive': '互動',
  'Autopilot': '自動執行',
  'Ask': '詢問',
  'Auto Review': '自動審查',
  'Force': '強制',
  'Ignore': '忽略',
  'Smart': '智慧',
  'Dangerous': '危險',
  'Strict': '嚴格',
  'Settings changed elsewhere. Your change was not saved. Review the latest values and try again.':
      '設定已在其他位置變更。你的變更未儲存。請檢查最新值後再試一次。',
  'Automation settings changed elsewhere. Your change was not saved. Review the latest values and try again.':
      '自動化設定已在其他位置變更。你的變更未儲存。請檢查最新值後再試一次。',

  'Alias': '別名',
  'CCS Profile': 'CCS 設定檔',
  'Usage Name': '用量顯示名稱',
  'Device Name': '裝置名稱',
  'My Phone': '我的手機',
  'Generating…': '產生中…',
  'Generate': '產生',
  'Host': '主機',
  'Username': '使用者名稱',
  'Port': '連接埠',
  'Install Directory': '安裝目錄',
  'Default per platform': '依平台使用預設值',
  'Search built-in themes': '搜尋內建主題',
  'Initial Prompt': '初始提示詞',
  'Describe what the agent should build or paste an image':
      '描述要讓 Agent 建立的內容，或貼上圖片',
  'Loading branches': '正在載入 Branch',
  'Select Branch': '選擇 Branch',
  'Create an agent profile in settings': '請先在設定中建立 Agent 設定檔',
  'Select Agent Profile': '選擇 Agent 設定檔',
  'Create Another': '繼續建立下一個',
  'Working': '處理中',
  'Complete the prompt, project, branch, and agent profile.':
      '請完成提示詞、專案、Branch 與 Agent 設定檔。',
  'Generating workspace identity': '正在產生工作區識別',
  'Checking generated branch': '正在檢查產生的 Branch',
  'Creating workspace': '正在建立工作區',
  'Starting agent': '正在啟動 Agent',
  'Could not paste clipboard image.': '無法貼上剪貼簿圖片。',
  'Choose every workspace setting yourself, including the branch name and optional parent workspace.':
      '自行選擇所有工作區設定，包括 Branch 名稱與選填的父工作區。',
  'Describe the replacement to generate.': '描述要產生的替換內容。',
  'Define the reusable instruction and its availability.': '定義可重複使用的指示及其可用狀態。',
  'Show this action in the Text Actions menu.': '在 Text Actions 選單中顯示此動作。',
  'Choose which CLI and model run this action.': '選擇執行此動作的 CLI 與模型。',
  'Inherit the global AI Assist agent by default.': '預設繼承全域 AI Assist Agent。',
  'Inherit the selected model unless overridden.': '除非覆寫，否則繼承目前選取的模型。',
  'Reasoning effort for the effective model.': '設定實際使用模型的推理強度。',
  'Action': '動作',
  'Enabled': '已啟用',
  'Reasoning': '推理',
  'No text actions': '尚無文字操作',
  'Select a text action': '選擇文字操作',
  'Delete Text Action': '刪除文字操作',
  'Action ID is required.': '動作 ID 為必填。',
  'Action name is required.': '動作名稱為必填。',
  'Action prompt is required.': '動作提示詞為必填。',
  'Action IDs must be unique.': '動作 ID 不可重複。',
  'Action names must be unique.': '動作名稱不可重複。',
  'Text changed while the action was running.': '執行動作期間文字已變更。',
  'Text action returned no replacement text.': '文字操作未回傳可替換的文字。',
  'Text action could not update this field.': '文字操作無法更新此欄位。',
  'Text action applied.': '已套用文字操作。',
  'Text action was canceled.': '已取消文字操作。',
  // Settings coverage guard: remaining user-facing titles.
  'Whisper Model': 'Whisper 模型',
  'Workspace Directory': '工作區目錄',
  'Confirm Project Removal': '確認移除專案',
  'Confirm Workspace Removal': '確認移除工作區',
  'Show Tray Icon': '顯示系統匣圖示',
  'Show Dock Badge': '顯示 Dock 徽章',
  'Show Tray Badge': '顯示系統匣徽章',
  'Keep Computer Awake': '防止電腦休眠',
  'Keep Runtime Open When App Quits': '應用程式結束後保持 Runtime 執行',
  'Empty Host Shutdown': '空閒 Host 關閉',
  'Detached Session Shutdown': '分離工作階段關閉',
  'Open Logs Folder': '開啟 Log 資料夾',
  'Export Diagnostics': '匯出診斷資料',
  'Log Level': 'Log 等級',
  'Send Crash Reports': '傳送當機報告',
  'Star Alera on GitHub': '在 GitHub 為 Alera 加星',
  'All Alera Skills': '所有 Alera Skills',
  'Alera CLI Skill': 'Alera CLI 技能',
  'Alera Orchestration Skill': 'Alera 協調技能',
  'Agent Profiles Skill': 'Agent Profiles 技能',
  'Codex Hooks': 'Codex 狀態 Hook',
  'Claude Code Hooks': 'Claude Code 狀態 Hook',
  'GitHub Copilot Hooks': 'GitHub Copilot 狀態 Hook',
  'Cursor Hooks': 'Cursor 狀態 Hook',
  'Antigravity Hooks': 'Antigravity 狀態 Hook',
  'OpenCode Hooks': 'OpenCode 狀態 Hook',
  'OpenCode 2 Hooks': 'OpenCode 2 狀態 Hook',
  'Pi Hooks': 'Pi 狀態 Hook',
  'Amp Hooks': 'Amp 狀態 Hook',
  'Grok Build Hooks': 'Grok Build 狀態 Hook',
  'Show Tab Titles in Sidebar': '在側邊欄顯示分頁標題',
  'Agent Status Notifications': 'Agent 狀態通知',
  'Agent Finished Notifications': 'Agent 完成通知',
  'Keep Computer Awake While Agents Are Working': 'Agent 工作時防止電腦休眠',
  'Provider Quotas': 'Provider 配額',
  'Claude Code Quotas': 'Claude Code 配額',
  'Claude Default Quotas': 'Claude 預設帳號配額',
  'Claude Default in Usage': '在用量中顯示 Claude 預設帳號',
  'Claude CCS Profiles': 'Claude CCS 設定檔',
  'Quota Credential Environment': '配額憑證環境',
  'Kimi API Key Variable': 'Kimi API Key 環境變數',
  'Agent And Model': 'Agent 與模型',
  'Duplicate And Reorder': '複製與重新排序',
  'Show Pull Request Status': '顯示 Pull Request 狀態',
  'Notify When Checks Fail': '檢查失敗時通知',
  // Complete Settings surface coverage.
  'Sign in with Google or GitHub.': '使用 Google 或 GitHub 登入。',
  'Choose which runtime events notify enrolled phones.': '選擇哪些執行環境事件要通知已加入的手機。',
  'Account Ownership': '帳號擁有權',
  'Transfer a runtime or delete an Alera account.': '移轉執行環境或刪除 Alera 帳號。',
  'Transcribe microphone recordings locally or with a remote speech API.':
      '在本機或透過遠端語音 API 轉錄麥克風錄音。',
  'Use a Codex subscription or an OpenAI-compatible API with a secure token.':
      '使用 Codex 訂閱或搭配安全 Token 的 OpenAI 相容 API。',
  'Download, resume, select, or remove local Whisper models.':
      '下載、繼續下載、選擇或移除本機 Whisper 模型。',
  'Clean up or summarize transcripts with an agent subscription.':
      '使用 Agent 訂閱整理或摘要轉錄內容。',
  'Record and transcribe a sample without leaving AI Dictation settings.':
      '不離開 AI 聽寫設定即可錄製並轉錄範例。',
  'Support Alera': '支持 Alera',
  'Star On GitHub': '在 GitHub 加星',
  'Not Now': '現在不要',
  'Alera CLI And Skills': 'Alera CLI 與 Skills',
  'Register the CLI command and install agent instructions.':
      '註冊 CLI 指令並安裝 Agent 指示。',
  'Alera CLI Command': 'Alera CLI 指令',
  'Register the Alera command on PATH for terminals and agents.':
      '將 Alera 指令註冊到 PATH，供終端機與 Agent 使用。',
  'Install or update CLI and orchestration skills. Reapplies selected status hooks.':
      '安裝或更新 CLI 與協調 Skills，並重新套用已選取的狀態 Hook。',
  'Install the Codex skill that teaches agents to use the Alera CLI.':
      '安裝教導 Agent 使用 Alera CLI 的 Codex Skill。',
  'Install or update orchestration and reapply selected status hooks.':
      '安裝或更新協調功能，並重新套用已選取的狀態 Hook。',
  'Install optional skills for specialized Alera workflows.':
      '安裝適用於特定 Alera 工作流程的選用 Skills。',
  'Research models and design, manage, and validate quota-aware Agent Profiles.':
      '研究模型，並設計、管理及驗證具配額感知能力的 Agent Profiles。',
  'Managed hooks let terminal tabs show agent state.':
      '受管理的 Hook 可讓終端機分頁顯示 Agent 狀態。',
  'Use an Alera-managed Codex runtime home with status hooks.':
      '使用由 Alera 管理、含狀態 Hook 的 Codex runtime home。',
  'Use an Alera-managed Claude Code config with status hooks.':
      '使用由 Alera 管理、含狀態 Hook 的 Claude Code 設定。',
  'Use an Alera-managed GitHub Copilot home overlay.':
      '使用由 Alera 管理的 GitHub Copilot home overlay。',
  'Use an Alera-managed Cursor agent plugin wrapper.':
      '使用由 Alera 管理的 Cursor Agent plugin wrapper。',
  'Install Alera-managed Antigravity hooks for the agy CLI. Disable to remove only Alera-managed hook entries.':
      '為 agy CLI 安裝由 Alera 管理的 Antigravity Hook；停用時只會移除 Alera 管理的 Hook 項目。',
  'Use an Alera-managed OpenCode config overlay with status plugin.':
      '使用由 Alera 管理、含狀態 Plugin 的 OpenCode 設定 overlay。',
  'Use an Alera-managed OpenCode 2 config overlay with the v2 status plugin.':
      '使用由 Alera 管理、含 v2 狀態 Plugin 的 OpenCode 2 設定 overlay。',
  'Use an Alera-managed Pi agent overlay with status extension.':
      '使用由 Alera 管理、含狀態 Extension 的 Pi Agent overlay。',
  'Use an Alera-managed Amp config overlay.': '使用由 Alera 管理的 Amp 設定 overlay。',
  'Install Alera-managed Grok build hooks in a dedicated global file.':
      '在專用的全域檔案中安裝由 Alera 管理的 Grok Build Hook。',
  'Devin Hooks': 'Devin 狀態 Hook',
  'Install Alera-managed Devin lifecycle hooks in the global Devin config.':
      '在全域 Devin 設定中安裝由 Alera 管理的生命週期 Hook。',
  'fx Status': 'fx 狀態',
  'Receive fx lifecycle state through its built-in local Herdr integration on macOS and Linux.':
      '在 macOS 與 Linux 上透過 fx 內建的本機 Herdr 整合接收生命週期狀態。',
  'How Alera reacts while agents are running.': '設定 Agent 執行期間 Alera 的行為。',
  'Use each agent tab title under a workspace instead of the latest activity.':
      '在工作區下顯示各 Agent 分頁標題，而非最新活動。',
  'Show native notifications when an agent needs attention. Bursts are grouped into one notification.':
      'Agent 需要注意時顯示原生通知；短時間大量事件會合併為一則通知。',
  'Show native notifications when agents need attention.': 'Agent 需要注意時顯示原生通知。',
  'Also notify when an agent finishes. Most agents report the end of a turn, not the end of a task, so this notifies on every reply.':
      'Agent 完成時也通知。多數 Agent 回報的是一個回合結束而非整個工作結束，因此每次回覆都會通知。',
  'Also notify when an agent finishes a turn.': 'Agent 完成一個回合時也通知。',
  'Keep this computer and display awake during agent work.':
      'Agent 工作期間讓此電腦與顯示器保持喚醒。',
  'Agent profiles unavailable': '無法取得 Agent Profiles',
  'No agent profiles': '尚無 Agent Profile',
  'How this agent is launched for a dispatched task.':
      '設定此 Agent 接收派送工作時的啟動方式。',
  'Adapter Type': 'Adapter 類型',
  'Command Preview': '指令預覽',
  'The host quotes these arguments for the actual platform shell.':
      'Host 會依實際平台 Shell 對這些參數加上適當引號。',
  'Routing': '路由',
  'Signals the orchestrator reads when planning a run.': '協調器規劃執行時會讀取的訊號。',
  'Managed': '受管理',
  'Command': '指令',
  'Test Agent Profile': '測試 Agent Profile',
  'The profile command runs here. It does not receive a dispatched task.':
      'Profile 指令會在此執行，不會收到派送工作。',
  'Confirm Reduced Protections': '確認降低保護',
  'Save Anyway': '仍要儲存',
  'Agent profile order could not be saved': '無法儲存 Agent Profile 順序',
  'Agent profile saved': 'Agent Profile 已儲存',
  'Default agent profile updated': '預設 Agent Profile 已更新',
  'Agent profile cloned': 'Agent Profile 已複製',
  'Choose which usage sources appear for the active workspace host.':
      '選擇目前工作區 Host 要顯示哪些用量來源。',
  'Active Quota Host': '目前配額 Host',
  'Run quota commands locally or through the installed Alera runtime for this workspace.':
      '在本機或透過此工作區已安裝的 Alera 執行環境執行配額指令。',
  'Quota Display Order': '配額顯示順序',
  'Set the left-to-right order of enabled providers in the status bar.':
      '設定狀態列中已啟用 Provider 由左到右的顯示順序。',
  'Configure the default Claude account and every CCS profile together.':
      '一起設定預設 Claude 帳號與所有 CCS Profile。',
  'Enable Claude quotas for default and CCS accounts.':
      '啟用預設帳號與 CCS 帳號的 Claude 配額。',
  'Query the default Claude account separately from configured CCS profiles.':
      '將預設 Claude 帳號與已設定的 CCS Profile 分開查詢。',
  'Configure the default Claude account independently.': '獨立設定預設 Claude 帳號。',
  'Include the default Claude account in Usage independently of quota polling.':
      '不受配額輪詢影響，獨立決定是否在「用量」中包含預設 Claude 帳號。',
  'Choose whether the default Claude account appears in Usage.':
      '選擇是否在「用量」中顯示預設 Claude 帳號。',
  'Add CCS profiles and choose which ones appear in Usage.':
      '新增 CCS Profile，並選擇哪些要顯示在「用量」中。',
  'Configure CCS alias and profile pairs for Claude quotas.':
      '設定 Claude 配額使用的 CCS 別名與 Profile 配對。',
  'Credential Environment': '憑證環境',
  'Configure environment variable names for the active workspace host.':
      '設定目前工作區 Host 使用的環境變數名稱。',
  'Configure environment variable names for Kimi, Z.ai and MiniMax.':
      '設定 Kimi、Z.ai 與 MiniMax 的環境變數名稱。',
  'Configure the Kimi API key environment variable name.':
      '設定 Kimi API Key 的環境變數名稱。',
  'Environment variable read on the active host. The secret value is never stored by Alera.':
      '從目前 Host 讀取的環境變數；Alera 絕不儲存 Secret 值。',
  'Z.ai API Key Variable': 'Z.ai API Key 環境變數',
  'Z.ai Base URL Variable': 'Z.ai Base URL 環境變數',
  'Optional environment variable for the coding plan API base URL.':
      'Coding plan API Base URL 的選用環境變數。',
  'MiniMax API Key Variable': 'MiniMax API Key 環境變數',
  'MiniMax API Host Variable': 'MiniMax API Host 環境變數',
  'Optional environment variable selecting the global or china token plan endpoint.':
      '選用的環境變數，用來選擇全球或中國 Token plan endpoint。',
  'Credential Availability': '憑證可用狀態',
  'Check whether each configured variable exists without reading its secret value.':
      '檢查各已設定的環境變數是否存在，不讀取其 Secret 值。',
  'Show in Usage': '顯示於用量',
  'Work': '工作',
  'AI Assist Agent': 'AI Assist Agent',
  'Choose the CLI used for AI Assist jobs.': '選擇 AI Assist 工作使用的 CLI。',
  'CLI used for AI Assist jobs.': 'AI Assist 工作使用的 CLI。',
  'AI Assist Commit Messages': 'AI Assist Commit 訊息',
  'Choose the agent, model, reasoning and instructions for commit messages.':
      '選擇產生 Commit 訊息使用的 Agent、模型、推理強度與指示。',
  'AI Assist Pull Request Details': 'AI Assist Pull Request 詳細資料',
  'Choose the agent, model, reasoning and instructions for pull request details.':
      '選擇產生 Pull Request 詳細資料使用的 Agent、模型、推理強度與指示。',
  'AI Assist Reading Diffs': 'AI Assist 閱讀差異',
  'Choose the agent, model, reasoning and instructions for reading diffs.':
      '選擇閱讀差異使用的 Agent、模型、推理強度與指示。',
  'AI Assist Agent Titles': 'AI Assist Agent 標題',
  'Configure automatic conversation titles, provider, model and instructions.':
      '設定自動對話標題、Provider、模型與指示。',
  'AI Assist Workspace Identity': 'AI Assist 工作區識別',
  'Choose the agent, model, reasoning and instructions for workspace identity.':
      '選擇產生工作區識別使用的 Agent、模型、推理強度與指示。',
  'Local agent CLIs run short background jobs from source control and workspace context.':
      '本機 Agent CLI 會依原始碼控制與工作區 Context 執行短時間背景工作。',
  'Run short local agent jobs for source control, workspace identity, and speech.':
      '執行用於原始碼控制、工作區識別與語音的短時間本機 Agent 工作。',
  'Enable AI Assist': '啟用 AI Assist',
  'Generate text for source control, workspaces, and agent conversations.':
      '為原始碼控制、工作區與 Agent 對話產生文字。',
  'Auto-Generate Agent Titles': '自動產生 Agent 標題',
  'Name new agent conversations from their first prompt or recent context.':
      '依第一個 Prompt 或最近 Context 為新的 Agent 對話命名。',
  'Custom Command': '自訂指令',
  'Use {prompt} to pass the prompt as an argument; otherwise Alera sends it on stdin.':
      '使用 {prompt} 將 Prompt 當作參數傳入；否則 Alera 會透過 stdin 傳送。',
  'Used by prompts that override the global agent with custom command.':
      '供以自訂指令覆寫全域 Agent 的 Prompt 使用。',
  'Configure the agent, model, reasoning and instructions for this prompt.':
      '設定此 Prompt 使用的 Agent、模型、推理強度與指示。',
  'Instructions': '指示',
  'Reasoning effort for models that support it.': '支援此功能的模型所使用的推理強度。',
  'Override the global agent for this prompt.': '為此 Prompt 覆寫全域 Agent。',
  'Optional prompt guidance.': '選用的 Prompt 指引。',
  'Optional instructions': '選用指示',
  'Confirmation prompts for destructive workspace actions.': '破壞性工作區操作的確認提示。',
  'Ask before unregistering a project and deleting its workspace metadata.':
      '取消註冊專案並刪除其工作區中繼資料前先詢問。',
  'Ask before unregistering a project.': '取消註冊專案前先詢問。',
  'Always required because removal closes all tabs, stops running processes, and discards unsaved changes.':
      '此確認一律必要，因為移除會關閉所有分頁、停止執行中的程序並捨棄未儲存變更。',
  'Ask before removing a workspace worktree.': '移除工作區 Worktree 前先詢問。',
  'Tray icon and dock or taskbar badge while Alera is running.':
      'Alera 執行期間的系統匣圖示與 Dock 或工作列徽章。',
  'Keep Alera in the menu extra (macOS), notification area (Windows), or status bar (Ubuntu). Closing the window hides it; Quit from the tray or the app menu exits.': '讓 Alera 保留在 menu extra（macOS）、通知區域（Windows）或狀態列（Ubuntu）。關閉視窗只會隱藏程式；從系統匣或應用程式選單選擇結束才會退出。',
  'Keep Alera in the menu extra, notification area, or Ubuntu status bar.':
      '讓 Alera 保留在 menu extra、通知區域或 Ubuntu 狀態列。',
  'Show how many agents are waiting for review on the Dock, taskbar, or Ubuntu Dock.':
      '在 Dock、工作列或 Ubuntu Dock 顯示等待審查的 Agent 數量。',
  'Show how many agents are waiting for review on the Dock or taskbar.':
      '在 Dock 或工作列顯示等待審查的 Agent 數量。',
  'Draw how many agents are waiting for review onto the tray icon itself. Linux only; macOS and Windows show that count on the Dock or taskbar.':
      '直接在系統匣圖示上顯示等待審查的 Agent 數量。僅限 Linux；macOS 與 Windows 會在 Dock 或工作列顯示。',
  'Draw how many agents are waiting for review onto the tray icon.':
      '在系統匣圖示上顯示等待審查的 Agent 數量。',
  'Compact review and CI status for workspaces backed by a hosted Git repository.':
      '為使用託管 Git Repository 的工作區顯示精簡的審查與 CI 狀態。',
  'Show draft, ready, running, failed, merged, and closed state beside each workspace. Alera batches GitHub workspaces into one refresh per repository.':
      '在各工作區旁顯示草稿、就緒、執行中、失敗、已合併與已關閉狀態。Alera 會依 Repository 批次重新整理 GitHub 工作區。',
  'Show hosted review and CI state beside each workspace.':
      '在各工作區旁顯示託管審查與 CI 狀態。',
  'Show one native notification when a pull request enters a failed-check state. Enabling this keeps the lightweight monitor active while Alera is hidden.':
      'Pull Request 進入檢查失敗狀態時顯示一則原生通知。啟用後，即使 Alera 隱藏仍會保持輕量監控運作。',
  'Show a native notification when PR checks start failing.':
      'PR 檢查開始失敗時顯示原生通知。',
  'Lifecycle of the local runtime host that owns terminal sessions.':
      '管理終端機工作階段的本機 Runtime Host 生命週期。',
  'Prevent idle sleep and display sleep while Alera is running.':
      'Alera 執行期間防止閒置休眠與顯示器休眠。',
  'Prevents idle sleep and display sleep while Alera is running. Closing the lid still follows this device\'s power settings.':
      'Alera 執行期間防止閒置休眠與顯示器休眠；闔上上蓋仍依此裝置的電源設定運作。',
  'Leave the app-launched sidecar running after a clean quit.':
      '應用程式正常結束後仍讓其啟動的 sidecar 保持執行。',
  'Leave the app-launched sidecar running after a clean quit. Persistent CLI runtimes are never stopped by quitting, and unexpected exits always leave the host up.': '應用程式正常結束後仍讓其啟動的 sidecar 保持執行。持久化 CLI Runtime 不會因結束應用程式而停止，非預期退出也會保留 Host。',
  'Seconds to keep the host alive after the app closes with no running sessions.':
      '應用程式關閉且沒有執行中工作階段後，Host 保持存活的秒數。',
  'Stop the terminal host after the app closes with no sessions.':
      '應用程式關閉且沒有工作階段後停止 Terminal Host。',
  'Seconds to keep detached running sessions alive after the app closes.':
      '應用程式關閉後，分離且仍執行中的工作階段保持存活的秒數。',
  'Stop detached running terminal sessions after the app stays closed.':
      '應用程式持續關閉後停止分離且仍執行中的終端機工作階段。',
  'Show the folder holding the app log files.': '顯示存放應用程式 Log 檔案的資料夾。',
  'Could not open the logs folder.': '無法開啟 Log 資料夾。',
  'Save app and runtime logs with version details as a zip.':
      '將應用程式與 Runtime Log 連同版本資訊儲存為 ZIP。',
  'Diagnostics exported.': '診斷資料已匯出。',
  'Zip Archive': 'ZIP 壓縮檔',
  'How much detail is written to the log files.': '設定寫入 Log 檔案的詳細程度。',
  'Send crashes to Sentry, an external service.': '將當機資訊傳送至外部服務 Sentry。',
  'Check desktop releases for this platform.': '檢查此平台的桌面版 Release。',
  'Show your support for the project.': '表達你對此專案的支持。',
  'Star': '加星',
  'Starring…': '正在加星…',
  'Thanks for starring Alera': '感謝你為 Alera 加星',
  'Terminal colors, theme and spacing.': '終端機色彩、主題與間距。',
  'Default terminal typography for new sessions.': '新工作階段的預設終端機字型設定。',
  'Font Family': '字型家族',
  'Typeface used in new terminal sessions.': '新終端機工作階段使用的字型。',
  'Font Size': '字型大小',
  'Text size used in new terminal sessions.': '新終端機工作階段使用的文字大小。',
  'Font Weight': '字重',
  'Weight used for terminal text.': '終端機文字使用的字重。',
  'Line Height': '行高',
  'Vertical spacing for terminal rows.': '終端機列的垂直間距。',
  'Default cursor appearance for terminal sessions.': '終端機工作階段的預設游標外觀。',
  'Blinking Cursor': '閃爍游標',
  'Blink the cursor while the terminal has focus.': '終端機取得焦點時讓游標閃爍。',
  'Blink the terminal cursor while focused.': '終端機取得焦點時讓游標閃爍。',
  'Cursor Opacity': '游標不透明度',
  'Opacity of the terminal cursor.': '終端機游標的不透明度。',
  'Built-in terminal color theme.': '內建終端機配色主題。',
  'Background Opacity': '背景不透明度',
  'Opacity of the terminal background.': '終端機背景的不透明度。',
  'Horizontal Padding': '水平內距',
  'Horizontal spacing around the terminal grid.': '終端機網格周圍的水平間距。',
  'Vertical Padding': '垂直內距',
  'Vertical spacing around the terminal grid.': '終端機網格周圍的垂直間距。',
  'Foreground Color': '前景色',
  'Override the terminal text color.': '覆寫終端機文字顏色。',
  'Background Color': '背景色',
  'Override the terminal background color.': '覆寫終端機背景顏色。',
  'Cursor Color': '游標顏色',
  'Override the terminal cursor color.': '覆寫終端機游標顏色。',
  'Selection Color': '選取顏色',
  'Override the terminal selection color.': '覆寫終端機選取範圍顏色。',
  'Mouse, scrolling and clipboard behavior for TUIs.': 'TUI 的滑鼠、捲動與剪貼簿行為。',
  'TUI Scroll Speed': 'TUI 捲動速度',
  'Mouse reports sent per wheel step while a TUI owns scrolling.':
      'TUI 接管捲動時，每格滑鼠滾輪要送出的滑鼠事件數。',
  'Mouse wheel speed for interactive terminal applications.':
      '互動式終端機應用程式的滑鼠滾輪速度。',
  'Copy On Select': '選取時複製',
  'Copy local terminal selections to the system clipboard.':
      '將本機終端機選取內容複製到系統剪貼簿。',
  'Copy local terminal selections automatically.': '自動複製本機終端機選取內容。',
  'Allow OSC 52 Clipboard Writes': '允許 OSC 52 寫入剪貼簿',
  'Let terminal applications replace the system clipboard.':
      '允許終端機應用程式取代系統剪貼簿內容。',
  'Allow terminal applications to replace the clipboard.': '允許終端機應用程式取代剪貼簿內容。',
  'Show Terminal Composer By Default': '預設顯示終端機輸入區',
  'Open the prompt composer when a new terminal session starts.':
      '新的終端機工作階段開始時開啟 Prompt 輸入區。',
  'History, shell startup and double-click selection behavior.':
      '歷史紀錄、Shell 啟動與雙擊選取行為。',
  'Use Login Shell': '使用 Login Shell',
  'Start shells as login shells so profile files such as ~/.zprofile and ~/.profile are loaded.':
      '以 Login Shell 啟動 Shell，以載入 ~/.zprofile、~/.profile 等設定檔。',
  'Reload Shell Environment': '重新載入 Shell 環境',
  'Re-read the login shell PATH so tools installed since the runtime started resolve in new terminals.':
      '重新讀取 Login Shell 的 PATH，讓 Runtime 啟動後新安裝的工具可在新終端機中解析。',
  'Reload': '重新載入',
  'Scrollback Lines': '回捲行數',
  'Maximum terminal history retained per session.': '每個工作階段保留的終端機歷史最大行數。',
  'Host Scrollback Size': 'Host 回捲大小',
  'Maximum host-side terminal output retained per session.':
      '每個工作階段在 Host 端保留的終端機輸出上限。',
  'Terminal Memory Budget': '終端機記憶體預算',
  'Word Separators': '單字分隔字元',
  'Characters that break double-click word selection.': '雙擊選取單字時視為分隔的字元。',
  'Color Overrides': '色彩覆寫',
  'Override core terminal colors.': '覆寫終端機主要色彩。',
  'Terminal Shortcut Behavior': '終端機快速鍵行為',
  'View and remap app-wide key bindings.': '檢視並重新對應應用程式全域按鍵綁定。',
  'Syntax highlighting theme used by editor tabs.': '編輯器分頁使用的語法醒目提示主題。',
  'Spaces inserted when pressing tab in editor tabs.': '在編輯器分頁按下 Tab 時插入的空白數。',
  'Automatically save dirty editor tabs after a pause.':
      '暫停操作一段時間後自動儲存有變更的編輯器分頁。',
  'Project Worktree Setup': '專案 Worktree 設定',
  'Configure copy rules, setup commands, and new workspace prompts.':
      '設定複製規則、Setup 指令與新工作區 Prompt。',
  'Add a project before configuring workspace setup.': '請先新增專案，再設定工作區。',
  'UI overrides take precedence over repo files.':
      'UI 覆寫設定的優先順序高於 Repository 檔案。',
  'Config Source': '設定來源',
  'UI Override': 'UI 覆寫',
  'Hosting Provider': '託管 Provider',
  'Git hosting provider used for pull requests and checks.':
      'Pull Request 與檢查使用的 Git 託管 Provider。',
  'Auto-Detect': '自動偵測',
  'Auto-detect uses public hosts. Select GitHub for GitHub Enterprise Server.':
      '自動偵測會使用公開 Host；GitHub Enterprise Server 請選擇 GitHub。',
  'Copy Rules': '複製規則',
  'Files copied from the main worktree. Gitignored matches from .worktreeinclude are copied too.':
      '從主 Worktree 複製的檔案；.worktreeinclude 中符合但被 Git 忽略的檔案也會複製。',
  'Setup Commands': 'Setup 指令',
  'Commands run from the new linked workspace.': '從新的 linked workspace 執行的指令。',
  'Project instructions appended to prompts that start an agent.':
      '附加到啟動 Agent Prompt 的專案指示。',
  'Repo file error': 'Repository 檔案錯誤',
  'No projects': '尚無專案',
  'Manage SSH targets and remote runtime bootstrap.':
      '管理 SSH 目標與遠端 Runtime Bootstrap。',
  'Mobile access unavailable': '無法使用行動裝置存取',
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
  'Enable and configure the mobile companion gateway.':
      '啟用並設定行動版輔助應用程式 Gateway。',
  'Connected Remote Devices': '已連線的遠端裝置',
  'Connected through your Alera account. Disable Remote Access to disconnect these devices.':
      '這些裝置透過你的 Alera 帳號連線；停用遠端存取即可中斷連線。',
  'Endpoint': 'Endpoint',
  'Optional expected name for the new device.': '新裝置的選用預期名稱。',
  'Expires In': '有效時間',
  'Minutes before the offer expires.': '配對邀請到期前的分鐘數。',
  'Generate Pairing QR': '產生配對 QR Code',
  'Enables the gateway if it is disabled.': '若 Gateway 尚未啟用，會一併啟用。',
  'Generate a pairing QR for the mobile companion app.':
      '為行動版輔助應用程式產生配對 QR Code。',
  'Generate a pairing QR to link a new device.': '產生配對 QR Code 以連結新裝置。',
  'No active offers': '目前沒有有效邀請',
  'Devices that can connect to this runtime.': '可連線到此 Runtime 的裝置。',
  'No paired devices': '尚無已配對裝置',
  'Link a device to see it here.': '連結裝置後會顯示在這裡。',
  'Rename, revoke, or delete paired mobile devices.': '重新命名、撤銷或刪除已配對的行動裝置。',
  'Link Mobile Device': '連結行動裝置',
  'Cancel Pairing Offer': '取消配對邀請',
  'The offer becomes unusable immediately.': '此邀請會立即失效。',
  'Revoked': '已撤銷',
  'Revoke': '撤銷',
  'Installer Output': '安裝程式輸出',
  'The installer runs here and skips confirmation prompts.':
      '安裝程式會在此執行並略過確認提示。',
  'Try again': '再試一次',
  'Declare a profile to let a run dispatch work to it.':
      '建立 Profile，讓執行工作可以派送給它。',
  'Where new linked workspaces are created on disk.':
      '新 linked workspace 在磁碟上的建立位置。',
  'Install or update every core Alera agent skill.':
      '安裝或更新所有核心 Alera Agent Skill。',
  'Install agent instructions for the Alera CLI.': '安裝 Alera CLI 的 Agent 指示。',
  'Install agent instructions for Alera orchestration.':
      '安裝 Alera 協調功能的 Agent 指示。',
  'Install specialized instructions for Agent Profile catalogs.':
      '安裝 Agent Profile Catalog 專用指示。',
  'Use Alera-managed Codex runtime hooks.': '使用由 Alera 管理的 Codex Runtime Hook。',
  'Install managed Antigravity hooks for the agy CLI.':
      '為 agy CLI 安裝受管理的 Antigravity Hook。',
  'Install managed OpenCode status plugin.': '安裝受管理的 OpenCode 狀態 Plugin。',
  'Install managed OpenCode 2 status plugin.': '安裝受管理的 OpenCode 2 狀態 Plugin。',
  'Install managed Pi status extension.': '安裝受管理的 Pi 狀態 Extension。',
  'Install managed Grok Build status hooks.': '安裝受管理的 Grok Build 狀態 Hook。',
  'Choose quota providers and their display order.': '選擇配額 Provider 及其顯示順序。',
  'Create a reusable text replacement action.': '建立可重複使用的文字替換動作。',
  'Show an action in the Text Actions menu.': '在 Text Actions 選單中顯示動作。',
  'Choose the CLI and model for an action.': '選擇動作使用的 CLI 與模型。',
  'Copy actions and arrange their menu order.': '複製動作並調整其選單順序。',
  'Remove a text action after confirmation.': '確認後移除文字動作。',
  'If this is useful, consider starring the repo. It helps more developers discover it.':
      '如果 Alera 對你有幫助，可以考慮在 GitHub 為 Repository 加星，讓更多開發者能發現它。',
  'Keeps this computer and display awake while agents are working. Lid-close behavior follows this device\'s power settings.':
      'Agent 工作期間讓此電腦與顯示器保持喚醒；闔上上蓋時仍依此裝置的電源設定運作。',
  'Keeps this computer and display awake while agents are working. Alera also asks this device to stay awake when the lid is closed, subject to its power policy.':
      'Agent 工作期間讓此電腦與顯示器保持喚醒；闔上上蓋時 Alera 也會要求裝置保持喚醒，但仍受其電源政策限制。',
  'Ceiling for terminal scrollback held in the app. Over it, terminals you have not looked at recently are unloaded and restored when you return. Their agents keep running. Only panes currently on screen stay loaded over it. Use 0 for no limit.': '應用程式中終端機回捲內容的記憶體上限。超過上限時，最近未檢視的終端機會卸載，返回時再還原；其中的 Agent 仍會繼續執行。只有目前畫面上的 Pane 可超過此上限保持載入。設為 0 表示不限制。',
  'The device loses access and active sessions disconnect immediately. This cannot be undone.':
      '此裝置會失去存取權，作用中的工作階段會立即中斷連線。此操作無法復原。',
  'Permanently removes this revoked device record from the list. This cannot be undone.':
      '從清單永久移除此已撤銷的裝置紀錄。此操作無法復原。',
  'Agent Profile In Use': 'Agent Profile 使用中',
  'Delete Agent Profile?': '刪除 Agent Profile？',
  'Name is required.': '名稱為必填。',
  'Command is required.': '指令為必填。',
  'This profile reduces Codex approval or sandbox protections.':
      '此 Profile 會降低 Codex 的核准或沙盒保護。',
  'This profile will bypass Codex approvals and sandbox protections.':
      '此 Profile 會略過 Codex 的核准與沙盒保護。',
  'This profile lets Claude continue with reduced permission prompts.':
      '此 Profile 會讓 Claude 在較少權限提示的情況下繼續執行。',
  'This profile lets Copilot take broader actions with less supervision.':
      '此 Profile 會讓 Copilot 在較少監督下執行更廣泛的操作。',
  'This profile reduces Cursor review, sandbox, or trust protections.':
      '此 Profile 會降低 Cursor 的審查、沙盒或信任保護。',
  'This profile lets Antigravity skip permission checks.':
      '此 Profile 會讓 Antigravity 略過權限檢查。',
  'This profile lets OpenCode approve actions automatically.':
      '此 Profile 會讓 OpenCode 自動核准操作。',
  'This profile pre-approves project trust for Pi.':
      '此 Profile 會預先核准 Pi 的專案信任。',
  'This profile lets Grok Build continue with reduced permission prompts.':
      '此 Profile 會讓 Grok Build 在較少權限提示的情況下繼續執行。',
  'This profile lets Devin take broader actions with less supervision.':
      '此 Profile 會讓 Devin 在較少監督下執行更廣泛的操作。',
  'No quota providers enabled': '尚未啟用配額 Provider',
  'No CCS profiles configured': '尚未設定 CCS Profile',
  'Not shown in Usage': '不顯示於用量',
  'Alias and profile are required.': '別名與 Profile 為必填。',
  'Alias and profile must be unique.': '別名與 Profile 不可重複。',
  'Register': '註冊',
  'Connected through relay': '透過 Relay 連線',
  'Expired': '已過期',
  'Offer expired': '配對邀請已過期',
  'Scan with the Alera mobile app': '使用 Alera 行動版 App 掃描',
  'No matching options': '沒有符合的選項',
  'Language Intelligence': '語言智慧',
  'Enable project-aware definition and reference navigation per language. Semantic servers stay off until you enable them.':
      '依語言啟用專案感知的定義與引用導覽。語意伺服器預設關閉，只有你啟用後才會使用。',
  'Semantic navigation is disabled until you enable this language.':
      '此語言啟用前，不會使用語意導覽。',
  'Semantic Server': '語意伺服器',
  'Structural Parser': '結構解析器',
  'Retained parser for syntax, outline, folding, and structural editing.':
      '保留式解析器，用於語法、程式大綱、摺疊與結構化編輯。',
  'Executable Override': '執行檔覆寫',
  'Leave blank to use PATH first, then Alera-managed installation when supported.':
      '留空會先使用這台裝置 PATH 中的 Provider 指令；若支援，找不到時再使用 Alera 管理的安裝。',
  'Status': '狀態',
  'Ready': '就緒',
  'Missing': '缺少',
  'Check Again': '重新檢查',
};
