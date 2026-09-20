part of 'alera_localizations.dart';

// Pattern-based Japanese translations for strings that embed runtime values.
// Lookup order: static table first, then these rules. Mirrors the rule set in
// `_translateDynamicTraditionalChinese`.
String? _translateDynamicJapanese(String source) {
  final sleepProject = RegExp(
    r'^This closes all tabs and terminal sessions for all (\d+) workspaces in "(.+)"\. Worktrees, branches, and files will be preserved\.(.*)$',
  ).firstMatch(source);
  if (sleepProject != null) {
    final suffix = sleepProject.group(3)!;
    var dirtyWarning = '';
    if (suffix == ' One editor has unsaved changes that will be discarded.') {
      dirtyWarning = ' 保存していない変更のあるエディターが 1 件あり、その変更は破棄されます。';
    } else {
      final dirtyEditors = RegExp(
        r'^ (\d+) editors have unsaved changes that will be discarded\.$',
      ).firstMatch(suffix);
      if (dirtyEditors != null) {
        dirtyWarning =
            ' 保存していない変更のあるエディターが ${dirtyEditors.group(1)} 件あり、その変更は破棄されます。';
      }
    }
    return '「${sleepProject.group(2)}」にある ${sleepProject.group(1)} 個のワークスペースのタブとターミナルセッションをすべて閉じます。Worktree、ブランチ、ファイルは保持されます。$dirtyWarning';
  }
  final sleepProjectFailed = RegExp(r'^Could not sleep project: (.+)$')
      .firstMatch(source);
  if (sleepProjectFailed != null) {
    return 'プロジェクトをスリープできませんでした: ${sleepProjectFailed.group(1)}';
  }
  final diffOverview = RegExp(r'^Diff overview, (\d+) removed, (\d+) added$')
      .firstMatch(source);
  if (diffOverview != null) {
    return 'Diff の概要、削除 ${diffOverview.group(1)} 行、追加 ${diffOverview.group(2)} 行';
  }
  final requiredField = RegExp(r'^(.+) is required$').firstMatch(source);
  if (requiredField != null) {
    final label = AleraLocalizations._lookupJapanese(requiredField.group(1)!);
    return '$label は必須です。';
  }
  final agentGroupDescription = RegExp(
    r'^(.+) · (Waiting for input|Blocked|Interrupted|Done \(Unread\)|Done)$',
  ).firstMatch(source);
  if (agentGroupDescription != null) {
    final state = AleraLocalizations._lookupJapanese(
      agentGroupDescription.group(2)!,
    );
    return '${agentGroupDescription.group(1)} · $state';
  }
  final rebaseCurrentBranchOnto = RegExp(r'^Rebase Current Branch onto (.+)$')
      .firstMatch(source);
  if (rebaseCurrentBranchOnto != null) {
    return '現在のブランチを ${rebaseCurrentBranchOnto.group(1)} に Rebase';
  }
  final autosavePaused = RegExp(r'^Autosave paused: (.+)$').firstMatch(source);
  if (autosavePaused != null) {
    final detail = AleraLocalizations._lookupJapanese(autosavePaused.group(1)!);
    return '自動保存を一時停止しました: $detail';
  }
  final dropCommitMessage = RegExp(
    r'^Removes (.+) "(.+)" from the current branch\. This cannot be undone\.$',
  ).firstMatch(source);
  if (dropCommitMessage != null) {
    return '現在のブランチから ${dropCommitMessage.group(1)}「${dropCommitMessage.group(2)}」を削除します。この操作は元に戻せません。';
  }
  final mergeCommitMessage = RegExp(r'^Merge (.+) into (.+)\?$')
      .firstMatch(source);
  if (mergeCommitMessage != null) {
    return '${mergeCommitMessage.group(1)} を ${mergeCommitMessage.group(2)} にマージしますか？';
  }
  final rebaseCommitMessage = RegExp(
    r'^Rebase (.+) onto (.+) "(.+)"\. This rewrites the current branch history\.$',
  ).firstMatch(source);
  if (rebaseCommitMessage != null) {
    return '${rebaseCommitMessage.group(1)} を ${rebaseCommitMessage.group(2)}「${rebaseCommitMessage.group(3)}」に Rebase します。現在のブランチの履歴が書き換えられます。';
  }
  final singleDirty = RegExp(r'^(.+) has unsaved changes\.$')
      .firstMatch(source);
  if (singleDirty != null) {
    return '「${singleDirty.group(1)}」に保存していない変更があります。';
  }
  final multipleDirty = RegExp(r'^(\d+) editor tabs have unsaved changes\.$')
      .firstMatch(source);
  if (multipleDirty != null) {
    return '${multipleDirty.group(1)} 個のエディタータブに保存していない変更があります。';
  }
  final activeAgent = RegExp(
    r'^An agent is actively working in "(.+)"\. Closing will terminate the session\.$',
  ).firstMatch(source);
  if (activeAgent != null) {
    return 'Agent が「${activeAgent.group(1)}」で作業中です。閉じるとこのセッションは終了します。';
  }
  final runningProcess = RegExp(
    r'^The process "(.+)" is still running in "(.+)"\. Closing will terminate it\.$',
  ).firstMatch(source);
  if (runningProcess != null) {
    return 'プロセス「${runningProcess.group(1)}」が「${runningProcess.group(2)}」でまだ実行中です。閉じるとそのプロセスは終了します。';
  }
  final runningCommand = RegExp(
    r'^A command is still running in "(.+)"\. Closing will terminate it\.$',
  ).firstMatch(source);
  if (runningCommand != null) {
    return '「${runningCommand.group(1)}」でコマンドがまだ実行中です。閉じるとそのコマンドは終了します。';
  }
  final busyTerminals = RegExp(
    r'^(\d+) terminal tabs have running processes or active agents\. Closing will terminate them\.$',
  ).firstMatch(source);
  if (busyTerminals != null) {
    return '${busyTerminals.group(1)} 個のターミナルタブに実行中のプロセスまたはアクティブな Agent があります。閉じるとそれらは終了します。';
  }
  final preservedAttachments = RegExp(
    r'^(\d+) attached (?:item|items) will be preserved\.$',
  ).firstMatch(source);
  if (preservedAttachments != null) {
    return '${preservedAttachments.group(1)} 件の添付ファイルは保持されます。';
  }

  final effortLabel = RegExp(r'^(.+) Effort$').firstMatch(source);
  if (effortLabel != null) {
    return '推論の深さ: ${effortLabel.group(1)}';
  }
  final chunkCount = RegExp(r'^(\d+) (?:Chunk|Chunks)$').firstMatch(source);
  if (chunkCount != null) {
    return '${chunkCount.group(1)} 個のチャンク';
  }
  final retainedLines = RegExp(r'^Kept (\d+)/(\d+) Changed Lines$')
      .firstMatch(source);
  if (retainedLines != null) {
    return '変更行 ${retainedLines.group(1)}/${retainedLines.group(2)} を保持';
  }
  final chunkPosition = RegExp(r'^Chunk (\d+) of (\d+)$').firstMatch(source);
  if (chunkPosition != null) {
    return 'チャンク ${chunkPosition.group(1)}/${chunkPosition.group(2)}';
  }
  final generatingChunk = RegExp(r'^Generating chunk (\d+) of (\d+)$')
      .firstMatch(source);
  if (generatingChunk != null) {
    return 'チャンク ${generatingChunk.group(1)}/${generatingChunk.group(2)} を生成しています';
  }
  final repairingChunk = RegExp(r'^Repairing chunk (\d+) of (\d+)$')
      .firstMatch(source);
  if (repairingChunk != null) {
    return 'チャンク ${repairingChunk.group(1)}/${repairingChunk.group(2)} を修復しています';
  }
  final combiningChunks = RegExp(r'^Combining (\d+) (?:chunk|chunks)$')
      .firstMatch(source);
  if (combiningChunks != null) {
    return '${combiningChunks.group(1)} 個のチャンクを結合しています';
  }

  final shortcutConflict = RegExp(
    r'^(.+) is assigned to "(.+)"\. Reassign it to "(.+)"\?$',
  ).firstMatch(source);
  if (shortcutConflict != null) {
    final conflictLabel = AleraLocalizations._lookupJapanese(
      shortcutConflict.group(2)!,
    );
    final targetLabel = AleraLocalizations._lookupJapanese(
      shortcutConflict.group(3)!,
    );
    return '${shortcutConflict.group(1)} は「$conflictLabel」に割り当てられています。「$targetLabel」に割り当て直しますか？';
  }
  final unsupportedKey = RegExp(r'^Unsupported key: (.+)\.$')
      .firstMatch(source);
  if (unsupportedKey != null) {
    return 'サポートされていないキーです: ${unsupportedKey.group(1)}。';
  }

  final sortBy = RegExp(r'^Sort By (.+)$').firstMatch(source);
  if (sortBy != null) {
    final label = AleraLocalizations._lookupJapanese(sortBy.group(1)!);
    return '$label で並べ替え';
  }
  final orphanTerminals = RegExp(r'^(\d+) orphan terminal(?:s)?$')
      .firstMatch(source);
  if (orphanTerminals != null) {
    return '孤立したターミナル ${orphanTerminals.group(1)} 件';
  }
  final forceQuitTerminal = RegExp(
    r'^Force-quits (.+)\. Anything running in that terminal is lost\.$',
  ).firstMatch(source);
  if (forceQuitTerminal != null) {
    return '「${forceQuitTerminal.group(1)}」を強制終了します。そのターミナルで実行中の内容はすべて失われます。';
  }

  final usageCount = RegExp(
    r'^(\d+) (assistant responses|transcript sources|unpriced responses)$',
  ).firstMatch(source);
  if (usageCount != null) {
    final noun = switch (usageCount.group(2)) {
      'assistant responses' => '件の Assistant 応答',
      'transcript sources' => '件のトランスクリプトソース',
      _ => '件の未課金の応答',
    };
    return '${usageCount.group(1)} $noun';
  }
  final usageInputShare = RegExp(r'^([0-9.]+%) of input$').firstMatch(source);
  if (usageInputShare != null) {
    return '入力の ${usageInputShare.group(1)}';
  }
  final usageScanSummary = RegExp(
    r'^Scanned (\d+) files in (\d+) ms\. Transcript content stays on this host\.$',
  ).firstMatch(source);
  if (usageScanSummary != null) {
    return '${usageScanSummary.group(1)} 件のファイルを ${usageScanSummary.group(2)} ms でスキャンしました。トランスクリプトの内容はこの Host に保持されます。';
  }
  final usagePartial = RegExp(r'^(.+) (.+) is partial\.$').firstMatch(source);
  if (usagePartial != null) {
    return '${usagePartial.group(1)} ${usagePartial.group(2)} のデータは不完全です。';
  }
  final dailyUsageSemantics = RegExp(
    r'^Daily Claude Code, Codex, and Grok Build token usage\. (.+)$',
  ).firstMatch(source);
  if (dailyUsageSemantics != null) {
    return 'Claude Code、Codex、Grok Build の日次トークン使用量。${dailyUsageSemantics.group(1)}';
  }

  final pullRequestCount = RegExp(r'^(Checks|Comments) \((\d+)\)$')
      .firstMatch(source);
  if (pullRequestCount != null) {
    final label = pullRequestCount.group(1) == 'Checks' ? 'チェック' : 'コメント';
    return '$label（${pullRequestCount.group(2)}）';
  }
  final checkGroup = RegExp(r'^(\d+) (failing|in progress|successful) Checks?$')
      .firstMatch(source);
  if (checkGroup != null) {
    final state = switch (checkGroup.group(2)) {
      'failing' => '失敗',
      'in progress' => '実行中',
      _ => '成功',
    };
    return '$state のチェック ${checkGroup.group(1)} 件';
  }

  final installProviderCli = RegExp(
    r'^Install `(.+)` and ensure it is on your PATH\.$',
  ).firstMatch(source);
  if (installProviderCli != null) {
    return '`${installProviderCli.group(1)}` をインストールし、PATH に含まれていることを確認してください。';
  }
  final runProviderAuth = RegExp(r'^Run `(.+)` to sign in, then refresh\.$')
      .firstMatch(source);
  if (runProviderAuth != null) {
    return '`${runProviderAuth.group(1)}` を実行してサインインし、再読み込みしてください。';
  }

  final accountRuntime = RegExp(r'^Runtime (.+)$').firstMatch(source);
  if (accountRuntime != null) {
    return 'Runtime ${accountRuntime.group(1)}';
  }
  final linkIdentityProvider = RegExp(r'^Link (Google|GitHub)$')
      .firstMatch(source);
  if (linkIdentityProvider != null) {
    return '${linkIdentityProvider.group(1)} を関連付け';
  }
  final activeMobileSubscriptions = RegExp(
    r'^(\d+) active mobile subscription\(s\)\.$',
  ).firstMatch(source);
  if (activeMobileSubscriptions != null) {
    return '有効なモバイルのサブスクリプションが ${activeMobileSubscriptions.group(1)} 件あります。';
  }
  final transferRuntimeAccount = RegExp(
    r'^Transfer this runtime and its mobile subscriptions to account (.+)\? This installation will sign out\.$',
  ).firstMatch(source);
  if (transferRuntimeAccount != null) {
    return 'この Runtime とモバイルのサブスクリプションをアカウント ${transferRuntimeAccount.group(1)} に移管しますか？このインストールはサインアウトされます。';
  }
  final signInFailure = RegExp(r'^Sign in failed: (.+)$').firstMatch(source);
  if (signInFailure != null) {
    return 'サインインに失敗しました: ${signInFailure.group(1)}';
  }

  final resizeMasterList = RegExp(r'^Resize (.+) List$').firstMatch(source);
  if (resizeMasterList != null) {
    final label = AleraLocalizations._lookupJapanese(
      resizeMasterList.group(1)!,
    );
    return '$label リストのサイズを変更';
  }

  final allAutomationFilter = RegExp(r'^All (State|Project|Profile|Tag)$')
      .firstMatch(source);
  if (allAutomationFilter != null) {
    final label = AleraLocalizations._lookupJapanese(
      allAutomationFilter.group(1)!,
    );
    return 'すべての$label';
  }

  final unknownPromptVariable = RegExp(r'^Unknown prompt variable: (.+)$')
      .firstMatch(source);
  if (unknownPromptVariable != null) {
    return '不明なプロンプト変数: ${unknownPromptVariable.group(1)}';
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
    return 'ダウンロードが ${dictationInterrupted.group(1)} で中断しました。準備ができたら再開できます。';
  }
  final dictationDownloadSize = RegExp(
    r'^Download size (\d+(?:\.\d+)? (?:KiB|MiB))\.$',
  ).firstMatch(source);
  if (dictationDownloadSize != null) {
    return 'ダウンロードサイズ ${dictationDownloadSize.group(1)}。';
  }
  final systemRecognitionFailed = RegExp(
    r'^System speech recognition failed: (.+)$',
  ).firstMatch(source);
  if (systemRecognitionFailed != null) {
    return 'システムの音声認識に失敗しました: ${systemRecognitionFailed.group(1)}';
  }
  final systemRecognitionStartFailed = RegExp(
    r'^System speech recognition could not start: (.+)$',
  ).firstMatch(source);
  if (systemRecognitionStartFailed != null) {
    return 'システムの音声認識を開始できませんでした: ${systemRecognitionStartFailed.group(1)}';
  }
  final speechProcessingFallback = RegExp(
    r'^The transcript was inserted without speech processing: (.+)$',
  ).firstMatch(source);
  if (speechProcessingFallback != null) {
    return '音声処理に失敗したため、文字起こし結果をそのまま挿入しました: ${speechProcessingFallback.group(1)}';
  }

  final originalPath = RegExp(r'^Original \((.+)\)$').firstMatch(source);
  if (originalPath != null) {
    return '変更前（${originalPath.group(1)}）';
  }

  final runtimeBusyWithSuffix = RegExp(
    r'^The runtime has (.+)\. (Force stop terminates them\.|You can quit and leave the runtime running, or force stop it\.)$',
  ).firstMatch(source);
  if (runtimeBusyWithSuffix != null) {
    final details = _translateRuntimeBusyItemsJapanese(
      runtimeBusyWithSuffix.group(1)!,
    );
    final suffix = runtimeBusyWithSuffix.group(2)!;
    final translatedSuffix = switch (suffix) {
      'Force stop terminates them.' => '強制停止するとそれらは終了します。',
      _ => 'Alera を終了して Runtime を起動したままにすることも、強制停止することもできます。',
    };
    return 'Runtime には現在 $detailsがあります。$translatedSuffix';
  }

  final runtimeBusy = RegExp(r'^The runtime has (.+)\.$').firstMatch(source);
  if (runtimeBusy != null) {
    final details = _translateRuntimeBusyItemsJapanese(runtimeBusy.group(1)!);
    return 'Runtime には現在 $detailsがあります。';
  }

  final generatedBranchExists = RegExp(
    r'^(?:Bad state: )?The generated branch "(.+)" already exists\.$',
  ).firstMatch(source);
  if (generatedBranchExists != null) {
    return '生成された Branch「${generatedBranchExists.group(1)}」はすでに存在します。';
  }
  final workspaceIdentityUnavailable = RegExp(
    r'^(?:Bad state: )?AI Assist could not generate an available workspace identity\.$',
  ).firstMatch(source);
  if (workspaceIdentityUnavailable != null) {
    return 'AI Assist は利用可能なワークスペースの識別子を生成できませんでした。';
  }
  final retryAgentRequiresUpdate = RegExp(
    r'^(?:Unsupported operation: )?Update Alera on this host before retrying agent launch safely\.$',
  ).firstMatch(source);
  if (retryAgentRequiresUpdate != null) {
    return 'Agent の起動を安全に再試行するには、先にこの Host の Alera を更新してください。';
  }
  final originalAgentLaunchUnavailable = RegExp(
    r'^(?:Bad state: )?The original agent launch identity is unavailable\.$',
  ).firstMatch(source);
  if (originalAgentLaunchUnavailable != null) {
    return '元の Agent 起動識別子は利用できません。';
  }

  final globalValue = RegExp(r'^Global \((.+)\)$').firstMatch(source);
  if (globalValue != null) {
    final value = globalValue.group(1)!;
    final translatedValue = AleraLocalizations._lookupJapanese(value);
    return 'グローバル（$translatedValue）';
  }
  final runningTextAction = RegExp(r'^Running (.+)\.$').firstMatch(source);
  if (runningTextAction != null) {
    return '「${runningTextAction.group(1)}」を実行しています。';
  }
  final textActionFailed = RegExp(r'^Text action failed: (.+)$')
      .firstMatch(source);
  if (textActionFailed != null) {
    return 'テキスト操作に失敗しました: ${textActionFailed.group(1)}';
  }

  final editorAvailable = RegExp(
    r'^(Zed|VS Code|Visual Studio Code) is available(?:: (.+))?\.?$',
  ).firstMatch(source);
  if (editorAvailable != null) {
    final version = editorAvailable.group(2);
    return version == null
        ? '${editorAvailable.group(1)} は利用できます。'
        : '${editorAvailable.group(1)} は利用できます: $version';
  }
  final editorUnavailable = RegExp(
    r'^(Zed|VS Code|Visual Studio Code) is not available\.$',
  ).firstMatch(source);
  if (editorUnavailable != null) {
    return '${editorUnavailable.group(1)} は利用できません。';
  }
  final editorNotFound = RegExp(
    r'^(.+) was not found\. Check Settings > Editor\.$',
  ).firstMatch(source);
  if (editorNotFound != null) {
    return '${editorNotFound.group(1)} が見つかりません。設定 > エディターを確認してください。';
  }
  final editorVersionFailed = RegExp(
    r'^(.+) was found but its version check failed\.$',
  ).firstMatch(source);
  if (editorVersionFailed != null) {
    return '${editorVersionFailed.group(1)} は見つかりましたが、バージョン確認に失敗しました。';
  }
  final editorNotStartable = RegExp(
    r'^(.+) could not be started\. Check Settings > Editor\.$',
  ).firstMatch(source);
  if (editorNotStartable != null) {
    return '${editorNotStartable.group(1)} を起動できませんでした。設定 > エディターを確認してください。';
  }
  final editorStartFailed = RegExp(
    r'^Could not start (.+)\. Configure the executable in Settings > Editor\.$',
  ).firstMatch(source);
  if (editorStartFailed != null) {
    return '${editorStartFailed.group(1)} を起動できませんでした。設定 > エディターで実行ファイルを指定してください。';
  }
  final openPathFailed = RegExp(
    r'^Could not open the requested path in (.+)\. Check Settings > Editor and try again\.$',
  ).firstMatch(source);
  if (openPathFailed != null) {
    return '指定されたパスを ${openPathFailed.group(1)} で開けませんでした。設定 > エディターを確認してからもう一度お試しください。';
  }
  final openTargetMissing = RegExp(
    r'^The requested (.+) file target does not exist\.$',
  ).firstMatch(source);
  if (openTargetMissing != null) {
    return '指定された ${openTargetMissing.group(1)} のファイルターゲットは存在しません。';
  }
  final openTargetOutside = RegExp(
    r'^The requested (.+) file target is outside the active Alera workspace\.$',
  ).firstMatch(source);
  if (openTargetOutside != null) {
    return '指定された ${openTargetOutside.group(1)} のファイルターゲットは、アクティブな Alera ワークスペースの外にあります。';
  }
  final openTargetsInvalid = RegExp(
    r'^(?:The|A) requested (.+) file targets? (?:are invalid|is missing or outside the active Alera workspace)\.$',
  ).firstMatch(source);
  if (openTargetsInvalid != null) {
    return '指定された ${openTargetsInvalid.group(1)} のファイルターゲットは無効か、アクティブな Alera ワークスペースの外にあります。';
  }
  final editorPositionInvalid = RegExp(
    r'^(.+) line and column values must be positive\.$',
  ).firstMatch(source);
  if (editorPositionInvalid != null) {
    return '${editorPositionInvalid.group(1)} の行と列の値は正の数である必要があります。';
  }
  final openInEditor = RegExp(r'^Open in (.+)$').firstMatch(source);
  if (openInEditor != null) {
    return '${openInEditor.group(1)} で開く';
  }
  final openChangesInEditor = RegExp(r'^Open Changes in (.+)$')
      .firstMatch(source);
  if (openChangesInEditor != null) {
    return '変更を ${openChangesInEditor.group(1)} で開く';
  }
  final couldNotOpenIn = RegExp(
    r'^Could not open (file|item|changed files|workspace) in (.+)\.$',
  ).firstMatch(source);
  if (couldNotOpenIn != null) {
    final noun = switch (couldNotOpenIn.group(1)) {
      'file' => 'ファイル',
      'item' => '項目',
      'changed files' => '変更されたファイル',
      _ => 'ワークスペース',
    };
    return '$noun を ${couldNotOpenIn.group(2)} で開けませんでした。';
  }
  final workspaceCreatedNoEditor = RegExp(
    r'^Workspace created, but (.+) could not be opened\.$',
  ).firstMatch(source);
  if (workspaceCreatedNoEditor != null) {
    return 'ワークスペースは作成されましたが、${workspaceCreatedNoEditor.group(1)} を開けませんでした。';
  }
  final customExecutable = RegExp(r'^Custom (.+) Executable$')
      .firstMatch(source);
  if (customExecutable != null) {
    return 'カスタムの ${customExecutable.group(1)} 実行ファイル';
  }
  final commandEnvironment = RegExp(
    r'^Off uses the (.+) command from the local command environment\.$',
  ).firstMatch(source);
  if (commandEnvironment != null) {
    return 'オフの場合は、ローカルのコマンド環境にある ${commandEnvironment.group(1)} コマンドを使用します。';
  }
  final executableTitle = RegExp(r'^(Zed|VS Code) Executable$')
      .firstMatch(source);
  if (executableTitle != null) {
    return '${executableTitle.group(1)} の実行ファイル';
  }
  final executablePathDescription = RegExp(
    r'^Full path to the (.+) executable on this machine\.$',
  ).firstMatch(source);
  if (executablePathDescription != null) {
    return 'このマシン上の ${executablePathDescription.group(1)} 実行ファイルのフルパスです。';
  }
  final executablePathHint = RegExp(r'^Path to (.+) or its executable$')
      .firstMatch(source);
  if (executablePathHint != null) {
    return '${executablePathHint.group(1)} またはその実行ファイルのパス';
  }
  final newWindowPerWorktree = RegExp(
    r'^Open each Alera worktree as a separate (.+) window instead of reusing the last one\.$',
  ).firstMatch(source);
  if (newWindowPerWorktree != null) {
    return '各 Alera worktree を、直前のウィンドウを再利用せずに個別の ${newWindowPerWorktree.group(1)} ウィンドウで開きます。';
  }
  final autoOpenInEditor = RegExp(
    r'^After Alera creates a linked workspace, open that workspace in (.+) automatically\.$',
  ).firstMatch(source);
  if (autoOpenInEditor != null) {
    return 'Alera がリンク済みワークスペースを作成した後、そのワークスペースを ${autoOpenInEditor.group(1)} で自動的に開きます。';
  }
  final checkEditor = RegExp(r'^Check (Zed|VS Code)$').firstMatch(source);
  if (checkEditor != null) {
    return '${checkEditor.group(1)} を確認';
  }

  final selectedValue = RegExp(r'^Selected: (.+)$').firstMatch(source);
  if (selectedValue != null) {
    return '選択中: ${selectedValue.group(1)}';
  }
  final showingCount = RegExp(r'^Showing (\d+) of (\d+)$').firstMatch(source);
  if (showingCount != null) {
    return '${showingCount.group(2)} 件中 ${showingCount.group(1)} 件を表示';
  }

  final interfaceLabel = RegExp(r'^Interface \((.+)\)$').firstMatch(source);
  if (interfaceLabel != null) {
    return 'ネットワークインターフェイス（${interfaceLabel.group(1)}）';
  }
  final expiresMinutesSeconds = RegExp(r'^Expires in (\d+)m (\d+)s$')
      .firstMatch(source);
  if (expiresMinutesSeconds != null) {
    return '${expiresMinutesSeconds.group(1)} 分 ${expiresMinutesSeconds.group(2)} 秒後に期限切れ';
  }
  final expiresMinutes = RegExp(r'^Expires in (\d+)m$').firstMatch(source);
  if (expiresMinutes != null) {
    return '${expiresMinutes.group(1)} 分後に期限切れ';
  }
  final expiresSeconds = RegExp(r'^Expires in (\d+)s$').firstMatch(source);
  if (expiresSeconds != null) {
    return '${expiresSeconds.group(1)} 秒後に期限切れ';
  }
  final mobileTimestamp = RegExp(r'^(Revoked|Paired|Last seen) (.+)$')
      .firstMatch(source);
  if (mobileTimestamp != null) {
    final label = switch (mobileTimestamp.group(1)) {
      'Revoked' => '無効化',
      'Paired' => 'ペアリング',
      _ => '最終接続',
    };
    return '$label ${mobileTimestamp.group(2)}';
  }
  if (source == 'Connected through relay') {
    return 'Relay 経由で接続中';
  }
  final versionLabel = RegExp(
    r'^(Current version|Update version) (.+?)(?: \(build (.+)\))?$',
  ).firstMatch(source);
  if (versionLabel != null) {
    final prefix = versionLabel.group(1) == 'Current version'
        ? '現在のバージョン'
        : '更新後のバージョン';
    final build = versionLabel.group(3);
    return build == null
        ? '$prefix ${versionLabel.group(2)}'
        : '$prefix ${versionLabel.group(2)}（Build $build）';
  }
  final modelPassedTo = RegExp(r'^Model passed to (.+)\.$').firstMatch(source);
  if (modelPassedTo != null) {
    return '${modelPassedTo.group(1)} に渡すモデルです。';
  }
  final usageLabel = RegExp(r'^Usage: (.+)$').firstMatch(source);
  if (usageLabel != null) {
    return 'Usage: ${usageLabel.group(1)}';
  }
  final moveEarlier = RegExp(r'^Move (.+) Earlier$').firstMatch(source);
  if (moveEarlier != null) {
    return '${moveEarlier.group(1)} を前に移動';
  }
  final moveLater = RegExp(r'^Move (.+) Later$').firstMatch(source);
  if (moveLater != null) {
    return '${moveLater.group(1)} を後ろに移動';
  }
  final downloadingUpdate = RegExp(r'^Downloading update (.+)\.$')
      .firstMatch(source);
  if (downloadingUpdate != null) {
    return '更新 ${downloadingUpdate.group(1)} をダウンロードしています。';
  }
  final installingUpdate = RegExp(
    r'^Installing update (.+)\. Alera will restart\.$',
  ).firstMatch(source);
  if (installingUpdate != null) {
    return '更新 ${installingUpdate.group(1)} をインストールしています。Alera が再起動します。';
  }
  final updateReady = RegExp(r'^Update (.+) is ready to install\.$')
      .firstMatch(source);
  if (updateReady != null) {
    return '更新 ${updateReady.group(1)} をインストールできます。';
  }
  final upgradingThrough = RegExp(
    r'^Upgrading through (.+)\. Alera will close and reopen\.$',
  ).firstMatch(source);
  if (upgradingThrough != null) {
    return '${upgradingThrough.group(1)} を通じて更新しています。Alera は終了後に再度開きます。';
  }
  final packageManagerClose = RegExp(
    r'^Alera will close, let (.+) install the update, and open again\.$',
  ).firstMatch(source);
  if (packageManagerClose != null) {
    return 'Alera を終了し、${packageManagerClose.group(1)} が更新をインストールした後、再度開きます。';
  }
  final updateInstallFailed = RegExp(r'^Update installation failed: (.+)$')
      .firstMatch(source);
  if (updateInstallFailed != null) {
    return '更新のインストールに失敗しました: ${updateInstallFailed.group(1)}';
  }
  final packageUpgradeFailed = RegExp(
    r'^The (.+) upgrade could not be started: (.+)$',
  ).firstMatch(source);
  if (packageUpgradeFailed != null) {
    return '${packageUpgradeFailed.group(1)} による更新を開始できませんでした: ${packageUpgradeFailed.group(2)}';
  }
  final restartFailed = RegExp(r'^Alera could not restart: (.+)$')
      .firstMatch(source);
  if (restartFailed != null) {
    return 'Alera を再起動できませんでした: ${restartFailed.group(1)}';
  }
  final desktopUpdateUnavailable = RegExp(
    r'^Desktop updates are not available on (.+)\.$',
  ).firstMatch(source);
  if (desktopUpdateUnavailable != null) {
    return '${desktopUpdateUnavailable.group(1)} ではデスクトップ版の自動更新を利用できません。';
  }

  return null;
}

String _translateRuntimeBusyItemsJapanese(String source) {
  return source
      .replaceAllMapped(
        RegExp(r'(\d+) open agent\(s\)'),
        (match) => '${match.group(1)} 件の実行中の Agent',
      )
      .replaceAllMapped(
        RegExp(r'(\d+) active terminal session\(s\)'),
        (match) => '${match.group(1)} 件のアクティブなターミナルセッション',
      )
      .replaceAllMapped(
        RegExp(r'(\d+) active background job\(s\)'),
        (match) => '${match.group(1)} 件のアクティブなバックグラウンドジョブ',
      )
      .replaceAllMapped(
        RegExp(r'(\d+) active push subscription\(s\)'),
        (match) => '${match.group(1)} 件のアクティブなプッシュ購読',
      )
      .replaceAll(', and ', '、')
      .replaceAll(' and ', '、')
      .replaceAll(', ', '、');
}
