part of 'alera_localizations.dart';

// Pattern-based Traditional Chinese translations for strings that embed
// runtime values. Lookup order: static table first, then these rules.
String? _translateDynamicTraditionalChinese(String source) {
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
        AleraLocalizations._traditionalChinese[shortcutConflict.group(2)!] ??
        shortcutConflict.group(2)!;
    final targetLabel =
        AleraLocalizations._traditionalChinese[shortcutConflict.group(3)!] ??
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
    final label =
        AleraLocalizations._traditionalChinese[sortBy.group(1)!] ??
        sortBy.group(1)!;
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
  final checkGroup = RegExp(r'^(\d+) (failing|in progress|successful) Checks?$')
      .firstMatch(source);
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

  final resizeMasterList = RegExp(r'^Resize (.+) List$').firstMatch(source);
  if (resizeMasterList != null) {
    final label =
        AleraLocalizations._traditionalChinese[resizeMasterList.group(1)!] ??
        resizeMasterList.group(1)!;
    return '調整$label清單大小';
  }

  final allAutomationFilter = RegExp(r'^All (State|Project|Profile|Tag)$')
      .firstMatch(source);
  if (allAutomationFilter != null) {
    final label =
        AleraLocalizations._traditionalChinese[allAutomationFilter.group(1)!] ??
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
  final dictationDownloadSize = RegExp(
    r'^Download size (\d+(?:\.\d+)? (?:KiB|MiB))\.$',
  ).firstMatch(source);
  if (dictationDownloadSize != null) {
    return '下載大小 ${dictationDownloadSize.group(1)}。';
  }
  final systemRecognitionFailed = RegExp(
    r'^System speech recognition failed: (.+)$',
  ).firstMatch(source);
  if (systemRecognitionFailed != null) {
    return '系統語音辨識失敗：${systemRecognitionFailed.group(1)}';
  }
  final systemRecognitionStartFailed = RegExp(
    r'^System speech recognition could not start: (.+)$',
  ).firstMatch(source);
  if (systemRecognitionStartFailed != null) {
    return '無法啟動系統語音辨識：${systemRecognitionStartFailed.group(1)}';
  }
  final speechProcessingFallback = RegExp(
    r'^The transcript was inserted without speech processing: (.+)$',
  ).firstMatch(source);
  if (speechProcessingFallback != null) {
    return '語音處理失敗，已插入原始轉錄內容：${speechProcessingFallback.group(1)}';
  }

  final originalPath = RegExp(r'^Original \((.+)\)$').firstMatch(source);
  if (originalPath != null) {
    return '原始（${originalPath.group(1)}）';
  }

  final runtimeBusyWithSuffix = RegExp(
    r'^The runtime has (.+)\. (Force stop terminates them\.|You can quit and leave the runtime running, or force stop it\.)$',
  ).firstMatch(source);
  if (runtimeBusyWithSuffix != null) {
    final details = _translateRuntimeBusyItems(runtimeBusyWithSuffix.group(1)!);
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

  final generatedBranchExists = RegExp(
    r'^(?:Bad state: )?The generated branch "(.+)" already exists\.$',
  ).firstMatch(source);
  if (generatedBranchExists != null) {
    return '產生的 Branch「${generatedBranchExists.group(1)}」已存在。';
  }
  final workspaceIdentityUnavailable = RegExp(
    r'^(?:Bad state: )?AI Assist could not generate an available workspace identity\.$',
  ).firstMatch(source);
  if (workspaceIdentityUnavailable != null) {
    return 'AI Assist 無法產生可用的工作區識別。';
  }
  final retryAgentRequiresUpdate = RegExp(
    r'^(?:Unsupported operation: )?Update Alera on this host before retrying agent launch safely\.$',
  ).firstMatch(source);
  if (retryAgentRequiresUpdate != null) {
    return '請先更新此 Host 上的 Alera，再安全重試啟動 Agent。';
  }
  final originalAgentLaunchUnavailable = RegExp(
    r'^(?:Bad state: )?The original agent launch identity is unavailable\.$',
  ).firstMatch(source);
  if (originalAgentLaunchUnavailable != null) {
    return '原始 Agent 啟動識別無法使用。';
  }

  final globalValue = RegExp(r'^Global \((.+)\)$').firstMatch(source);
  if (globalValue != null) {
    final value = globalValue.group(1)!;
    final translatedValue =
        AleraLocalizations._traditionalChinese[value] ?? value;
    return '全域（$translatedValue）';
  }
  final runningTextAction = RegExp(r'^Running (.+)\.$').firstMatch(source);
  if (runningTextAction != null) {
    return '正在執行「${runningTextAction.group(1)}」。';
  }
  final textActionFailed = RegExp(r'^Text action failed: (.+)$')
      .firstMatch(source);
  if (textActionFailed != null) {
    return '文字操作失敗：${textActionFailed.group(1)}';
  }

  final editorAvailable = RegExp(
    r'^(Zed|VS Code|Visual Studio Code) is available(?:: (.+))?\.?$',
  ).firstMatch(source);
  if (editorAvailable != null) {
    final version = editorAvailable.group(2);
    return version == null
        ? '${editorAvailable.group(1)} 可使用。'
        : '${editorAvailable.group(1)} 可使用：$version';
  }
  final editorUnavailable = RegExp(
    r'^(Zed|VS Code|Visual Studio Code) is not available\.$',
  ).firstMatch(source);
  if (editorUnavailable != null) {
    return '${editorUnavailable.group(1)} 無法使用。';
  }
  final editorNotFound = RegExp(
    r'^(.+) was not found\. Check Settings > Editor\.$',
  ).firstMatch(source);
  if (editorNotFound != null) {
    return '找不到 ${editorNotFound.group(1)}。請檢查設定 > 編輯器。';
  }
  final editorVersionFailed = RegExp(
    r'^(.+) was found but its version check failed\.$',
  ).firstMatch(source);
  if (editorVersionFailed != null) {
    return '找到 ${editorVersionFailed.group(1)}，但版本檢查失敗。';
  }
  final editorNotStartable = RegExp(
    r'^(.+) could not be started\. Check Settings > Editor\.$',
  ).firstMatch(source);
  if (editorNotStartable != null) {
    return '無法啟動 ${editorNotStartable.group(1)}。請檢查設定 > 編輯器。';
  }
  final editorStartFailed = RegExp(
    r'^Could not start (.+)\. Configure the executable in Settings > Editor\.$',
  ).firstMatch(source);
  if (editorStartFailed != null) {
    return '無法啟動 ${editorStartFailed.group(1)}。請在設定 > 編輯器中設定執行檔。';
  }
  final openPathFailed = RegExp(
    r'^Could not open the requested path in (.+)\. Check Settings > Editor and try again\.$',
  ).firstMatch(source);
  if (openPathFailed != null) {
    return '無法在 ${openPathFailed.group(1)} 中開啟要求的路徑。請檢查設定 > 編輯器後再試。';
  }
  final openTargetMissing = RegExp(
    r'^The requested (.+) file target does not exist\.$',
  ).firstMatch(source);
  if (openTargetMissing != null) {
    return '要求的 ${openTargetMissing.group(1)} 檔案目標不存在。';
  }
  final openTargetOutside = RegExp(
    r'^The requested (.+) file target is outside the active Alera workspace\.$',
  ).firstMatch(source);
  if (openTargetOutside != null) {
    return '要求的 ${openTargetOutside.group(1)} 檔案目標不在使用中的 Alera 工作區內。';
  }
  final openTargetsInvalid = RegExp(
    r'^(?:The|A) requested (.+) file targets? (?:are invalid|is missing or outside the active Alera workspace)\.$',
  ).firstMatch(source);
  if (openTargetsInvalid != null) {
    return '要求的 ${openTargetsInvalid.group(1)} 檔案目標無效或不在使用中的 Alera 工作區內。';
  }
  final editorPositionInvalid = RegExp(
    r'^(.+) line and column values must be positive\.$',
  ).firstMatch(source);
  if (editorPositionInvalid != null) {
    return '${editorPositionInvalid.group(1)} 的行與欄值必須為正數。';
  }
  final openInEditor = RegExp(r'^Open in (.+)$').firstMatch(source);
  if (openInEditor != null) {
    return '在 ${openInEditor.group(1)} 中開啟';
  }
  final openChangesInEditor = RegExp(r'^Open Changes in (.+)$')
      .firstMatch(source);
  if (openChangesInEditor != null) {
    return '在 ${openChangesInEditor.group(1)} 中開啟變更';
  }
  final couldNotOpenIn = RegExp(
    r'^Could not open (file|item|changed files|workspace) in (.+)\.$',
  ).firstMatch(source);
  if (couldNotOpenIn != null) {
    final noun = switch (couldNotOpenIn.group(1)) {
      'file' => '檔案',
      'item' => '項目',
      'changed files' => '變更檔案',
      _ => '工作區',
    };
    return '無法在 ${couldNotOpenIn.group(2)} 中開啟$noun。';
  }
  final workspaceCreatedNoEditor = RegExp(
    r'^Workspace created, but (.+) could not be opened\.$',
  ).firstMatch(source);
  if (workspaceCreatedNoEditor != null) {
    return '工作區已建立，但無法開啟 ${workspaceCreatedNoEditor.group(1)}。';
  }
  final customExecutable = RegExp(r'^Custom (.+) Executable$')
      .firstMatch(source);
  if (customExecutable != null) {
    return '自訂 ${customExecutable.group(1)} 執行檔';
  }
  final commandEnvironment = RegExp(
    r'^Off uses the (.+) command from the local command environment\.$',
  ).firstMatch(source);
  if (commandEnvironment != null) {
    return '關閉時會使用本機命令環境中的 ${commandEnvironment.group(1)} 指令。';
  }
  final executableTitle = RegExp(r'^(Zed|VS Code) Executable$')
      .firstMatch(source);
  if (executableTitle != null) {
    return '${executableTitle.group(1)} 執行檔';
  }
  final executablePathDescription = RegExp(
    r'^Full path to the (.+) executable on this machine\.$',
  ).firstMatch(source);
  if (executablePathDescription != null) {
    return '此機器上 ${executablePathDescription.group(1)} 執行檔的完整路徑。';
  }
  final executablePathHint = RegExp(r'^Path to (.+) or its executable$')
      .firstMatch(source);
  if (executablePathHint != null) {
    return '${executablePathHint.group(1)} 或其執行檔的路徑';
  }
  final newWindowPerWorktree = RegExp(
    r'^Open each Alera worktree as a separate (.+) window instead of reusing the last one\.$',
  ).firstMatch(source);
  if (newWindowPerWorktree != null) {
    return '將每個 Alera worktree 開啟為獨立的 ${newWindowPerWorktree.group(1)} 視窗，而非沿用上一個視窗。';
  }
  final autoOpenInEditor = RegExp(
    r'^After Alera creates a linked workspace, open that workspace in (.+) automatically\.$',
  ).firstMatch(source);
  if (autoOpenInEditor != null) {
    return 'Alera 建立連結工作區後，自動在 ${autoOpenInEditor.group(1)} 中開啟該工作區。';
  }
  final checkEditor = RegExp(r'^Check (Zed|VS Code)$').firstMatch(source);
  if (checkEditor != null) {
    return '檢查 ${checkEditor.group(1)}';
  }

  final selectedValue = RegExp(r'^Selected: (.+)$').firstMatch(source);
  if (selectedValue != null) {
    return '已選取：${selectedValue.group(1)}';
  }
  final showingCount = RegExp(r'^Showing (\d+) of (\d+)$').firstMatch(source);
  if (showingCount != null) {
    return '顯示 ${showingCount.group(1)} / ${showingCount.group(2)}';
  }

  final interfaceLabel = RegExp(r'^Interface \((.+)\)$').firstMatch(source);
  if (interfaceLabel != null) {
    return '網路介面（${interfaceLabel.group(1)}）';
  }
  final expiresMinutesSeconds = RegExp(r'^Expires in (\d+)m (\d+)s$')
      .firstMatch(source);
  if (expiresMinutesSeconds != null) {
    return '${expiresMinutesSeconds.group(1)} 分 ${expiresMinutesSeconds.group(2)} 秒後到期';
  }
  final expiresMinutes = RegExp(r'^Expires in (\d+)m$').firstMatch(source);
  if (expiresMinutes != null) {
    return '${expiresMinutes.group(1)} 分鐘後到期';
  }
  final expiresSeconds = RegExp(r'^Expires in (\d+)s$').firstMatch(source);
  if (expiresSeconds != null) {
    return '${expiresSeconds.group(1)} 秒後到期';
  }
  final mobileTimestamp = RegExp(r'^(Revoked|Paired|Last seen) (.+)$')
      .firstMatch(source);
  if (mobileTimestamp != null) {
    final label = switch (mobileTimestamp.group(1)) {
      'Revoked' => '已撤銷',
      'Paired' => '已配對',
      _ => '最後上線',
    };
    return '$label ${mobileTimestamp.group(2)}';
  }
  if (source == 'Connected through relay') {
    return '透過 Relay 連線';
  }
  final versionLabel = RegExp(
    r'^(Current version|Update version) (.+?)(?: \(build (.+)\))?$',
  ).firstMatch(source);
  if (versionLabel != null) {
    final prefix = versionLabel.group(1) == 'Current version' ? '目前版本' : '更新版本';
    final build = versionLabel.group(3);
    return build == null
        ? '$prefix ${versionLabel.group(2)}'
        : '$prefix ${versionLabel.group(2)}（Build $build）';
  }
  final modelPassedTo = RegExp(r'^Model passed to (.+)\.$').firstMatch(source);
  if (modelPassedTo != null) {
    return '傳給 ${modelPassedTo.group(1)} 的模型。';
  }
  final globalSettingValue = RegExp(r'^Global \((.+)\)$').firstMatch(source);
  if (globalSettingValue != null) {
    return '全域（${globalSettingValue.group(1)}）';
  }
  final usageLabel = RegExp(r'^Usage: (.+)$').firstMatch(source);
  if (usageLabel != null) {
    return 'Usage：${usageLabel.group(1)}';
  }
  final moveEarlier = RegExp(r'^Move (.+) Earlier$').firstMatch(source);
  if (moveEarlier != null) {
    return '將 ${moveEarlier.group(1)} 往前移';
  }
  final moveLater = RegExp(r'^Move (.+) Later$').firstMatch(source);
  if (moveLater != null) {
    return '將 ${moveLater.group(1)} 往後移';
  }
  final downloadingUpdate = RegExp(r'^Downloading update (.+)\.$')
      .firstMatch(source);
  if (downloadingUpdate != null) {
    return '正在下載更新 ${downloadingUpdate.group(1)}。';
  }
  final installingUpdate = RegExp(
    r'^Installing update (.+)\. Alera will restart\.$',
  ).firstMatch(source);
  if (installingUpdate != null) {
    return '正在安裝更新 ${installingUpdate.group(1)}。Alera 將重新啟動。';
  }
  final updateReady = RegExp(r'^Update (.+) is ready to install\.$')
      .firstMatch(source);
  if (updateReady != null) {
    return '更新 ${updateReady.group(1)} 已可安裝。';
  }
  final upgradingThrough = RegExp(
    r'^Upgrading through (.+)\. Alera will close and reopen\.$',
  ).firstMatch(source);
  if (upgradingThrough != null) {
    return '正在透過 ${upgradingThrough.group(1)} 更新。Alera 將關閉後重新開啟。';
  }
  final packageManagerClose = RegExp(
    r'^Alera will close, let (.+) install the update, and open again\.$',
  ).firstMatch(source);
  if (packageManagerClose != null) {
    return 'Alera 將關閉，交由 ${packageManagerClose.group(1)} 安裝更新後再重新開啟。';
  }
  final updateInstallFailed = RegExp(r'^Update installation failed: (.+)$')
      .firstMatch(source);
  if (updateInstallFailed != null) {
    return '更新安裝失敗：${updateInstallFailed.group(1)}';
  }
  final packageUpgradeFailed = RegExp(
    r'^The (.+) upgrade could not be started: (.+)$',
  ).firstMatch(source);
  if (packageUpgradeFailed != null) {
    return '無法啟動 ${packageUpgradeFailed.group(1)} 更新：${packageUpgradeFailed.group(2)}';
  }
  final restartFailed = RegExp(r'^Alera could not restart: (.+)$')
      .firstMatch(source);
  if (restartFailed != null) {
    return 'Alera 無法重新啟動：${restartFailed.group(1)}';
  }
  final desktopUpdateUnavailable = RegExp(
    r'^Desktop updates are not available on (.+)\.$',
  ).firstMatch(source);
  if (desktopUpdateUnavailable != null) {
    return '${desktopUpdateUnavailable.group(1)} 目前不支援桌面版自動更新。';
  }

  return null;
}

String _translateRuntimeBusyItems(String source) {
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
