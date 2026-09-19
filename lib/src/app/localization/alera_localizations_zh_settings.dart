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
  'No matching options': '沒有符合的選項',
};
