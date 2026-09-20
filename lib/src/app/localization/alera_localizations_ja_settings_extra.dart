part of 'alera_localizations.dart';

const Map<String, String> _japaneseSettingsExtra = <String, String>{
  // Settings navigation and application.
  'Workspaces': 'ワークスペース',
  'Workspace Directory': 'ワークスペースのディレクトリ',
  'Where new linked workspaces are created on disk. Existing workspaces are not moved. Leave empty to use the default (~/.alera/workspaces).': '新しいリンク済みワークスペースをディスク上のどこに作成するかを指定します。既存のワークスペースは移動されません。空欄にすると既定の場所（~/.alera/workspaces）を使用します。',
  'Automatic archiving for inactive workspaces.':
      '使われていないワークスペースを自動的にアーカイブします。',
  'Auto-Archive After Inactivity': '未使用期間の経過後に自動アーカイブ',
  'Days without activity before a workspace moves to the Archived section. Set to 0 to keep workspaces listed.':
      'ワークスペースが「アーカイブ済み」セクションに移動されるまでの未使用日数です。0 にすると一覧に残り続けます。',
  'Confirmation prompts for destructive workspace actions.':
      'ワークスペースに対する破壊的な操作の前に確認を表示します。',
  'Confirm Project Removal': 'プロジェクトの削除を確認',
  'Ask before unregistering a project and deleting its workspace metadata.':
      'プロジェクトの登録を解除し、そのワークスペースのメタデータを削除する前に確認します。',
  'Confirm Workspace Removal': 'ワークスペースの削除を確認',
  'Always required because removal closes all tabs, stops running processes, and discards unsaved changes.':
      '削除するとすべてのタブが閉じられ、実行中のプロセスが停止し、保存していない変更が破棄されるため、確認は常に必要です。',
  'Tray icon and dock or taskbar badge while Alera is running.':
      'Alera の実行中に表示するトレイアイコンと、Dock／タスクバーのバッジです。',
  'Show Tray Icon': 'トレイアイコンを表示',
  'Keep Alera in the menu extra (macOS), notification area (Windows), or status bar (Ubuntu). Closing the window hides it; Quit from the tray or the app menu exits.': 'Alera を macOS のメニューバー、Windows の通知領域、Ubuntu のステータスバーに常駐させます。ウィンドウを閉じても非表示になるだけで、トレイまたはアプリメニューから「終了」を選ぶと終了します。',
  'Show Dock Badge': 'Dock／タスクバーのバッジを表示',
  'Show how many agents are waiting for review on the Dock, taskbar, or Ubuntu Dock.':
      'レビュー待ちの Agent の数を Dock、タスクバー、Ubuntu Dock に表示します。',
  'Show Tray Badge': 'トレイバッジを表示',
  'Draw how many agents are waiting for review onto the tray icon itself. Linux only; macOS and Windows show that count on the Dock or taskbar.': 'レビュー待ちの Agent 数をトレイアイコン自体に描画します。Linux のみ対応で、macOS と Windows では Dock またはタスクバーに表示されます。',
  'Compact review and CI status for workspaces backed by a hosted Git repository.':
      'ホスティングされた Git Repository に紐づくワークスペースのレビュー状況と CI 状態をコンパクトに表示します。',
  'Show Pull Request Status': 'Pull Request の状態を表示',
  'Show draft, ready, running, failed, merged, and closed state beside each workspace. Alera batches GitHub workspaces into one refresh per repository.': '各ワークスペースの横に、ドラフト、準備完了、実行中、失敗、マージ済み、クローズ済みの状態を表示します。Alera は GitHub のワークスペースをリポジトリ単位でまとめて更新します。',
  'Notify When Checks Fail': 'チェック失敗時に通知',
  'Show one native notification when a pull request enters a failed-check state. Enabling this keeps the lightweight monitor active while Alera is hidden.': 'Pull Request のチェックが失敗状態になったときに、ネイティブ通知を 1 件表示します。有効にすると、Alera が非表示のときも軽量な監視が動作し続けます。',
  'Lifecycle of the local runtime host that owns terminal sessions.':
      'ターミナルセッションを管理するローカル Runtime Host のライフサイクルです。',
  'Keep Computer Awake': 'コンピューターをスリープさせない',
  'Prevents idle sleep and display sleep while Alera is running. Closing the lid still follows this device\'s power settings.':
      'Alera の実行中は、アイドルスリープとディスプレイのスリープを抑止します。ふたを閉じたときの動作は、このデバイスの電源設定に従います。',
  'Keep Runtime Open When App Quits': 'アプリ終了後も Runtime を起動したままにする',
  'Leave the app-launched sidecar running after a clean quit. Persistent CLI runtimes are never stopped by quitting, and unexpected exits always leave the host up.': '正常に終了した後も、アプリが起動したサイドカーを動かし続けます。常駐型の CLI Runtime は終了操作で停止されることはなく、予期しない終了でも Host は残ります。',
  'Empty Host Shutdown': 'セッションのない Host の停止',
  'Seconds to keep the host alive after the app closes with no running sessions.':
      '実行中のセッションがない状態でアプリを閉じた後、Host を維持する秒数です。',
  'Detached Session Shutdown': 'デタッチされたセッションの停止',
  'Seconds to keep detached running sessions alive after the app closes.':
      'アプリを閉じた後、デタッチされた実行中のセッションを維持する秒数です。',

  // Diagnostics, support, and updates.
  'Alera keeps rotating log files on this computer so an error can be investigated after it happens.':
      'Alera はこのコンピューター上でログファイルをローテーションしながら保存し、問題の発生後に調査できるようにします。',
  'Open Logs Folder': 'ログフォルダーを開く',
  'Export Diagnostics': '診断情報をエクスポート',
  'Save a zip with app and runtime logs plus version details.':
      'アプリと Runtime のログ、バージョン情報を ZIP に保存します。',
  'Log Level': 'ログレベル',
  'Send Crash Reports': 'クラッシュレポートを送信',
  'Send crashes to Sentry, an external service. Off by default; enable only if you want to share crash diagnostics.':
      'クラッシュ情報を外部サービスの Sentry に送信します。既定ではオフで、診断情報を共有してもよい場合にのみ有効にしてください。',
  'Could not open the logs folder.': 'ログフォルダーを開けませんでした。',
  'Zip Archive': 'ZIP アーカイブ',
  'Diagnostics exported.': '診断情報をエクスポートしました。',
  'Support Alera': 'Alera を応援する',
  'Star Alera on GitHub': 'GitHub で Alera にスターを付ける',
  'Star': 'スター',
  'Starring…': 'スターを付けています…',
  'Try again': 'もう一度試す',
  'Thanks for starring Alera': 'Alera にスターを付けていただきありがとうございます',
  'Thanks for the support!': 'ご支援ありがとうございます！',
  'Update status': '更新の状態',
  'Checking for updates': '更新を確認しています',
  'No update available': '利用できる更新はありません',
  'Manual update available': '手動でインストールできる更新があります',
  'Update available': '更新があります',
  'Downloading update': '更新をダウンロードしています',
  'Installing update': '更新をインストールしています',
  'Restarting Alera': 'Alera を再起動しています',
  'Restart Alera': 'Alera を再起動',
  'Update failed': '更新に失敗しました',
  'Checking': '確認中',
  'Check for Updates': '更新を確認',
  'Download Manually': '手動でダウンロード',
  'Installation Guide': 'インストール手順',
  'Update Alera': 'Alera を更新',
  'The update runs here. Answer any prompt in the terminal.':
      '更新はここで実行されます。ターミナルに確認が表示されたら、そのまま応答してください。',
  'Update checks are disabled in this privacy portable build.':
      'このプライバシー版ポータブルビルドでは更新の確認が無効になっています。',
  'Update handoff complete. Alera will restart shortly.':
      '更新の引き継ぎが完了しました。まもなく Alera が再起動します。',
  'Restart Alera to load any update installed by the command.':
      'コマンドでインストールされた更新を読み込むには、Alera を再起動してください。',
  'Restarting Alera.': 'Alera を再起動しています。',
  'No update index is published yet.': '更新インデックスはまだ公開されていません。',
  'Alera is up to date.': 'Alera は最新の状態です。',

  // Agent profiles and managed options.
  'How this agent is launched for a dispatched task.':
      'ディスパッチされたタスクに対して、この Agent をどう起動するかを設定します。',
  'Adapter Type': 'Adapter の種類',
  'Command Preview': 'コマンドのプレビュー',
  'The host quotes these arguments for the actual platform shell.':
      'Host は実際のプラットフォームのシェルに合わせて、これらの引数を適切にクォートします。',
  'Routing': 'ルーティング',
  'Signals the orchestrator reads when planning a run.':
      'オーケストレーターが実行を計画するときに参照するシグナルです。',
  'Prompt Delivery': 'プロンプトの受け渡し方法',
  'Launch Mode': '起動モード',
  'Managed': '管理対象',
  'Command': 'コマンド',
  'Exact Model ID': 'モデル ID を直接指定',
  'Use a model ID that is not in the discovered list.':
      '検出された一覧にないモデル ID を使用します。',
  'Persona': 'Persona',
  'Select a known agent persona or enter an exact name.':
      '既知の Agent Persona を選ぶか、正確な名前を入力します。',
  'Exact Persona': 'Persona を直接指定',
  'Use a persona name that is not in the discovered list.':
      '検出された一覧にない Persona 名を使用します。',
  'Managed Options': '管理対象のオプション',
  'Alera builds the interactive command from these agent-specific settings.':
      'Alera はこれらの Agent 固有の設定から対話型コマンドを組み立てます。',
  'Leave empty to use the agent default.': '空欄にすると Agent の既定値を使用します。',
  'Reasoning Effort': '推論の深さ',
  'Plan Mode Reasoning Effort': 'Plan Mode の推論の深さ',
  'Applies only while Codex is in plan mode, which is entered with Shift+Tab or /plan. Codex has no way to start there.': 'Codex が Plan Mode のときにのみ適用されます。Plan Mode には Shift+Tab または /plan で入ります。Codex を最初から Plan Mode で起動することはできません。',
  'Sandbox': 'Sandbox',
  'Approval Policy': '承認ポリシー',
  'Web Search': 'Web 検索',
  'Allow Codex to search the web.': 'Codex に Web 検索を許可します。',
  'Bypass All Protections': 'すべての保護をバイパス',
  'Bypass both approval prompts and sandbox isolation.':
      '承認の確認と Sandbox の分離の両方をバイパスします。',
  'Allow Skip Permissions': '権限確認のスキップを許可',
  'Mode': 'モード',
  'Context': 'Context',
  'Allow All': 'すべて許可',
  'Allow tools and paths without individual prompts.': '個別の確認なしにツールとパスを許可します。',
  'Maximum AI Credits': 'AI Credits の上限',
  'Maximum Autopilot Continues': 'Autopilot の自動継続回数の上限',
  'Do Not Ask User': 'ユーザーに確認しない',
  'Continue without asking the user for input.': 'ユーザーに入力を求めずに続行します。',
  'Permission Mode': '権限モード',
  'Review Mode': 'レビューモード',
  'Trust Workspace': 'ワークスペースを信頼',
  'Trust the workspace without an interactive prompt.':
      '確認を表示せずにワークスペースを信頼します。',
  'Skip Permissions': '権限確認をスキップ',
  'Run without Antigravity permission checks.':
      'Antigravity の権限チェックを行わずに実行します。',
  'Enable the Antigravity sandbox.': 'Antigravity の Sandbox を有効にします。',
  'Auto Approve': '自動承認',
  'Approve OpenCode actions automatically.': 'OpenCode の操作を自動的に承認します。',
  'Thinking': '思考',
  'Project Trust': 'プロジェクトの信頼',
  'Fast Mode': '高速モード',
  'Prefer lower latency responses.': '低レイテンシーの応答を優先します。',
  'Sandbox Devin exec-tool processes where supported.':
      '対応している環境では、Devin の exec-tool プロセスを Sandbox 内で実行します。',
  'Disable Web Search': 'Web 検索を無効化',
  'Disable Grok Build web search and web fetch tools.':
      'Grok Build の Web 検索と Web Fetch ツールを無効にします。',
  'Resume Latest Session': '最新のセッションを再開',
  'Ignore Additional Directories': '追加ディレクトリを無視',
  'Do not load additional directories configured by fx.':
      'fx で設定された追加ディレクトリを読み込みません。',
  'Record Session': 'セッションを記録',
  'Agent profiles unavailable': 'Agent プロファイルを取得できません',
  'No agent profiles': 'Agent プロファイルがありません',
  'Declare a profile to let a run dispatch work to it.':
      'プロファイルを定義すると、実行時にそこへ処理をディスパッチできます。',
  'Agent profile order could not be saved': 'Agent プロファイルの並び順を保存できませんでした',
  'Confirm Reduced Protections': '保護の緩和を確認',
  'Agent profile saved': 'Agent プロファイルを保存しました',
  'Test Agent Profile': 'Agent プロファイルをテスト',
  'The profile command runs here. It does not receive a dispatched task.':
      'プロファイルのコマンドはここで実行されます。ディスパッチされたタスクは渡されません。',
  'Default agent profile updated': '既定の Agent プロファイルを更新しました',
  'Agent profile cloned': 'Agent プロファイルを複製しました',

  // Agent integration settings.
  'Alera CLI And Skills': 'Alera CLI と Skills',
  'Register the CLI command and install agent instructions.':
      'CLI コマンドを登録し、Agent 向けの手引きをインストールします。',
  'Alera CLI Command': 'Alera CLI コマンド',
  'Register the Alera command on PATH for terminals and agents.':
      'ターミナルや Agent から使えるよう、Alera コマンドを PATH に登録します。',
  'All Alera Skills': 'すべての Alera Skills',
  'Install or update CLI and orchestration skills. Reapplies selected status hooks.':
      'CLI と Orchestration の Skills をインストールまたは更新し、選択中のステータス Hooks を再適用します。',
  'Alera CLI Skill': 'Alera CLI Skill',
  'Install the Codex skill that teaches agents to use the Alera CLI.':
      'Agent に Alera CLI の使い方を教える Codex Skill をインストールします。',
  'Alera Orchestration Skill': 'Alera Orchestration Skill',
  'Install or update orchestration and reapply selected status hooks.':
      'Orchestration をインストールまたは更新し、選択中のステータス Hooks を再適用します。',
  'Install optional skills for specialized Alera workflows.':
      '特定の Alera ワークフロー向けの任意の Skills をインストールします。',
  'Agent Profiles Skill': 'Agent Profiles Skill',
  'Research models and design, manage, and validate quota-aware Agent Profiles.':
      'モデルを調査し、使用枠を考慮した Agent Profiles の設計・管理・検証を行います。',
  'Agent Executables': 'Agent の実行ファイル',
  'Override a supported agent CLI executable on this device. Leave a path blank to use the default command from PATH.':
      'このデバイスで、対応している Agent CLI の実行ファイルを上書きします。パスを空欄にすると PATH 上の既定のコマンドを使用します。',
  'Git Bash Executable': 'Git Bash の実行ファイル',
  'Full path to git-bash.exe used when Windows Terminal is unavailable. Leave blank to auto-detect Git for Windows.': 'Windows Terminal を利用できないときに使用する git-bash.exe のフルパスです。空欄にすると Git for Windows を自動検出します。',
  'Managed hooks let terminal tabs show agent state.':
      '管理対象の Hooks により、ターミナルタブに Agent の状態を表示できます。',
  'Codex Hooks': 'Codex Hooks',
  'Use an Alera-managed Codex runtime home with status hooks.':
      'ステータス Hooks を備えた、Alera 管理の Codex Runtime Home を使用します。',
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
  'fx Status': 'fx のステータス',
  'How Alera reacts while agents are running.':
      'Agent の実行中に Alera がどう振る舞うかを設定します。',
  'Show Tab Titles in Sidebar': 'サイドバーにタブのタイトルを表示',
  'Agent Status Notifications': 'Agent のステータス通知',
  'Show native notifications when an agent needs attention. Bursts are grouped into one notification.':
      'Agent が対応を必要とするときにネイティブ通知を表示します。短時間に集中した通知は 1 件にまとめられます。',
  'Agent Finished Notifications': 'Agent 完了時の通知',
  'Also notify when an agent finishes. Most agents report the end of a turn, not the end of a task, so this notifies on every reply.': 'Agent が完了したときにも通知します。多くの Agent はタスクの完了ではなく 1 ターンの終了を報告するため、返信のたびに通知される点に注意してください。',
  'Keep Computer Awake While Agents Are Working': 'Agent の作業中はコンピューターをスリープさせない',

  // Quotas.
  'Provider Quotas': 'プロバイダーの使用枠',
  'Choose which usage sources appear for the active workspace host.':
      'アクティブなワークスペースの Host で、どの使用量ソースを表示するかを選択します。',
  'Active Quota Host': '使用枠を取得する Host',
  'Run quota commands locally or through the installed Alera runtime for this workspace.':
      '使用枠のコマンドをローカルで実行するか、このワークスペース用にインストールされた Alera Runtime 経由で実行します。',
  'Quota Display Order': '使用枠の表示順',
  'Set the left-to-right order of enabled providers in the status bar.':
      'ステータスバーで有効なプロバイダーを左から右へ並べる順序を設定します。',
  'Configure the default Claude account and every CCS profile together.':
      '既定の Claude アカウントとすべての CCS プロファイルをまとめて設定します。',
  'Claude Code Quotas': 'Claude Code の使用枠',
  'Claude Default Quotas': 'Claude 既定アカウントの使用枠',
  'Query the default Claude account separately from configured CCS profiles.':
      '既定の Claude アカウントを、設定済みの CCS Profiles とは別に照会します。',
  'Claude Default in Usage': 'Usage に Claude 既定アカウントを表示',
  'Include the default Claude account in Usage independently of quota polling.':
      '使用枠のポーリング設定とは独立して、既定の Claude アカウントを Usage に含めます。',
  'Claude CCS Profiles': 'Claude CCS Profiles',
  'Add CCS profiles and choose which ones appear in Usage.':
      'CCS Profiles を追加し、Usage に表示するものを選択します。',
  'Credential Environment': '認証情報の環境変数',
  'Configure environment variable names for the active workspace host.':
      'アクティブなワークスペースの Host で使用する環境変数名を設定します。',
  'Kimi API Key Variable': 'Kimi API Key の変数',
  'Environment variable read on the active host. The secret value is never stored by Alera.':
      'アクティブな Host 上で読み取る環境変数です。Alera がその秘密の値を保存することはありません。',
  'Z.ai API Key Variable': 'Z.ai API Key の変数',
  'Z.ai Base URL Variable': 'Z.ai Base URL の変数',
  'Optional environment variable for the coding plan API base URL.':
      'コーディングプランの API Base URL を指定する環境変数（任意）。',
  'MiniMax API Key Variable': 'MiniMax API Key の変数',
  'MiniMax API Host Variable': 'MiniMax API Host の変数',
  'Optional environment variable selecting the global or china token plan endpoint.':
      'グローバル版と中国版のトークンプランのエンドポイントを選択する環境変数（任意）。',
  'Credential Availability': '認証情報の有無',
  'Check whether each configured variable exists without reading its secret value.':
      '設定した各変数が存在するかどうかだけを確認し、秘密の値は読み取りません。',
  'No quota providers enabled': '使用枠のプロバイダーが有効になっていません',
  'No CCS profiles configured': 'CCS プロファイルが設定されていません',
  'Shown in status bar': 'ステータスバーに表示',
  'Hidden from status bar - available in the quota panel':
      'ステータスバーには非表示。使用枠パネルでは確認できます',
  'Not shown in Usage': 'Usage に表示しない',
  'Show in Usage': 'Usage に表示',

  // AI Assist.
  'Custom Command': 'カスタムコマンド',
  'Enter a command before selecting this agent. Use {prompt} to pass the prompt as an argument; otherwise Alera sends it on stdin.': 'この Agent を選ぶ前にコマンドを入力してください。{prompt} を使うとプロンプトを引数として渡せます。指定しない場合、Alera は標準入力で送信します。',
  'Local agent CLIs run short background jobs from source control and workspace context.':
      'ローカルの Agent CLI が、バージョン管理とワークスペースの Context を使って短いバックグラウンドジョブを実行します。',
  'Enable AI Assist': 'AI Assist を有効化',
  'Generate text for source control, workspaces, and agent conversations.':
      'バージョン管理、ワークスペース、Agent の会話のためのテキストを生成します。',
  'Auto-Generate Agent Titles': 'Agent のタイトルを自動生成',
  'Name new agent conversations from their first prompt or recent context.':
      '最初のプロンプトや直近の Context をもとに、新しい Agent の会話に名前を付けます。',
  'Use {prompt} to pass the prompt as an argument; otherwise Alera sends it on stdin.':
      '{prompt} を使うとプロンプトを引数として渡せます。指定しない場合、Alera は標準入力で送信します。',
  'Used by prompts that override the global agent with custom command.':
      'グローバル Agent をカスタムコマンドで上書きするプロンプトで使用されます。',
  'Configure the agent, model, reasoning and instructions for this prompt.':
      'このプロンプトで使う Agent、モデル、推論、指示を設定します。',
  'Instructions': '指示',
  'CLI used for AI Assist jobs.': 'AI Assist のジョブで使用する CLI です。',
  'Reasoning effort for models that support it.': '対応しているモデルで使用する推論の深さです。',
  'Override the global agent for this prompt.':
      'このプロンプトに限りグローバル Agent を上書きします。',
  'Override the global model for this prompt.': 'このプロンプトに限りグローバルモデルを上書きします。',
  'Optional prompt guidance.': 'プロンプトへの補足指示（任意）。',
  'Optional instructions': '指示（任意）',

  // Mobile access.
  'Mobile access unavailable': 'モバイルからのアクセスを利用できません',
  'Connected Remote Devices': '接続中のリモートデバイス',
  'Connected through your Alera account. Disable Remote Access to disconnect these devices.':
      'Alera アカウント経由で接続しています。「リモートアクセス」を無効にすると、これらのデバイスの接続が切断されます。',
  'Endpoint': 'エンドポイント',
  'Optional expected name for the new device.': '新しいデバイスに想定される名前（任意）。',
  'Expires In': '有効期限',
  'Minutes before the offer expires.': 'ペアリング招待が失効するまでの分数です。',
  'Generate Pairing QR': 'ペアリング QR コードを生成',
  'Enables the gateway if it is disabled.': 'ゲートウェイが無効の場合は、あわせて有効にします。',
  'No active offers': '有効なペアリング招待はありません',
  'Generate a pairing QR to link a new device.':
      'ペアリング QR コードを生成して、新しいデバイスを関連付けます。',
  'Devices that can connect to this runtime.': 'この Runtime に接続できるデバイスです。',
  'No paired devices': 'ペアリング済みのデバイスはありません',
  'Link a device to see it here.': 'デバイスを関連付けると、ここに表示されます。',
  'Cancel Pairing Offer': 'ペアリング招待をキャンセル',
  'The offer becomes unusable immediately.': 'この招待はただちに使用できなくなります。',
  'Enable Mobile Access': 'モバイルからのアクセスを有効化',
  'Accept connections from paired mobile devices.':
      'ペアリング済みのモバイルデバイスからの接続を受け付けます。',
  'Enable Remote Access': 'リモートアクセスを有効化',
  'Allow signed-in Alera mobile devices to discover this runtime and use the encrypted relay.':
      'サインイン済みの Alera モバイルデバイスがこの Runtime を検出し、暗号化された Relay を利用できるようにします。',
  'Relay Status': 'Relay の状態',
  'Connection Mode': '接続モード',
  'Windows Firewall': 'Windows ファイアウォール',
  'Bind Host': 'バインドする Host',
  'Interface the gateway listens on.': 'ゲートウェイが待ち受けるネットワークインターフェイスです。',
  'Network Hint': 'ネットワークのヒント',
  'Gateway listener port.': 'ゲートウェイが待ち受けるポートです。',
  'Apply Gateway Settings': 'ゲートウェイ設定を適用',
  'Persist gateway changes.': 'ゲートウェイの変更を保存します。',
  'Tailscale Status': 'Tailscale の状態',
  'NetBird Status': 'NetBird の状態',
  'NetBird Endpoint': 'NetBird のエンドポイント',
  'Address included in new pairing offers.': '新しいペアリング招待に含めるアドレスです。',
  'Offer expired - generate a new one': 'ペアリング招待の有効期限が切れました。新しく生成してください',
  'Scan with the Alera mobile app': 'Alera モバイルアプリで読み取ってください',
  'Offer expired': 'ペアリング招待の有効期限が切れました',
  'Copied': 'コピーしました',
  'Copy Pairing JSON': 'ペアリング JSON をコピー',
  'Revoked': '無効化済み',
  'Delete Device': 'デバイスを削除',
  'Rename Device': 'デバイス名を変更',
  'Revoke Device': 'デバイスを無効化',
  'Cancel Offer': '招待をキャンセル',

  // Project/worktree configuration.
  'UI overrides take precedence over repo files.':
      'UI での上書き設定は、リポジトリ内のファイルより優先されます。',
  'Config Source': '設定の取得元',
  'Project instructions appended to prompts that start an agent.':
      'Agent を起動するプロンプトの末尾に追加される、プロジェクト固有の指示です。',
  'Git hosting provider used for pull requests and checks.':
      'Pull Request とチェックで使用する Git ホスティングプロバイダーです。',
  'Hosting Provider': 'ホスティングプロバイダー',
  'Auto-detect uses public hosts. Select GitHub for GitHub Enterprise Server.':
      '自動検出は公開ホストのみを対象とします。GitHub Enterprise Server の場合は GitHub を選択してください。',
  'Auto-Detect': '自動検出',
  'Copy Rules': 'コピールール',
  'Files copied from the main worktree. Gitignored matches from .worktreeinclude are copied too.': 'メインの Worktree からコピーされるファイルです。`.worktreeinclude` に一致すれば、Gitignore 対象のファイルもコピーされます。',
  'Setup Commands': 'セットアップコマンド',
  'Commands run from the new linked workspace.':
      '新しく作成されたリンク済みワークスペースで実行されるコマンドです。',
  'No projects': 'プロジェクトがありません',
  'Add a project before configuring workspace setup.':
      'ワークスペースのセットアップを設定する前に、プロジェクトを追加してください。',

  // Terminal/editor.
  'Default terminal typography for new sessions.':
      '新しいセッションで使うターミナルの既定のフォント設定です。',
  'Default cursor appearance for terminal sessions.': 'ターミナルセッションの既定のカーソル外観です。',
  'Blink the cursor while the terminal has focus.':
      'ターミナルにフォーカスがあるときにカーソルを点滅させます。',
  'Terminal colors, theme and spacing.': 'ターミナルの配色、テーマ、余白です。',
  'Foreground Color': '前景色',
  'Override the terminal text color.': 'ターミナルの文字色を上書きします。',
  'Background Color': '背景色',
  'Override the terminal background color.': 'ターミナルの背景色を上書きします。',
  'Cursor Color': 'カーソルの色',
  'Override the terminal cursor color.': 'ターミナルのカーソル色を上書きします。',
  'Selection Color': '選択範囲の色',
  'Override the terminal selection color.': 'ターミナルの選択範囲の色を上書きします。',
  'Mouse, scrolling and clipboard behavior for TUIs.':
      'TUI でのマウス、スクロール、クリップボードの動作です。',
  'Mouse reports sent per wheel step while a TUI owns scrolling.':
      'TUI がスクロールを制御しているときに、ホイール 1 目盛りごとに送信するマウスイベントの数です。',
  'Copy local terminal selections to the system clipboard.':
      'ローカルのターミナルで選択した内容をシステムのクリップボードにコピーします。',
  'Let terminal applications replace the system clipboard.':
      'ターミナル上のアプリケーションがシステムのクリップボードを書き換えることを許可します。',
  'History, shell startup and double-click selection behavior.':
      '履歴、シェルの起動、ダブルクリック選択の動作です。',
  'Use Login Shell': 'ログインシェルを使用',
  'Start shells as login shells so profile files such as ~/.zprofile and ~/.profile are loaded.':
      'シェルをログインシェルとして起動し、~/.zprofile や ~/.profile などのプロファイルが読み込まれるようにします。',
  'Reload Shell Environment': 'シェル環境を再読み込み',
  'Re-read the login shell PATH so tools installed since the runtime started resolve in new terminals.':
      'ログインシェルの PATH を読み直し、Runtime の起動後にインストールしたツールを新しいターミナルで使えるようにします。',
  'Reload': '再読み込み',
  'Terminal Memory Budget': 'ターミナルのメモリ上限',
  'No matching fonts.': '一致するフォントがありません。',
  'Font Family': 'フォント',
  'Font Size': 'フォントサイズ',
  'Font Weight': 'フォントの太さ',
  'Line Height': '行の高さ',
  'Background Opacity': '背景の不透明度',
  'Horizontal Padding': '左右の余白',
  'Vertical Padding': '上下の余白',
  'Blinking Cursor': 'カーソルの点滅',
  'Cursor Opacity': 'カーソルの不透明度',
  'Color Overrides': '色の上書き',
  'TUI Scroll Speed': 'TUI のスクロール速度',
  'Copy On Select': '選択時にコピー',
  'Allow OSC 52 Clipboard Writes': 'OSC 52 によるクリップボード書き込みを許可',
  'Show Terminal Composer By Default': 'ターミナルの入力欄を既定で表示',
  'Scrollback Lines': 'スクロールバックの行数',
  'Host Scrollback Size': 'Host 側スクロールバックのサイズ',
  'Word Separators': '単語の区切り文字',
  'Terminal Shortcut Behavior': 'ターミナルでのショートカットの動作',

  // Settings search copy.
  'Where new linked workspaces are created on disk.':
      '新しいリンク済みワークスペースをディスク上のどこに作成するかです。',
  'Move inactive workspaces into the Archived section.':
      '使われていないワークスペースを「アーカイブ済み」セクションに移動します。',
  'Ask before unregistering a project.': 'プロジェクトの登録を解除する前に確認します。',
  'Ask before removing a workspace worktree.':
      'ワークスペースの Worktree を削除する前に確認します。',
  'Show the folder holding the app log files.':
      'アプリのログファイルが保存されているフォルダーを表示します。',
  'Save app and runtime logs with version details as a zip.':
      'アプリと Runtime のログをバージョン情報とともに ZIP に保存します。',
  'How much detail is written to the log files.': 'ログファイルに書き出す詳細度です。',
  'Show your support for the project.': 'このプロジェクトへの支持を示します。',
  'Install or update every core Alera agent skill.':
      'Alera のコア Agent Skill をすべてインストールまたは更新します。',
  'Install agent instructions for the Alera CLI.':
      'Alera CLI 用の Agent 向け手引きをインストールします。',
  'Install agent instructions for Alera orchestration.':
      'Alera Orchestration 用の Agent 向け手引きをインストールします。',
  'Install specialized instructions for Agent Profile catalogs.':
      'Agent Profile カタログ向けの専用手引きをインストールします。',
  'Use Alera-managed Codex runtime hooks.':
      'Alera 管理の Codex Runtime Hooks を使用します。',
  'Show native notifications when agents need attention.':
      'Agent が対応を必要とするときにネイティブ通知を表示します。',
  'Also notify when an agent finishes a turn.': 'Agent が 1 ターンを終えたときにも通知します。',
  'Keep this computer and display awake during agent work.':
      'Agent の作業中はコンピューターとディスプレイをスリープさせません。',
  'View and remap app-wide key bindings.': 'アプリ全体のキーバインドを確認して割り当て直します。',
  'Syntax highlighting theme used by editor tabs.':
      'エディタータブで使用するシンタックスハイライトのテーマです。',
  'Spaces inserted when pressing tab in editor tabs.':
      'エディタータブで Tab キーを押したときに挿入されるスペースの数です。',
  'Automatically save dirty editor tabs after a pause.':
      '編集中のエディタータブを、操作が止まってから自動保存します。',
  'AI Assist Agent': 'AI Assist Agent',
  'AI Assist Commit Messages': 'AI Assist の Commit メッセージ',
  'AI Assist Pull Request Details': 'AI Assist の Pull Request 詳細',
  'AI Assist Agent Titles': 'AI Assist の Agent タイトル',
  'AI Assist Workspace Identity': 'AI Assist のワークスペース識別子',
  'AI Assist Reading Diffs': 'AI Assist のリーディング Diff',
  'Project Worktree Setup': 'プロジェクトの Worktree セットアップ',
  'Link Mobile Device': 'モバイルデバイスを関連付け',
};
