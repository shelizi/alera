part of 'alera_localizations.dart';

const Map<String, String> _simplifiedChineseSettingsExtra = <String, String>{
  // Settings navigation and application.
  'Workspaces': '工作区',
  'Workspace Directory': '工作区目录',
  'Where new linked workspaces are created on disk. Existing workspaces are not moved. Leave empty to use the default (~/.alera/workspaces).':
      '新建关联工作区在磁盘上的存放位置。现有工作区不会移动。留空会使用默认位置（~/.alera/workspaces）。',
  'Automatic archiving for inactive workspaces.': '自动归档空闲的工作区。',
  'Auto-Archive After Inactivity': '空闲后自动归档',
  'Days without activity before a workspace moves to the Archived section. Set to 0 to keep workspaces listed.':
      '工作区多久未活动后移至“已归档”区段。设为 0 可让工作区持续显示。',
  'Confirmation prompts for destructive workspace actions.':
      '针对破坏性工作区操作显示确认提示。',
  'Confirm Project Removal': '确认移除项目',
  'Ask before unregistering a project and deleting its workspace metadata.':
      '取消注册项目并删除其工作区元数据前先询问。',
  'Confirm Workspace Removal': '确认移除工作区',
  'Always required because removal closes all tabs, stops running processes, and discards unsaved changes.':
      '移除会关闭所有标签页、停止运行中的程序并丢弃未保存变更，因此一律需要确认。',
  'Tray icon and dock or taskbar badge while Alera is running.':
      'Alera 执行时的系统托盘图标与 Dock／任务栏徽章。',
  'Show Tray Icon': '显示系统托盘图标',
  'Keep Alera in the menu extra (macOS), notification area (Windows), or status bar (Ubuntu). Closing the window hides it; Quit from the tray or the app menu exits.': '让 Alera 保留在 macOS 菜单栏、Windows 通知区或 Ubuntu 状态栏。关闭窗口只会隐藏；从系统托盘或应用程序菜单选择结束才会退出。',
  'Show Dock Badge': '显示 Dock／任务栏徽章',
  'Show how many agents are waiting for review on the Dock, taskbar, or Ubuntu Dock.':
      '在 Dock、任务栏或 Ubuntu Dock 显示等待审阅的 Agent 数量。',
  'Show Tray Badge': '显示系统托盘徽章',
  'Draw how many agents are waiting for review onto the tray icon itself. Linux only; macOS and Windows show that count on the Dock or taskbar.':
      '直接在系统托盘图标显示等待审阅的 Agent 数量。仅限 Linux；macOS 与 Windows 会显示在 Dock 或任务栏。',
  'Compact review and CI status for workspaces backed by a hosted Git repository.':
      '显示由托管 Git Repository 支持之工作区的精简审阅与 CI 状态。',
  'Show Pull Request Status': '显示 Pull Request 状态',
  'Show draft, ready, running, failed, merged, and closed state beside each workspace. Alera batches GitHub workspaces into one refresh per repository.':
      '在每个工作区旁显示草稿、就绪、运行中、失败、已合并与已关闭状态。Alera 会按 Repository 合并 GitHub 工作区的刷新。',
  'Notify When Checks Fail': '检查失败时通知',
  'Show one native notification when a pull request enters a failed-check state. Enabling this keeps the lightweight monitor active while Alera is hidden.':
      'Pull Request 进入检查失败状态时显示一次原生通知。启用后，即使 Alera 隐藏仍会维持轻量监控。',
  'Lifecycle of the local runtime host that owns terminal sessions.':
      '管理终端会话之本地 Runtime Host 的生命周期。',
  'Keep Computer Awake': '保持电脑唤醒',
  'Prevents idle sleep and display sleep while Alera is running. Closing the lid still follows this device\'s power settings.':
      'Alera 运行时防止空闲睡眠与屏幕休眠。合上上盖仍按此设备的电源设置处理。',
  'Keep Runtime Open When App Quits': '应用程序结束后保持 Runtime 执行',
  'Leave the app-launched sidecar running after a clean quit. Persistent CLI runtimes are never stopped by quitting, and unexpected exits always leave the host up.':
      '正常结束应用程序后，保留由应用程序启动的 Sidecar。持久型 CLI Runtime 不会因退出而停止，非预期退出也会保留 Host。',
  'Empty Host Shutdown': '空闲 Host 关闭',
  'Seconds to keep the host alive after the app closes with no running sessions.':
      '应用程序关闭且没有运行中会话时，Host 继续存活的秒数。',
  'Detached Session Shutdown': '分离会话关闭',
  'Seconds to keep detached running sessions alive after the app closes.':
      '应用程序关闭后，分离且仍在执行的会话继续存活的秒数。',

  // Diagnostics, support, and updates.
  'Alera keeps rotating log files on this computer so an error can be investigated after it happens.':
      'Alera 会在这台电脑上轮替保存 Log，方便在错误发生后进行调查。',
  'Open Logs Folder': '打开 Log 文件夹',
  'Export Diagnostics': '导出诊断数据',
  'Save a zip with app and runtime logs plus version details.':
      '将应用程序与 Runtime Log 及版本信息保存为 ZIP。',
  'Log Level': 'Log 等级',
  'Send Crash Reports': '发送崩溃报告',
  'Send crashes to Sentry, an external service. Off by default; enable only if you want to share crash diagnostics.':
      '将崩溃信息发送到外部服务 Sentry。默认关闭；只有在你愿意分享崩溃诊断数据时才启用。',
  'Could not open the logs folder.': '无法打开 Log 文件夹。',
  'Zip Archive': 'ZIP 压缩包',
  'Diagnostics exported.': '诊断数据已导出。',
  'Support Alera': '支持 Alera',
  'Star Alera on GitHub': '在 GitHub 为 Alera 加星',
  'Star': '加星',
  'Starring…': '正在加星…',
  'Try again': '再试一次',
  'Thanks for starring Alera': '感谢你在 GitHub 为 Alera 加星',
  'Thanks for the support!': '感谢你的支持！',
  'Update status': '更新状态',
  'Checking for updates': '正在检查更新',
  'No update available': '当前没有可用更新',
  'Manual update available': '有可手动安装的更新',
  'Update available': '有可用更新',
  'Downloading update': '正在下载更新',
  'Installing update': '正在安装更新',
  'Restarting Alera': '正在重启 Alera',
  'Restart Alera': '重启 Alera',
  'Update failed': '更新失败',
  'Checking': '检查中',
  'Check for Updates': '检查更新',
  'Download Manually': '手动下载',
  'Installation Guide': '安装指南',
  'Update Alera': '更新 Alera',
  'The update runs here. Answer any prompt in the terminal.':
      '更新会在这里执行；若终端出现提示，请直接响应。',
  'Update checks are disabled in this privacy portable build.':
      '此隐私 Portable 版本已禁用更新检查。',
  'Update handoff complete. Alera will restart shortly.': '更新交接完成，Alera 即将重启。',
  'Restart Alera to load any update installed by the command.':
      '重启 Alera 以加载由命令安装的更新。',
  'Restarting Alera.': '正在重启 Alera。',
  'No update index is published yet.': '尚未发布更新索引。',
  'Alera is up to date.': 'Alera 已是最新版本。',

  // Agent profiles and managed options.
  'How this agent is launched for a dispatched task.':
      '设置此 Agent 接收到派送工作时的启动方式。',
  'Adapter Type': 'Adapter 类型',
  'Command Preview': '命令预览',
  'The host quotes these arguments for the actual platform shell.':
      'Host 会按实际平台 Shell 正确引用这些参数。',
  'Routing': '路由',
  'Signals the orchestrator reads when planning a run.':
      'Orchestrator 规划执行时会读取的信号。',
  'Prompt Delivery': '提示词传递方式',
  'Launch Mode': '启动模式',
  'Managed': '受管理',
  'Command': '命令',
  'Exact Model ID': '指定模型 ID',
  'Use a model ID that is not in the discovered list.': '使用不在已检测列表中的模型 ID。',
  'Persona': 'Persona',
  'Select a known agent persona or enter an exact name.':
      '选择已知的 Agent Persona，或输入确切名称。',
  'Exact Persona': '指定 Persona',
  'Use a persona name that is not in the discovered list.':
      '使用不在已检测列表中的 Persona 名称。',
  'Managed Options': '受管理选项',
  'Alera builds the interactive command from these agent-specific settings.':
      'Alera 会按这些 Agent 专属设置组合互动式命令。',
  'Leave empty to use the agent default.': '留空以使用 Agent 默认值。',
  'Reasoning Effort': '推理强度',
  'Plan Mode Reasoning Effort': 'Plan Mode 推理强度',
  'Applies only while Codex is in plan mode, which is entered with Shift+Tab or /plan. Codex has no way to start there.':
      '仅在 Codex 进入 Plan Mode 时应用；可通过 Shift+Tab 或 /plan 进入。Codex 无法直接以此模式启动。',
  'Sandbox': 'Sandbox',
  'Approval Policy': '批准策略',
  'Web Search': '网络搜索',
  'Allow Codex to search the web.': '允许 Codex 搜索网络。',
  'Bypass All Protections': '跳过所有保护',
  'Bypass both approval prompts and sandbox isolation.':
      '同时跳过批准提示与 Sandbox 隔离。',
  'Allow Skip Permissions': '允许跳过权限',
  'Mode': '模式',
  'Context': 'Context',
  'Allow All': '全部允许',
  'Allow tools and paths without individual prompts.': '允许工具与路径，不需逐项提示。',
  'Maximum AI Credits': 'AI Credits 上限',
  'Maximum Autopilot Continues': 'Autopilot 自动继续上限',
  'Do Not Ask User': '不要询问用户',
  'Continue without asking the user for input.': '不询问用户输入并继续执行。',
  'Permission Mode': '权限模式',
  'Review Mode': '审阅模式',
  'Trust Workspace': '信任工作区',
  'Trust the workspace without an interactive prompt.': '不显示互动提示，直接信任工作区。',
  'Skip Permissions': '跳过权限检查',
  'Run without Antigravity permission checks.': '执行时跳过 Antigravity 权限检查。',
  'Enable the Antigravity sandbox.': '启用 Antigravity Sandbox。',
  'Auto Approve': '自动批准',
  'Approve OpenCode actions automatically.': '自动批准 OpenCode 操作。',
  'Thinking': '思考',
  'Project Trust': '项目信任',
  'Fast Mode': '快速模式',
  'Prefer lower latency responses.': '优先使用较低延迟的响应。',
  'Sandbox Devin exec-tool processes where supported.':
      '在支持的平台上将 Devin exec-tool 程序置于 Sandbox。',
  'Disable Web Search': '禁用网络搜索',
  'Disable Grok Build web search and web fetch tools.':
      '禁用 Grok Build 的网络搜索与 Web Fetch 工具。',
  'Resume Latest Session': '继续最近会话',
  'Ignore Additional Directories': '忽略额外目录',
  'Do not load additional directories configured by fx.': '不要加载 fx 设置的额外目录。',
  'Record Session': '记录会话',
  'Agent profiles unavailable': '无法取得 Agent 配置文件',
  'No agent profiles': '没有 Agent 配置文件',
  'Declare a profile to let a run dispatch work to it.': '创建配置文件，让执行工作可以派送给它。',
  'Agent profile order could not be saved': '无法保存 Agent 配置文件顺序',
  'Confirm Reduced Protections': '确认降低保护',
  'Agent profile saved': 'Agent 配置文件已保存',
  'Test Agent Profile': '测试 Agent 配置文件',
  'The profile command runs here. It does not receive a dispatched task.':
      '配置文件命令会在这里执行，但不会收到派送工作。',
  'Default agent profile updated': '默认 Agent 配置文件已更新',
  'Agent profile cloned': 'Agent 配置文件已复制',

  // Agent integration settings.
  'Alera CLI And Skills': 'Alera CLI 与 Skills',
  'Register the CLI command and install agent instructions.':
      '注册 CLI 命令并安装 Agent 操作指引。',
  'Alera CLI Command': 'Alera CLI 命令',
  'Register the Alera command on PATH for terminals and agents.':
      '将 Alera 命令注册到 PATH，供终端与 Agent 使用。',
  'All Alera Skills': '所有 Alera Skills',
  'Install or update CLI and orchestration skills. Reapplies selected status hooks.':
      '安装或更新 CLI 与 Orchestration Skills，并重新应用已选择的状态 Hooks。',
  'Alera CLI Skill': 'Alera CLI Skill',
  'Install the Codex skill that teaches agents to use the Alera CLI.':
      '安装教导 Agent 使用 Alera CLI 的 Codex Skill。',
  'Alera Orchestration Skill': 'Alera Orchestration Skill',
  'Install or update orchestration and reapply selected status hooks.':
      '安装或更新 Orchestration，并重新应用已选择的状态 Hooks。',
  'Install optional skills for specialized Alera workflows.':
      '安装适用于特定 Alera 工作流程的选用 Skills。',
  'Agent Profiles Skill': 'Agent Profiles Skill',
  'Research models and design, manage, and validate quota-aware Agent Profiles.':
      '研究模型，并设计、管理与验证具配额感知能力的 Agent Profiles。',
  'Agent Executables': 'Agent 可执行文件',
  'Override a supported agent CLI executable on this device. Leave a path blank to use the default command from PATH.':
      '覆盖此设备上支持的 Agent CLI 可执行文件。路径留空则使用 PATH 中的默认命令。',
  'Git Bash Executable': 'Git Bash 可执行文件',
  'Full path to git-bash.exe used when Windows Terminal is unavailable. Leave blank to auto-detect Git for Windows.':
      'Windows Terminal 无法使用时所使用的 git-bash.exe 完整路径。留空会自动检测 Git for Windows。',
  'Managed hooks let terminal tabs show agent state.':
      '受管理的 Hooks 可让终端标签页显示 Agent 状态。',
  'Codex Hooks': 'Codex Hooks',
  'Use an Alera-managed Codex runtime home with status hooks.':
      '使用由 Alera 管理且包含状态 Hooks 的 Codex Runtime Home。',
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
  'fx Status': 'fx 状态',
  'How Alera reacts while agents are running.': '设置 Agent 执行时 Alera 的行为。',
  'Show Tab Titles in Sidebar': '在侧边栏显示标签页标题',
  'Agent Status Notifications': 'Agent 状态通知',
  'Show native notifications when an agent needs attention. Bursts are grouped into one notification.':
      'Agent 需要注意时显示原生通知；短时间大量通知会合并成一则。',
  'Agent Finished Notifications': 'Agent 完成通知',
  'Also notify when an agent finishes. Most agents report the end of a turn, not the end of a task, so this notifies on every reply.':
      'Agent 完成时也发出通知。多数 Agent 回报的是一轮对话结束，而非整个工作结束，因此每次回复都可能通知。',
  'Keep Computer Awake While Agents Are Working': 'Agent 工作时保持电脑唤醒',

  // Quotas.
  'Provider Quotas': 'Provider 配额',
  'Choose which usage sources appear for the active workspace host.':
      '选择作用中工作区 Host 要显示哪些用量来源。',
  'Active Quota Host': '作用中的配额 Host',
  'Run quota commands locally or through the installed Alera runtime for this workspace.':
      '在本地或通过此工作区已安装的 Alera Runtime 执行配额命令。',
  'Quota Display Order': '配额显示顺序',
  'Set the left-to-right order of enabled providers in the status bar.':
      '设置已启用 Provider 在状态栏由左至右的顺序。',
  'Configure the default Claude account and every CCS profile together.':
      '一起设置默认 Claude 账号与所有 CCS 配置文件。',
  'Claude Code Quotas': 'Claude Code 配额',
  'Claude Default Quotas': 'Claude 默认账号配额',
  'Query the default Claude account separately from configured CCS profiles.':
      '将默认 Claude 账号与已设置的 CCS Profiles 分开查询。',
  'Claude Default in Usage': '在 Usage 显示 Claude 默认账号',
  'Include the default Claude account in Usage independently of quota polling.':
      '不受配额轮询设置影响，独立决定是否在 Usage 显示默认 Claude 账号。',
  'Claude CCS Profiles': 'Claude CCS Profiles',
  'Add CCS profiles and choose which ones appear in Usage.':
      '添加 CCS Profiles，并选择哪些要显示在 Usage。',
  'Credential Environment': '凭据环境变量',
  'Configure environment variable names for the active workspace host.':
      '设置作用中工作区 Host 使用的环境变量名称。',
  'Kimi API Key Variable': 'Kimi API Key 变量',
  'Environment variable read on the active host. The secret value is never stored by Alera.':
      '在作用中 Host 读取的环境变量；Alera 永远不会保存其 Secret 值。',
  'Z.ai API Key Variable': 'Z.ai API Key 变量',
  'Z.ai Base URL Variable': 'Z.ai Base URL 变量',
  'Optional environment variable for the coding plan API base URL.':
      '选用的 Coding Plan API Base URL 环境变量。',
  'MiniMax API Key Variable': 'MiniMax API Key 变量',
  'MiniMax API Host Variable': 'MiniMax API Host 变量',
  'Optional environment variable selecting the global or china token plan endpoint.':
      '选用的环境变量，用来选择全球或中国 Token Plan Endpoint。',
  'Credential Availability': '凭据可用性',
  'Check whether each configured variable exists without reading its secret value.':
      '只检查各设置变量是否存在，不读取其 Secret 值。',
  'No quota providers enabled': '尚未启用任何配额 Provider',
  'No CCS profiles configured': '尚未设置 CCS Profile',
  'Shown in status bar': '显示于状态栏',
  'Hidden from status bar - available in the quota panel': '不显示于状态栏，但仍可在配额面板查看',
  'Not shown in Usage': '不显示于 Usage',
  'Show in Usage': '显示于 Usage',

  // AI Assist.
  'Custom Command': '自定义命令',
  'Enter a command before selecting this agent. Use {prompt} to pass the prompt as an argument; otherwise Alera sends it on stdin.':
      '选择此 Agent 前先输入命令。使用 {prompt} 将提示词当作参数传入；否则 Alera 会通过 stdin 发送。',
  'Local agent CLIs run short background jobs from source control and workspace context.':
      '本地 Agent CLI 会利用版本控制与工作区 Context 执行短时间后台任务。',
  'Enable AI Assist': '启用 AI Assist',
  'Generate text for source control, workspaces, and agent conversations.':
      '为版本控制、工作区与 Agent 对话生成文本。',
  'Auto-Generate Agent Titles': '自动生成 Agent 标题',
  'Name new agent conversations from their first prompt or recent context.':
      '按第一个提示词或近期 Context 为新的 Agent 对话命名。',
  'Use {prompt} to pass the prompt as an argument; otherwise Alera sends it on stdin.':
      '使用 {prompt} 将提示词当作参数传入；否则 Alera 会通过 stdin 发送。',
  'Used by prompts that override the global agent with custom command.':
      '供以自定义命令覆盖全局 Agent 的提示词使用。',
  'Configure the agent, model, reasoning and instructions for this prompt.':
      '设置此提示词使用的 Agent、模型、推理与指示。',
  'Instructions': '指示',
  'CLI used for AI Assist jobs.': 'AI Assist 工作所使用的 CLI。',
  'Reasoning effort for models that support it.': '支持此功能的模型所使用的推理强度。',
  'Override the global agent for this prompt.': '针对此提示词覆盖全局 Agent。',
  'Override the global model for this prompt.': '针对此提示词覆盖全局模型。',
  'Optional prompt guidance.': '选用的提示词指引。',
  'Optional instructions': '选用指示',

  // Mobile access.
  'Mobile access unavailable': '移动设备访问无法使用',
  'Connected Remote Devices': '已连接的远程设备',
  'Connected through your Alera account. Disable Remote Access to disconnect these devices.':
      '通过你的 Alera 账号连接。禁用“远程访问”即可中断这些设备。',
  'Endpoint': 'Endpoint',
  'Optional expected name for the new device.': '新设备的预期名称（可选）。',
  'Expires In': '到期时间',
  'Minutes before the offer expires.': '配对邀请到期前的分钟数。',
  'Generate Pairing QR': '生成配对 QR Code',
  'Enables the gateway if it is disabled.': '若 Gateway 尚未启用，会一并启用。',
  'No active offers': '没有作用中的配对邀请',
  'Generate a pairing QR to link a new device.': '生成配对 QR Code 以关联新设备。',
  'Devices that can connect to this runtime.': '可连接到此 Runtime 的设备。',
  'No paired devices': '没有已配对设备',
  'Link a device to see it here.': '关联设备后会显示在这里。',
  'Cancel Pairing Offer': '取消配对邀请',
  'The offer becomes unusable immediately.': '此邀请会立即失效。',
  'Enable Mobile Access': '启用移动设备访问',
  'Accept connections from paired mobile devices.': '接受已配对移动设备的连接。',
  'Enable Remote Access': '启用远程访问',
  'Allow signed-in Alera mobile devices to discover this runtime and use the encrypted relay.':
      '允许已登录的 Alera 移动设备探索此 Runtime 并使用加密 Relay。',
  'Relay Status': 'Relay 状态',
  'Connection Mode': '连接模式',
  'Windows Firewall': 'Windows 防火墙',
  'Bind Host': '绑定 Host',
  'Interface the gateway listens on.': 'Gateway 监听的网络接口。',
  'Network Hint': '网络提示',
  'Gateway listener port.': 'Gateway 监听端口。',
  'Apply Gateway Settings': '应用 Gateway 设置',
  'Persist gateway changes.': '保存 Gateway 变更。',
  'Tailscale Status': 'Tailscale 状态',
  'NetBird Status': 'NetBird 状态',
  'NetBird Endpoint': 'NetBird Endpoint',
  'Address included in new pairing offers.': '新配对邀请中包含的地址。',
  'Offer expired - generate a new one': '配对邀请已过期，请生成新的邀请',
  'Scan with the Alera mobile app': '使用 Alera 移动版 App 扫描',
  'Offer expired': '配对邀请已过期',
  'Copied': '已复制',
  'Copy Pairing JSON': '复制配对 JSON',
  'Revoked': '已撤销',
  'Delete Device': '删除设备',
  'Rename Device': '重命名设备',
  'Revoke Device': '撤销设备',
  'Cancel Offer': '取消邀请',

  // Project/worktree configuration.
  'UI overrides take precedence over repo files.':
      'UI 覆盖设置的优先级高于 Repository 文件。',
  'Config Source': '设置来源',
  'Project instructions appended to prompts that start an agent.':
      '附加到启动 Agent 提示词后方的项目指示。',
  'Git hosting provider used for pull requests and checks.':
      'Pull Request 与检查所使用的 Git 托管 Provider。',
  'Hosting Provider': '托管 Provider',
  'Auto-detect uses public hosts. Select GitHub for GitHub Enterprise Server.':
      '自动检测只使用公开 Host。GitHub Enterprise Server 请选择 GitHub。',
  'Auto-Detect': '自动检测',
  'Copy Rules': '复制规则',
  'Files copied from the main worktree. Gitignored matches from .worktreeinclude are copied too.':
      '从主要 Worktree 复制文件；`.worktreeinclude` 中匹配但被 Gitignore 的文件也会复制。',
  'Setup Commands': '设置命令',
  'Commands run from the new linked workspace.': '在新建的关联工作区中执行的命令。',
  'No projects': '没有项目',
  'Add a project before configuring workspace setup.': '请先添加项目，再设置工作区初始化。',

  // Terminal/editor.
  'Default terminal typography for new sessions.': '新终端会话的默认字体设置。',
  'Default cursor appearance for terminal sessions.': '终端会话的默认光标外观。',
  'Blink the cursor while the terminal has focus.': '终端取得焦点时让光标闪烁。',
  'Terminal colors, theme and spacing.': '终端色彩、主题与间距。',
  'Foreground Color': '前景色',
  'Override the terminal text color.': '覆盖终端文本颜色。',
  'Background Color': '背景色',
  'Override the terminal background color.': '覆盖终端背景颜色。',
  'Cursor Color': '光标颜色',
  'Override the terminal cursor color.': '覆盖终端光标颜色。',
  'Selection Color': '选区颜色',
  'Override the terminal selection color.': '覆盖终端选区颜色。',
  'Mouse, scrolling and clipboard behavior for TUIs.': 'TUI 的鼠标、滚动与剪贴板行为。',
  'Mouse reports sent per wheel step while a TUI owns scrolling.':
      'TUI 接管滚动时，每个滚轮步进发送的鼠标事件数。',
  'Copy local terminal selections to the system clipboard.':
      '将本地终端选择内容复制到系统剪贴板。',
  'Let terminal applications replace the system clipboard.': '允许终端应用程序覆盖系统剪贴板。',
  'History, shell startup and double-click selection behavior.':
      '历史、Shell 启动与双击选择行为。',
  'Use Login Shell': '使用 Login Shell',
  'Start shells as login shells so profile files such as ~/.zprofile and ~/.profile are loaded.':
      '以 Login Shell 启动 Shell，以加载 ~/.zprofile、~/.profile 等配置文件。',
  'Reload Shell Environment': '重新加载 Shell 环境',
  'Re-read the login shell PATH so tools installed since the runtime started resolve in new terminals.':
      '重新读取 Login Shell 的 PATH，让 Runtime 启动后新安装的工具可在新终端中找到。',
  'Reload': '重新加载',
  'Terminal Memory Budget': '终端内存预算',
  'No matching fonts.': '没有匹配的字体。',
  'Font Family': '字体',
  'Font Size': '字体大小',
  'Font Weight': '字重',
  'Line Height': '行高',
  'Background Opacity': '背景透明度',
  'Horizontal Padding': '水平内边距',
  'Vertical Padding': '垂直内边距',
  'Blinking Cursor': '光标闪烁',
  'Cursor Opacity': '光标透明度',
  'Color Overrides': '色彩覆盖',
  'TUI Scroll Speed': 'TUI 滚动速度',
  'Copy On Select': '选择时复制',
  'Allow OSC 52 Clipboard Writes': '允许 OSC 52 写入剪贴板',
  'Show Terminal Composer By Default': '默认显示终端 Composer',
  'Scrollback Lines': 'Scrollback 行数',
  'Host Scrollback Size': 'Host Scrollback 大小',
  'Word Separators': '单词分隔符',
  'Terminal Shortcut Behavior': '终端快捷键行为',

  // Settings search copy.
  'Where new linked workspaces are created on disk.': '新建关联工作区在磁盘上的存放位置。',
  'Move inactive workspaces into the Archived section.': '将空闲工作区移至“已归档”区段。',
  'Ask before unregistering a project.': '取消注册项目前先询问。',
  'Ask before removing a workspace worktree.': '移除工作区 Worktree 前先询问。',
  'Show the folder holding the app log files.': '显示保存应用程序 Log 的文件夹。',
  'Save app and runtime logs with version details as a zip.':
      '将应用程序与 Runtime Log 及版本信息保存为 ZIP。',
  'How much detail is written to the log files.': '设置写入 Log 的详细程度。',
  'Show your support for the project.': '表达你对此项目的支持。',
  'Install or update every core Alera agent skill.':
      '安装或更新所有核心 Alera Agent Skill。',
  'Install agent instructions for the Alera CLI.': '安装 Alera CLI 的 Agent 操作指引。',
  'Install agent instructions for Alera orchestration.':
      '安装 Alera Orchestration 的 Agent 操作指引。',
  'Install specialized instructions for Agent Profile catalogs.':
      '安装 Agent Profile Catalog 专用指引。',
  'Use Alera-managed Codex runtime hooks.':
      '使用由 Alera 管理的 Codex Runtime Hooks。',
  'Show native notifications when agents need attention.': 'Agent 需要注意时显示原生通知。',
  'Also notify when an agent finishes a turn.': 'Agent 完成一轮回复时也通知。',
  'Keep this computer and display awake during agent work.':
      'Agent 工作期间保持电脑与屏幕唤醒。',
  'View and remap app-wide key bindings.': '查看并重新设置全局快捷键。',
  'Syntax highlighting theme used by editor tabs.': '编辑器标签页使用的语法高亮主题。',
  'Spaces inserted when pressing tab in editor tabs.': '在编辑器标签页按下 Tab 时插入的空格数。',
  'Automatically save dirty editor tabs after a pause.': '编辑器变更一段时间没有操作后自动保存。',
  'AI Assist Agent': 'AI Assist Agent',
  'AI Assist Commit Messages': 'AI Assist Commit 消息',
  'AI Assist Pull Request Details': 'AI Assist Pull Request 详细信息',
  'AI Assist Agent Titles': 'AI Assist Agent 标题',
  'AI Assist Workspace Identity': 'AI Assist 工作区标识',
  'AI Assist Reading Diffs': 'AI Assist 阅读 Diff',
  'Project Worktree Setup': '项目 Worktree 设置',
  'Link Mobile Device': '关联移动设备',
};
