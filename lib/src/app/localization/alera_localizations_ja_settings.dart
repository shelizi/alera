part of 'alera_localizations.dart';

const Map<String, String> _japaneseSettings = <String, String>{
  // Settings sections.
  'Configuration Sync': '設定の同期',
  'Account': 'アカウント',
  'Application': 'アプリケーション',
  'Agents': 'Agent',
  'Quotas': '使用枠',
  'AI Assist': 'AI Assist',
  'AI Dictation': 'AI 音声入力',
  'Text Actions': 'テキスト操作',
  'Editor': 'エディター',
  'Terminal': 'ターミナル',
  'Keyboard': 'キーボード',
  'Projects': 'プロジェクト',
  'Mobile Devices': 'モバイルデバイス',
  'Remote Hosts': 'リモート Host',
  'Agent Profiles': 'Agent プロファイル',

  // Settings groups and common rows.
  'Language': '言語',
  'App Language': 'アプリの言語',
  'Storage': 'ストレージ',
  'Safety': '安全確認',
  'Desktop': 'デスクトップ',
  'Runtime': 'Runtime',
  'Diagnostics': '診断',
  'Updates': '更新',
  'Support': 'サポート',
  'Identity': 'アイデンティティ',
  'Account unavailable': 'アカウントを取得できません',
  'Your Alera identity protects cloud delivery and stays optional for local features.':
      'Alera のアイデンティティはクラウド配信を保護します。ローカル機能の利用には必須ではありません。',
  'Continue With Google': 'Google で続行',
  'Sign in through your default browser.': '既定のブラウザーでサインインします。',
  'Continue With GitHub': 'GitHub で続行',
  'Uses profile and verified email access only. Repository access is never requested.':
      'プロフィールと確認済みメールアドレスのみを参照します。Repository へのアクセス権は要求しません。',
  'Alera Account': 'Alera アカウント',
  'Add another verified sign-in method to this account.':
      'このアカウントに、確認済みのサインイン方法をもう 1 つ追加します。',
  'Sign Out': 'サインアウト',
  'Stops cloud push delivery from this runtime until you sign in again.':
      '再びサインインするまで、この Runtime からのクラウドプッシュ配信を停止します。',
  'Browser Sign In': 'ブラウザーでのサインイン',
  'A provider authorization is waiting in your browser.':
      'ブラウザーでプロバイダーの認可を待っています。',
  'Notifications are delivered only to mobile devices enrolled in this account.':
      '通知は、このアカウントに登録されたモバイルデバイスにのみ配信されます。',
  'Enable Mobile Push': 'モバイルプッシュを有効化',
  'Sign in before enabling cloud delivery.': 'クラウド配信を有効にする前にサインインしてください。',
  'Attention Required': '対応が必要',
  'Notify for waiting or blocked agents, decision gates, and escalations.':
      'Agent の入力待ち、ブロック、判断待ち、エスカレーション時に通知します。',
  'Agent Finished': 'Agent の完了',
  'Notify when an agent finishes a turn.': 'Agent が 1 ターンを終えたときに通知します。',
  'Terminal Ended': 'ターミナルの終了',
  'Notify when a terminal session exits or is closed.':
      'ターミナルセッションが終了または閉じられたときに通知します。',
  'Move this runtime to another account or remove your cloud identity.':
      'この Runtime を別のアカウントへ移すか、クラウドアイデンティティを削除します。',
  'Target Account ID': '移行先のアカウント ID',
  'Moving a runtime signs this installation out and requires authentication again.':
      'Runtime を移動すると、このインストールはサインアウトされ、再認証が必要になります。',
  'Account ID': 'アカウント ID',
  'Move This Runtime': 'この Runtime を移動',
  'Transfer runtime ownership and its mobile subscriptions.':
      'Runtime の所有権とモバイルのサブスクリプションを移管します。',
  'Move Runtime': 'Runtime を移動',
  'Delete Alera Account': 'Alera アカウントを削除',
  'Permanently removes provider identities, cloud sessions, subscriptions, and quota records.':
      'プロバイダーのアイデンティティ、クラウドセッション、サブスクリプション、使用枠の記録を完全に削除します。',
  'Delete Account': 'アカウントを削除',
  'This permanently removes your Alera cloud identity, active sessions, mobile subscriptions, and quota records. Recent sign-in may be required.': 'Alera のクラウドアイデンティティ、アクティブなセッション、モバイルのサブスクリプション、使用枠の記録を完全に削除します。最近のサインインが必要になる場合があります。',

  'Mobile Push': 'モバイルプッシュ',
  'Ownership': '所有権',
  'CLI And Skills': 'CLI と Skills',
  'Extra Skills': '追加の Skills',
  'Status Hooks': 'ステータス Hook',
  'Behavior': '動作',
  'Providers': 'プロバイダー',
  'Credentials': '認証情報',
  'Actions': '操作',
  'Generation': '生成',
  'Commit Messages': 'Commit メッセージ',
  'Pull Request Details': 'Pull Request の詳細',
  'Reading Diffs': 'リーディング Diff',
  'Workspace Identity': 'ワークスペースの識別子',
  'Transcription': '文字起こし',
  'Remote Transcription': 'リモート文字起こし',
  'Local Whisper Models': 'ローカルの Whisper モデル',
  'Speech Processing': '音声処理',
  'Test AI Dictation': 'AI 音声入力をテスト',
  'Choose where speech is converted to text on this device.':
      'このデバイスで音声をテキストに変換する場所を選択します。',
  'Enable AI Dictation': 'AI 音声入力を有効化',
  'Show microphone controls in supported composers.':
      '対応する入力欄にマイクのコントロールを表示します。',
  'Transcription Engine': '文字起こしエンジン',
  'Optional locale or language code. Leave blank for automatic detection.':
      'ロケールまたは言語コード（任意）。空欄にすると自動判定します。',
  'Allow Online Speech Recognition': 'オンライン音声認識を許可',
  'Windows may send microphone audio to Microsoft to create the transcription.':
      'Windows は文字起こしのために、マイクの音声を Microsoft に送信する場合があります。',
  'The system recognizer may send microphone audio to its online speech service.':
      'システムの認識エンジンは、マイクの音声をオンラインの音声サービスに送信する場合があります。',
  'Install multiple multilingual models and select one for local transcription.':
      '多言語モデルを複数インストールし、ローカル文字起こしに使うモデルを選択できます。',
  'Optionally improve the transcript with the agent subscription configured for Speech Messages in AI Assist settings.':
      'AI Assist 設定の「音声メッセージ」で指定した Agent サブスクリプションを使って、文字起こし結果を改善できます（任意）。',
  'Automatic Processing': '自動処理',
  'Raw text is always used if the selected agent is unavailable or fails.':
      '選択した Agent が利用できない、または失敗した場合は、常に元のテキストが使用されます。',
  'Off': 'オフ',
  'Clean Up': '整形',
  'Summarize': '要約',
  'Local Whisper': 'ローカル Whisper',
  'Codex Subscription (Experimental)': 'Codex サブスクリプション（実験的）',
  'OpenAI-Compatible API': 'OpenAI 互換 API',
  'System On-Device': 'システム（デバイス内）',
  'System Recognition': 'システムの音声認識',
  'Record locally and transcribe with the selected Whisper model.':
      'ローカルで録音し、選択した Whisper モデルで文字起こしします。',
  'Use the experimental Codex app-server realtime API with your Codex subscription.':
      'Codex サブスクリプションで、実験的な Codex app-server realtime API を使用します。',
  'Send recordings to an OpenAI-compatible audio transcription endpoint.':
      '録音を OpenAI 互換の音声文字起こしエンドポイントに送信します。',
  'Use the platform recognizer only when it guarantees offline processing.':
      'オフライン処理が保証されている場合にのみ、プラットフォームの認識エンジンを使用します。',
  'Use the platform speech service, which may process audio online.':
      'プラットフォームの音声サービスを使用します。音声がオンラインで処理される場合があります。',
  'Send recordings to Codex or an OpenAI-compatible speech API. Transcription endpoints do not use reasoning effort.':
      '録音を Codex または OpenAI 互換の音声 API に送信します。文字起こしのエンドポイントでは推論の深さは使用されません。',
  'Runtime Update Required': 'Runtime の更新が必要です',
  'Restart Alera to replace the running sidecar before configuring remote transcription.':
      'リモート文字起こしを設定する前に、Alera を再起動して実行中のサイドカーを入れ替えてください。',
  'Allow Remote Audio Processing': 'リモートでの音声処理を許可',
  'Recordings may leave this device and are deleted locally after transcription.':
      '録音がこのデバイスの外に送信される場合があります。文字起こし後、ローカルの録音は削除されます。',
  'Realtime Model': 'Realtime モデル',
  'Optional Codex realtime model override. Leave blank to use the subscription default. This Codex API is experimental.': 'Codex realtime モデルの上書き設定（任意）。空欄の場合はサブスクリプションの既定値を使用します。この Codex API は実験的です。',
  'Subscription default': 'サブスクリプションの既定値',
  'Base URL': 'Base URL',
  'Base API URL. Alera appends /audio/transcriptions when needed and preserves query parameters.': 'API の Base URL です。Alera は必要に応じて /audio/transcriptions を追加し、クエリパラメーターはそのまま保持します。',
  'Speech-to-text model accepted by the configured API.':
      '設定した API が受け付ける音声認識モデルです。',
  'Request Timeout': 'リクエストのタイムアウト',
  'Maximum time allowed for remote transcription.': 'リモート文字起こしに許容される最大時間です。',
  'API Token': 'API トークン',
  'Checking saved token...': '保存済みトークンを確認しています…',
  'The saved token belongs to another API origin. Replace it before transcribing.':
      '保存済みのトークンは別の API オリジンのものです。文字起こしの前に置き換えてください。',
  'A token is stored for this API origin.': 'この API オリジンのトークンが保存されています。',
  'No token is stored. Tokenless local APIs are also supported.':
      'トークンは保存されていません。トークン不要のローカル API にも対応しています。',
  'Replace saved token': '保存済みトークンを置き換え',
  'Replace Token': 'トークンを置き換え',
  'Save Token': 'トークンを保存',
  'Test Transcript': '文字起こしをテスト',
  'Record a short sample with the current configuration and review the transcript here.':
      '現在の設定で短い音声を録音し、その文字起こし結果をここで確認できます。',
  'Your test transcription appears here': 'テストの文字起こし結果がここに表示されます',
  'Enable AI Dictation before testing.': 'テストの前に AI 音声入力を有効にしてください。',
  'Restart Alera to update the runtime before testing remote transcription.':
      'リモート文字起こしをテストする前に、Alera を再起動して Runtime を更新してください。',
  'Allow remote audio processing before testing this engine.':
      'このエンジンをテストする前に、リモートでの音声処理を許可してください。',
  'Select the microphone, speak, then select Stop Dictation.':
      'マイクを選択して話し、終わったら「音声入力を停止」を選択してください。',
  'Queue Download': 'ダウンロードをキューに追加',
  'Selected': '選択済み',
  'Use Model': 'このモデルを使用',
  'Queued. This download starts when the active transfer finishes.':
      'キューに追加しました。現在の転送が完了するとダウンロードが始まります。',
  'Verifying downloaded model...': 'ダウンロードしたモデルを検証しています…',
  'The model download failed.': 'モデルのダウンロードに失敗しました。',
  'Installed and selected.': 'インストールして選択しました。',
  'Installed on this device.': 'このデバイスにインストール済みです。',
  'Fastest, with lower transcription accuracy.': '最速ですが、文字起こしの精度は低めです。',
  'Balanced speed and accuracy. Recommended for most devices.':
      '速度と精度のバランスが取れています。ほとんどのデバイスに推奨します。',
  'Improved accuracy with slower transcription.': '精度は高めですが、文字起こしは遅くなります。',
  'Highest curated accuracy with the largest memory cost.':
      '最も高い精度が得られますが、メモリ使用量も最大です。',
  'The model download could not finish. Try again.':
      'モデルのダウンロードを完了できませんでした。もう一度お試しください。',
  'Select another installed model before removing this one.':
      'このモデルを削除する前に、インストール済みの別のモデルを選択してください。',
  'Improving Transcript': '文字起こしを改善しています',
  'Cancel Transcription': '文字起こしをキャンセル',
  'Stop Dictation': '音声入力を停止',
  'Start Dictation': '音声入力を開始',
  'Remote audio processing was disabled before transcription.':
      '文字起こしの前にリモートでの音声処理が無効化されました。',
  'Enable AI Dictation in Settings before recording.':
      '録音の前に、設定で AI 音声入力を有効にしてください。',
  'The dictation text field is no longer available.':
      '音声入力の対象だったテキスト欄が利用できなくなりました。',
  'Download the selected Whisper model in Settings before recording.':
      '録音の前に、設定で選択中の Whisper モデルをダウンロードしてください。',
  'Allow remote audio processing in AI Dictation settings first.':
      '先に AI 音声入力の設定でリモートの音声処理を許可してください。',
  'Microphone permission is required for AI Dictation.':
      'AI 音声入力にはマイクの権限が必要です。',
  'On-device speech recognition is unavailable for this locale.':
      'このロケールではデバイス内音声認識を利用できません。',
  'Allow online speech recognition in AI Dictation settings first.':
      '先に AI 音声入力の設定でオンライン音声認識を許可してください。',
  'The system recognizer did not produce a transcription.':
      'システムの認識エンジンから文字起こし結果が得られませんでした。',
  'The microphone did not produce an audio recording.': 'マイクから音声を録音できませんでした。',
  'The text field was closed before dictation finished.':
      '音声入力が完了する前にテキスト欄が閉じられました。',
  's': '秒',

  'Typography': 'フォント設定',
  'Cursor': 'カーソル',
  'Appearance': '外観',
  'Interaction': '操作',
  'Advanced': '詳細設定',
  'Mobile Gateway': 'モバイルゲートウェイ',
  'Link A Device': 'デバイスを関連付け',
  'Active Pairing Offers': '有効なペアリング招待',
  'Paired Devices': 'ペアリング済みデバイス',
  'Indentation': 'インデント',
  'Autosave': '自動保存',
  'Tab Size': 'タブ幅',
  'Control dependency and build directories that Quick Open never indexes.':
      'クイックオープンがインデックスしない依存ライブラリやビルド用ディレクトリを管理します。',
  'Excluded Directory Names': '除外するディレクトリ名',
  'These directory names are never indexed, even when Git-ignored files are included. Matching is case-insensitive.':
      'これらのディレクトリ名は、Git で無視されたファイルを含める場合でもインデックスされません。大文字と小文字は区別されません。',
  'Directory name, e.g. generated': 'ディレクトリ名（例: generated）',
  '.git, .hg, and .svn are always excluded.': '.git、.hg、.svn は常に除外されます。',
  'Restore Defaults': '既定値に戻す',
  'PowerShell 7 Executable': 'PowerShell 7 の実行ファイル',
  'Optional full path to pwsh.exe on Windows. Leave blank to auto-detect standard, Scoop, LocalAppData, and PATH locations.': 'Windows 上の pwsh.exe のフルパス（任意）。空欄にすると、標準のインストール先、Scoop、LocalAppData、PATH から自動検出します。',
  'Override or auto-detect the Windows pwsh.exe path.':
      'Windows の pwsh.exe のパスを指定するか、自動検出します。',
  'Theme Preset': 'テーマのプリセット',
  'Search and select a built-in terminal color theme.':
      '組み込みのターミナル配色テーマを検索して選択します。',
  'Cursor Shape': 'カーソルの形状',
  'Cursor style for new terminal sessions.': '新しいターミナルセッションで使うカーソルのスタイルです。',
  'Toolbar Corner': 'ツールバーの位置',
  'Where the pulse, composer, and refresh buttons sit on the terminal tab.':
      'ターミナルタブ上で、稼働インジケーター、入力欄、再読み込みボタンを表示する位置です。',
  'Top Left': '左上',
  'Top Right': '右上',
  'Bottom Left': '左下',
  'Bottom Right': '右下',
  'Select color': '色を選択',
  'Choose color': '色を選択',
  'spaces': '個のスペース',
  'seconds': '秒',
  'Autosave Delay': '自動保存までの待ち時間',
  'Follow System': 'システムに従う',
  'English': 'English',
  '繁體中文': '繁體中文',

  // Settings descriptions.
  'Language used by the Alera interface.': 'Alera の画面で使用する言語です。',
  'Follow the system language or choose a language for Alera.':
      'システムの言語に従うか、Alera で使う言語を選択します。',
  'Review, download and upload configuration across your devices.':
      'デバイス間の設定を確認、ダウンロード、アップロードします。',
  'Identity, mobile push and runtime ownership.':
      'アイデンティティ、モバイルプッシュ、Runtime の所有権。',
  'Storage, safety, runtime, diagnostics and updates.':
      'ストレージ、安全確認、Runtime、診断、更新。',
  'Agent hooks, notifications and Alera skills.':
      'Agent の Hook、通知、Alera Skills。',
  'Provider usage, Claude profiles and credential environment.':
      'プロバイダーの使用量、Claude プロファイル、認証情報の環境変数。',
  'Local agent assistance for commits, pull requests, diffs, workspace identity, and speech.':
      'Commit、Pull Request、Diff、ワークスペース識別子、音声のためのローカル Agent 支援。',
  'Local, Codex subscription, and OpenAI-compatible speech-to-text.':
      'ローカル、Codex サブスクリプション、OpenAI 互換の音声認識。',
  'Create reusable replacements for selected text.':
      '選択したテキストを置き換える、再利用可能な操作を作成します。',
  'Code editor defaults.': 'コードエディターの既定値。',
  'Appearance defaults for new terminal sessions.': '新しいターミナルセッションの外観の既定値。',
  'Shortcuts and key bindings.': 'ショートカットとキーバインド。',
  'Per-project workspace setup.': 'プロジェクトごとのワークスペース設定。',
  'Pair and manage the mobile companion app.': 'モバイル版コンパニオンアプリをペアリングして管理します。',
  'SSH runtime targets.': 'SSH の Runtime ターゲット。',
  'Launch configurations orchestration can dispatch to.':
      'オーケストレーションがディスパッチできる起動設定。',
  'Syntax highlighting defaults for editor tabs.': 'エディタータブのシンタックスハイライトの既定値。',
  'Defaults used by editor tabs.': 'エディタータブで使用される既定値。',
  'Spaces inserted when pressing tab.': 'Tab キーを押したときに挿入されるスペースの数。',
  'Save dirty editor tabs after they have been idle.':
      '編集中のエディタータブを一定時間操作がないと自動保存します。',
  'Automatically save editor changes after a pause.':
      '操作が止まってから、エディターの変更を自動保存します。',
  'Idle time before saving editor changes.': 'エディターの変更を保存するまでの待ち時間。',
  'Search and select a syntax highlighting theme.':
      'シンタックスハイライトのテーマを検索して選択します。',
  'External Editor': '外部エディター',
  'Open workspaces and files in an external editor without changing Alera\'s built-in editor behavior.':
      'Alera 内蔵エディターの動作を変えずに、ワークスペースやファイルを外部エディターで開きます。',
  'Editor used by Open In menu entries, keyboard shortcuts, and external file targets.':
      '「Open In」メニュー、キーボードショートカット、外部ファイルターゲットで使用するエディターです。',
  'Default Code Open Target': 'コードを開く既定の場所',
  'Choose where normal editable source and text files open. Dedicated Alera previews stay internal.':
      '通常の編集可能なソースやテキストファイルを開く場所を選択します。Alera 専用のプレビューは内部のままです。',
  'Open Workspaces in New Window': 'ワークスペースを新しいウィンドウで開く',
  'Auto-open New Workspaces Externally': '新しいワークスペースを自動的に外部エディターで開く',
  'Run a non-destructive version check with the current executable setting.':
      '現在の実行ファイル設定で、変更を伴わないバージョン確認を実行します。',
  'Search syntax themes': 'シンタックステーマを検索',
  'Prompt Append': 'プロンプトへの追記',
  'Add project-specific agent instructions': 'プロジェクト固有の Agent 指示を追加します',
  'From': '変換元',
  'To': '変換先',
  'Defaults to from': '既定では変換元と同じ',
  'Save Override': '上書き設定を保存',
  'Custom Prompt': 'カスタムプロンプト',
  'Optional instructions for every dispatched task': 'ディスパッチされる各タスクに付加する指示（任意）',
  'Quota Group': '使用枠グループ',
  'Command mode is for advanced or unsupported CLI options. Use an interactive command that can accept a dispatch and report completion.': 'Command モードは、高度な設定や未対応の CLI オプション向けです。ディスパッチを受け取り、完了を報告できる対話型コマンドを指定してください。',
  'Profiles sharing a quota group drain the same usage bucket. Alera never measures this; it only avoids falling back inside the same group. Leave empty if unsure.': '同じ使用枠グループを共有するプロファイルは、同じ枠を消費します。Alera がこれを計測することはなく、同じグループ内でのフォールバックを避けるだけです。不明な場合は空欄のままにしてください。',
  'Alias': 'エイリアス',
  'CCS Profile': 'CCS プロファイル',
  'Usage Name': '使用量の表示名',
  'Device Name': 'デバイス名',
  'My Phone': 'マイフォン',
  'Generating…': '生成中…',
  'Generate': '生成',
  'Host': 'ホスト',
  'Username': 'ユーザー名',
  'Port': 'ポート',
  'Install Directory': 'インストール先ディレクトリ',
  'Default per platform': 'プラットフォームごとの既定値',
  'Search built-in themes': '組み込みテーマを検索',
  'Initial Prompt': '最初のプロンプト',
  'Describe what the agent should build or paste an image':
      'Agent に作らせたい内容を記述するか、画像を貼り付けてください',
  'Loading branches': 'Branch を読み込んでいます',
  'Select Branch': 'Branch を選択',
  'Create an agent profile in settings': '先に設定で Agent プロファイルを作成してください',
  'Select Agent Profile': 'Agent プロファイルを選択',
  'Create Another': '続けて作成',
  'Working': '処理中',
  'Complete the prompt, project, branch, and agent profile.':
      'プロンプト、プロジェクト、Branch、Agent プロファイルをすべて入力してください。',
  'Generating workspace identity': 'ワークスペースの識別子を生成しています',
  'Checking generated branch': '生成された Branch を確認しています',
  'Creating workspace': 'ワークスペースを作成しています',
  'Starting agent': 'Agent を起動しています',
  'Could not paste clipboard image.': 'クリップボードの画像を貼り付けられませんでした。',
  'Choose every workspace setting yourself, including the branch name and optional parent workspace.':
      'Branch 名や親ワークスペース（任意）を含め、ワークスペースの設定をすべて自分で指定します。',
  'Describe the replacement to generate.': '生成したい置き換え内容を記述してください。',
  'Define the reusable instruction and its availability.':
      '再利用する指示と、その利用可否を定義します。',
  'Show this action in the Text Actions menu.':
      'この操作を Text Actions メニューに表示します。',
  'Choose which CLI and model run this action.': 'この操作を実行する CLI とモデルを選択します。',
  'Inherit the global AI Assist agent by default.':
      '既定ではグローバルの AI Assist Agent を引き継ぎます。',
  'Inherit the selected model unless overridden.': '上書きしない限り、選択中のモデルを引き継ぎます。',
  'Reasoning effort for the effective model.': '実際に使用するモデルの推論の深さです。',
  'Action': '操作',
  'Enabled': '有効',
  'Reasoning': '推論',
  'No text actions': 'テキスト操作がありません',
  'Select a text action': 'テキスト操作を選択',
  'Delete Text Action': 'テキスト操作を削除',
  'Action ID is required.': '操作 ID は必須です。',
  'Action name is required.': '操作名は必須です。',
  'Action prompt is required.': '操作のプロンプトは必須です。',
  'Action IDs must be unique.': '操作 ID は重複できません。',
  'Action names must be unique.': '操作名は重複できません。',
  'Text changed while the action was running.': '操作の実行中にテキストが変更されました。',
  'Text action returned no replacement text.': 'テキスト操作から置き換えるテキストが返されませんでした。',
  'Text action could not update this field.': 'テキスト操作でこの入力欄を更新できませんでした。',
  'Text action applied.': 'テキスト操作を適用しました。',
  'Text action was canceled.': 'テキスト操作をキャンセルしました。',
  'No matching options': '一致する選択肢がありません',
};
