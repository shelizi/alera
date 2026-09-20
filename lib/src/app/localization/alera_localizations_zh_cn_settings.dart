part of 'alera_localizations.dart';

const Map<String, String> _simplifiedChineseSettings = <String, String>{
  // Settings sections.
  'Configuration Sync': '设置同步',
  'Account': '账号',
  'Application': '应用程序',
  'Agents': '代理',
  'Quotas': '配额',
  'AI Assist': 'AI 辅助',
  'AI Dictation': 'AI 听写',
  'Text Actions': '文本操作',
  'Editor': '编辑器',
  'Terminal': '终端',
  'Keyboard': '键盘',
  'Projects': '项目',
  'Mobile Devices': '移动设备',
  'Remote Hosts': '远程主机',
  'Agent Profiles': '代理配置文件',

  // Settings groups and common rows.
  'Language': '语言',
  'App Language': '应用语言',
  'Storage': '存储空间',
  'Safety': '安全',
  'Desktop': '桌面',
  'Runtime': '运行时',
  'Diagnostics': '诊断',
  'Updates': '更新',
  'Support': '支持',
  'Identity': '身份',
  'Account unavailable': '无法取得账号',
  'Your Alera identity protects cloud delivery and stays optional for local features.':
      '你的 Alera 身份可保护云端发送，本地功能仍可不登录使用。',
  'Continue With Google': '使用 Google 继续',
  'Sign in through your default browser.': '通过默认浏览器登录。',
  'Continue With GitHub': '使用 GitHub 继续',
  'Uses profile and verified email access only. Repository access is never requested.':
      '只会访问个人资料与已验证的电子邮件，不会要求 Repository 访问权限。',
  'Alera Account': 'Alera 账号',
  'Add another verified sign-in method to this account.': '为此账号添加另一个已验证的登录方式。',
  'Sign Out': '退出登录',
  'Stops cloud push delivery from this runtime until you sign in again.':
      '在你再次登录前，停止从此运行时发送云端推送。',
  'Browser Sign In': '浏览器登录',
  'A provider authorization is waiting in your browser.':
      '浏览器中正等待 Provider 授权。',
  'Notifications are delivered only to mobile devices enrolled in this account.':
      '通知只会发送到已加入此账号的移动设备。',
  'Enable Mobile Push': '启用移动设备推送',
  'Sign in before enabling cloud delivery.': '请先登录，再启用云端发送。',
  'Attention Required': '需要注意',
  'Notify for waiting or blocked agents, decision gates, and escalations.':
      '当 Agent 等待中、受阻、需要决策或升级处理时通知。',
  'Agent Finished': 'Agent 已完成',
  'Notify when an agent finishes a turn.': 'Agent 完成一个回合时通知。',
  'Terminal Ended': '终端已结束',
  'Notify when a terminal session exits or is closed.': '终端会话结束或关闭时通知。',
  'Move this runtime to another account or remove your cloud identity.':
      '将此运行时转移到其他账号，或移除你的云端身份。',
  'Target Account ID': '目标账号 ID',
  'Moving a runtime signs this installation out and requires authentication again.':
      '转移运行时会让此安装退出登录，之后需要重新验证。',
  'Account ID': '账号 ID',
  'Move This Runtime': '转移此运行时',
  'Transfer runtime ownership and its mobile subscriptions.':
      '转移运行时的所有权及其移动设备订阅。',
  'Move Runtime': '转移运行时',
  'Delete Alera Account': '删除 Alera 账号',
  'Permanently removes provider identities, cloud sessions, subscriptions, and quota records.':
      '永久移除 Provider 身份、云端会话、订阅与配额记录。',
  'Delete Account': '删除账号',
  'This permanently removes your Alera cloud identity, active sessions, mobile subscriptions, and quota records. Recent sign-in may be required.':
      '这会永久移除你的 Alera 云端身份、作用中的会话、移动设备订阅与配额记录。可能需要近期登录验证。',

  'Mobile Push': '移动推送',
  'Ownership': '所有权',
  'CLI And Skills': 'CLI 与技能',
  'Extra Skills': '额外技能',
  'Status Hooks': '状态 Hook',
  'Behavior': '行为',
  'Providers': '供应商',
  'Credentials': '凭据',
  'Actions': '操作',
  'Generation': '生成',
  'Commit Messages': 'Commit 消息',
  'Pull Request Details': 'Pull Request 详细信息',
  'Reading Diffs': '阅读差异',
  'Workspace Identity': '工作区标识',
  'Transcription': '转录',
  'Remote Transcription': '远程转录',
  'Local Whisper Models': '本地 Whisper 模型',
  'Speech Processing': '语音处理',
  'Test AI Dictation': '测试 AI 听写',
  'Choose where speech is converted to text on this device.':
      '选择要在此设备的哪个位置将语音转换成文本。',
  'Enable AI Dictation': '启用 AI 听写',
  'Show microphone controls in supported composers.': '在支持的输入区显示麦克风控件。',
  'Transcription Engine': '转录引擎',
  'Optional locale or language code. Leave blank for automatic detection.':
      '可选的地区或语言代码；留空会自动检测。',
  'Allow Online Speech Recognition': '允许在线语音识别',
  'Windows may send microphone audio to Microsoft to create the transcription.':
      'Windows 可能会将麦克风音频发送给 Microsoft 以生成转录。',
  'The system recognizer may send microphone audio to its online speech service.':
      '系统识别器可能会将麦克风音频发送到其在线语音服务。',
  'Install multiple multilingual models and select one for local transcription.':
      '可安装多个多语言模型，并选择一个用于本地转录。',
  'Optionally improve the transcript with the agent subscription configured for Speech Messages in AI Assist settings.':
      '可选择使用 AI 辅助设置中“语音消息”所设置的 Agent 订阅来改善转录内容。',
  'Automatic Processing': '自动处理',
  'Raw text is always used if the selected agent is unavailable or fails.':
      '若所选 Agent 无法使用或处理失败，会一律使用原始文本。',
  'Off': '关闭',
  'Clean Up': '整理',
  'Summarize': '摘要',
  'Local Whisper': '本地 Whisper',
  'Codex Subscription (Experimental)': 'Codex 订阅（实验性）',
  'OpenAI-Compatible API': 'OpenAI 兼容 API',
  'System On-Device': '系统设备端',
  'System Recognition': '系统语音识别',
  'Record locally and transcribe with the selected Whisper model.':
      '在本地录音，并使用所选 Whisper 模型转录。',
  'Use the experimental Codex app-server realtime API with your Codex subscription.':
      '使用 Codex 订阅搭配实验性的 Codex app-server realtime API。',
  'Send recordings to an OpenAI-compatible audio transcription endpoint.':
      '将录音发送到 OpenAI 兼容的音频转录端点。',
  'Use the platform recognizer only when it guarantees offline processing.':
      '仅在平台识别器保证离线处理时使用。',
  'Use the platform speech service, which may process audio online.':
      '使用平台语音服务；音频可能会在线处理。',
  'Send recordings to Codex or an OpenAI-compatible speech API. Transcription endpoints do not use reasoning effort.':
      '将录音发送到 Codex 或 OpenAI 兼容语音 API；转录端点不使用推理强度。',
  'Runtime Update Required': '需要更新运行时',
  'Restart Alera to replace the running sidecar before configuring remote transcription.':
      '设置远程转录前，请重启 Alera 以替换正在执行的 sidecar。',
  'Allow Remote Audio Processing': '允许远程音频处理',
  'Recordings may leave this device and are deleted locally after transcription.':
      '录音可能会离开此设备，并在转录完成后从本地删除。',
  'Realtime Model': 'Realtime 模型',
  'Optional Codex realtime model override. Leave blank to use the subscription default. This Codex API is experimental.':
      '可选择覆盖 Codex realtime 模型；留空使用订阅默认值。此 Codex API 为实验性。',
  'Subscription default': '订阅默认值',
  'Base URL': 'Base URL',
  'Base API URL. Alera appends /audio/transcriptions when needed and preserves query parameters.':
      'API Base URL。Alera 会在需要时附加 /audio/transcriptions，并保留 query parameters。',
  'Speech-to-text model accepted by the configured API.':
      '设置的 API 所接受的语音转文本模型。',
  'Request Timeout': '请求超时',
  'Maximum time allowed for remote transcription.': '远程转录允许的最长时间。',
  'API Token': 'API Token',
  'Checking saved token...': '正在检查已保存的 Token…',
  'The saved token belongs to another API origin. Replace it before transcribing.':
      '已保存的 Token 属于另一个 API origin；转录前请先替换。',
  'A token is stored for this API origin.': '此 API origin 已保存 Token。',
  'No token is stored. Tokenless local APIs are also supported.':
      '尚未保存 Token；也支持不需要 Token 的本地 API。',
  'Replace saved token': '替换已保存的 Token',
  'Replace Token': '替换 Token',
  'Save Token': '保存 Token',
  'Test Transcript': '测试转录',
  'Record a short sample with the current configuration and review the transcript here.':
      '使用当前设置录制短音频，并在此查看转录结果。',
  'Your test transcription appears here': '测试转录结果会显示在这里',
  'Enable AI Dictation before testing.': '测试前请先启用 AI 听写。',
  'Restart Alera to update the runtime before testing remote transcription.':
      '测试远程转录前，请重启 Alera 以更新运行时。',
  'Allow remote audio processing before testing this engine.':
      '测试此引擎前，请先允许远程音频处理。',
  'Select the microphone, speak, then select Stop Dictation.':
      '选择麦克风、开始说话，完成后选择“停止听写”。',
  'Queue Download': '排入下载队列',
  'Selected': '已选择',
  'Use Model': '使用模型',
  'Queued. This download starts when the active transfer finishes.':
      '已排入队列；当前的传输完成后会开始下载。',
  'Verifying downloaded model...': '正在验证下载的模型…',
  'The model download failed.': '模型下载失败。',
  'Installed and selected.': '已安装并选择。',
  'Installed on this device.': '已安装在此设备。',
  'Fastest, with lower transcription accuracy.': '速度最快，但转录准确度较低。',
  'Balanced speed and accuracy. Recommended for most devices.':
      '兼顾速度与准确度，建议大多数设备使用。',
  'Improved accuracy with slower transcription.': '准确度较高，但转录速度较慢。',
  'Highest curated accuracy with the largest memory cost.':
      '提供最高的精选准确度，但内存占用也最大。',
  'The model download could not finish. Try again.': '模型下载未能完成，请再试一次。',
  'Select another installed model before removing this one.':
      '移除此模型前，请先选择另一个已安装的模型。',
  'Improving Transcript': '正在改善转录内容',
  'Cancel Transcription': '取消转录',
  'Stop Dictation': '停止听写',
  'Start Dictation': '开始听写',
  'Remote audio processing was disabled before transcription.':
      '远程音频处理已在转录前禁用。',
  'Enable AI Dictation in Settings before recording.': '录音前请先在设置中启用 AI 听写。',
  'The dictation text field is no longer available.': '听写文本字段已无法使用。',
  'Download the selected Whisper model in Settings before recording.':
      '录音前请先在设置中下载所选的 Whisper 模型。',
  'Allow remote audio processing in AI Dictation settings first.':
      '请先在 AI 听写设置中允许远程音频处理。',
  'Microphone permission is required for AI Dictation.': 'AI 听写需要麦克风权限。',
  'On-device speech recognition is unavailable for this locale.':
      '此地区设置无法使用设备端语音识别。',
  'Allow online speech recognition in AI Dictation settings first.':
      '请先在 AI 听写设置中允许在线语音识别。',
  'The system recognizer did not produce a transcription.': '系统识别器未生成转录内容。',
  'The microphone did not produce an audio recording.': '麦克风未生成音频录音。',
  'The text field was closed before dictation finished.': '听写完成前文本字段已关闭。',
  's': '秒',

  'Typography': '字体',
  'Cursor': '光标',
  'Appearance': '外观',
  'Interaction': '互动',
  'Advanced': '高级',
  'Mobile Gateway': '移动网关',
  'Link A Device': '关联设备',
  'Active Pairing Offers': '有效的配对邀请',
  'Paired Devices': '已配对设备',
  'Indentation': '缩进',
  'Autosave': '自动保存',
  'Tab Size': 'Tab 宽度',
  'Control dependency and build directories that Quick Open never indexes.':
      '管理快速打开永远不创建索引的依赖库与构建目录。',
  'Excluded Directory Names': '排除的目录名称',
  'These directory names are never indexed, even when Git-ignored files are included. Matching is case-insensitive.':
      '即使包含 Git 忽略的文件，这些目录名称也永远不会创建索引；匹配不区分大小写。',
  'Directory name, e.g. generated': '目录名称，例如 generated',
  '.git, .hg, and .svn are always excluded.': '.git、.hg、.svn 永远固定排除。',
  'Restore Defaults': '恢复默认值',
  'PowerShell 7 Executable': 'PowerShell 7 可执行文件',
  'Optional full path to pwsh.exe on Windows. Leave blank to auto-detect standard, Scoop, LocalAppData, and PATH locations.':
      'Windows 上 pwsh.exe 的完整路径（可选）。留空会自动检测标准安装位置、Scoop、LocalAppData 与 PATH。',
  'Override or auto-detect the Windows pwsh.exe path.':
      '指定 Windows pwsh.exe 路径，或使用自动检测。',
  'Theme Preset': '主题预设',
  'Search and select a built-in terminal color theme.': '搜索并选择内建的终端配色主题。',
  'Cursor Shape': '光标形状',
  'Cursor style for new terminal sessions.': '新终端会话使用的光标样式。',
  'Toolbar Corner': '工具栏位置',
  'Where the pulse, composer, and refresh buttons sit on the terminal tab.':
      '设置终端标签页中的活动指示器、输入区与刷新按钮位置。',
  'Top Left': '左上',
  'Top Right': '右上',
  'Bottom Left': '左下',
  'Bottom Right': '右下',
  'Select color': '选择颜色',
  'Choose color': '选择颜色',
  'spaces': '个空格',
  'seconds': '秒',
  'Autosave Delay': '自动保存延迟',
  'Follow System': '跟随系统',
  'English': 'English',
  '繁體中文': '繁體中文',

  // Settings descriptions.
  'Language used by the Alera interface.': 'Alera 界面使用的语言。',
  'Follow the system language or choose a language for Alera.':
      '跟随系统语言，或为 Alera 指定语言。',
  'Review, download and upload configuration across your devices.':
      '查看、下载与上传不同设备间的设置。',
  'Identity, mobile push and runtime ownership.': '身份、移动推送与运行时所有权。',
  'Storage, safety, runtime, diagnostics and updates.': '存储空间、安全、运行时、诊断与更新。',
  'Agent hooks, notifications and Alera skills.': '代理 Hook、通知与 Alera 技能。',
  'Provider usage, Claude profiles and credential environment.':
      '供应商用量、Claude 配置文件与凭据环境。',
  'Local agent assistance for commits, pull requests, diffs, workspace identity, and speech.':
      '用于 Commit、Pull Request、差异、工作区标识与语音的本地代理辅助。',
  'Local, Codex subscription, and OpenAI-compatible speech-to-text.':
      '本地、Codex 订阅与 OpenAI 兼容的语音转文本。',
  'Create reusable replacements for selected text.': '为选择的文本创建可重复使用的替换操作。',
  'Code editor defaults.': '代码编辑器默认值。',
  'Appearance defaults for new terminal sessions.': '新终端会话的外观默认值。',
  'Shortcuts and key bindings.': '快捷键与按键绑定。',
  'Per-project workspace setup.': '各项目的工作区设置。',
  'Pair and manage the mobile companion app.': '配对并管理移动版辅助应用程序。',
  'SSH runtime targets.': 'SSH 执行目标。',
  'Launch configurations orchestration can dispatch to.': '可由协调流程派送的启动设置。',
  'Syntax highlighting defaults for editor tabs.': '编辑器标签页的语法高亮默认值。',
  'Defaults used by editor tabs.': '编辑器标签页使用的默认值。',
  'Spaces inserted when pressing tab.': '按下 Tab 时插入的空格数。',
  'Save dirty editor tabs after they have been idle.': '编辑器标签页空闲后自动保存未保存的变更。',
  'Automatically save editor changes after a pause.': '暂停操作一段时间后自动保存编辑器变更。',
  'Idle time before saving editor changes.': '保存编辑器变更前的空闲时间。',
  'Search and select a syntax highlighting theme.': '搜索并选择语法高亮主题。',
  'External Editor': '外部编辑器',
  'Open workspaces and files in an external editor without changing Alera\'s built-in editor behavior.':
      '在外部编辑器中打开工作区与文件，不变更 Alera 内建编辑器的行为。',
  'Editor used by Open In menu entries, keyboard shortcuts, and external file targets.':
      'Open In 菜单项目、键盘快捷键与外部文件目标所使用的编辑器。',
  'Default Code Open Target': '默认代码打开目标',
  'Choose where normal editable source and text files open. Dedicated Alera previews stay internal.':
      '选择一般可编辑源代码与文本文件的打开位置；Alera 专用预览仍保留在内部。',
  'Open Workspaces in New Window': '在新窗口打开工作区',
  'Auto-open New Workspaces Externally': '自动在外部编辑器打开新工作区',
  'Run a non-destructive version check with the current executable setting.':
      '使用当前的可执行文件设置执行不会修改数据的版本检查。',
  'Search syntax themes': '搜索语法主题',
  'Prompt Append': '附加提示词',
  'Add project-specific agent instructions': '添加项目专属的 Agent 指示',
  'From': '来源',
  'To': '目标',
  'Defaults to from': '默认与来源相同',
  'Save Override': '保存覆盖设置',
  'Custom Prompt': '自定义提示词',
  'Optional instructions for every dispatched task': '每个派送工作可选用的额外指示',
  'Quota Group': '配额组',
  'Command mode is for advanced or unsupported CLI options. Use an interactive command that can accept a dispatch and report completion.':
      'Command 模式适用于高级或尚未支持的 CLI 选项。请使用可接收派送工作并回报完成状态的互动式命令。',
  'Profiles sharing a quota group drain the same usage bucket. Alera never measures this; it only avoids falling back inside the same group. Leave empty if unsure.':
      '共享同一配额组的配置文件会消耗相同的用量额度。Alera 不会测量额度，只会避免在同组内进行 fallback；若不确定请留空。',
  'Alias': '别名',
  'CCS Profile': 'CCS 配置文件',
  'Usage Name': '用量显示名称',
  'Device Name': '设备名称',
  'My Phone': '我的手机',
  'Generating…': '生成中…',
  'Generate': '生成',
  'Host': '主机',
  'Username': '用户名',
  'Port': '端口',
  'Install Directory': '安装目录',
  'Default per platform': '按平台使用默认值',
  'Search built-in themes': '搜索内建主题',
  'Initial Prompt': '初始提示词',
  'Describe what the agent should build or paste an image':
      '描述要让 Agent 创建的内容，或粘贴图片',
  'Loading branches': '正在加载 Branch',
  'Select Branch': '选择 Branch',
  'Create an agent profile in settings': '请先在设置中创建 Agent 配置文件',
  'Select Agent Profile': '选择 Agent 配置文件',
  'Create Another': '继续创建下一个',
  'Working': '处理中',
  'Complete the prompt, project, branch, and agent profile.':
      '请完成提示词、项目、Branch 与 Agent 配置文件。',
  'Generating workspace identity': '正在生成工作区标识',
  'Checking generated branch': '正在检查生成的 Branch',
  'Creating workspace': '正在创建工作区',
  'Starting agent': '正在启动 Agent',
  'Could not paste clipboard image.': '无法粘贴剪贴板图片。',
  'Choose every workspace setting yourself, including the branch name and optional parent workspace.':
      '自行选择所有工作区设置，包括 Branch 名称与可选的父工作区。',
  'Describe the replacement to generate.': '描述要生成的替换内容。',
  'Define the reusable instruction and its availability.': '定义可重复使用的指示及其可用状态。',
  'Show this action in the Text Actions menu.': '在 Text Actions 菜单中显示此操作。',
  'Choose which CLI and model run this action.': '选择执行此操作的 CLI 与模型。',
  'Inherit the global AI Assist agent by default.': '默认继承全局 AI Assist Agent。',
  'Inherit the selected model unless overridden.': '除非覆盖，否则继承当前选择的模型。',
  'Reasoning effort for the effective model.': '设置实际使用模型的推理强度。',
  'Action': '操作',
  'Enabled': '已启用',
  'Reasoning': '推理',
  'No text actions': '尚无文本操作',
  'Select a text action': '选择文本操作',
  'Delete Text Action': '删除文本操作',
  'Action ID is required.': '操作 ID 为必填。',
  'Action name is required.': '操作名称为必填。',
  'Action prompt is required.': '操作提示词为必填。',
  'Action IDs must be unique.': '操作 ID 不可重复。',
  'Action names must be unique.': '操作名称不可重复。',
  'Text changed while the action was running.': '执移动作期间文本已变更。',
  'Text action returned no replacement text.': '文本操作未返回可替换的文本。',
  'Text action could not update this field.': '文本操作无法更新此字段。',
  'Text action applied.': '已应用文本操作。',
  'Text action was canceled.': '已取消文本操作。',
  'No matching options': '没有匹配的选项',
};
