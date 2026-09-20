part of 'alera_localizations.dart';

// Pattern-based Simplified Chinese translations for strings that embed runtime
// values. Lookup order: static table first, then these rules. Mirrors the rule
// set in `_translateDynamicTraditionalChinese`.
String? _translateDynamicSimplifiedChinese(String source) {
  final sleepProject = RegExp(
    r'^This closes all tabs and terminal sessions for all (\d+) workspaces in "(.+)"\. Worktrees, branches, and files will be preserved\.(.*)$',
  ).firstMatch(source);
  if (sleepProject != null) {
    final suffix = sleepProject.group(3)!;
    var dirtyWarning = '';
    if (suffix == ' One editor has unsaved changes that will be discarded.') {
      dirtyWarning = ' 有 1 个编辑器包含尚未保存的变更，将会丢弃。';
    } else {
      final dirtyEditors = RegExp(
        r'^ (\d+) editors have unsaved changes that will be discarded\.$',
      ).firstMatch(suffix);
      if (dirtyEditors != null) {
        dirtyWarning = ' 有 ${dirtyEditors.group(1)} 个编辑器包含尚未保存的变更，将会丢弃。';
      }
    }
    return '这会关闭“${sleepProject.group(2)}”中全部 ${sleepProject.group(1)} 个工作区的标签页与终端会话。Worktree、分支与文件会保留。$dirtyWarning';
  }
  final sleepProjectFailed = RegExp(r'^Could not sleep project: (.+)$')
      .firstMatch(source);
  if (sleepProjectFailed != null) {
    return '无法让项目休眠：${sleepProjectFailed.group(1)}';
  }
  final diffOverview = RegExp(r'^Diff overview, (\d+) removed, (\d+) added$')
      .firstMatch(source);
  if (diffOverview != null) {
    return '差异概览，移除 ${diffOverview.group(1)} 行，新增 ${diffOverview.group(2)} 行';
  }
  final requiredField = RegExp(r'^(.+) is required$').firstMatch(source);
  if (requiredField != null) {
    final label = AleraLocalizations._lookupSimplifiedChinese(
      requiredField.group(1)!,
    );
    return '$label 为必填。';
  }
  final agentGroupDescription = RegExp(
    r'^(.+) · (Waiting for input|Blocked|Interrupted|Done \(Unread\)|Done)$',
  ).firstMatch(source);
  if (agentGroupDescription != null) {
    final state = AleraLocalizations._lookupSimplifiedChinese(
      agentGroupDescription.group(2)!,
    );
    return '${agentGroupDescription.group(1)} · $state';
  }
  final rebaseCurrentBranchOnto = RegExp(r'^Rebase Current Branch onto (.+)$')
      .firstMatch(source);
  if (rebaseCurrentBranchOnto != null) {
    return '将当前分支 Rebase 到 ${rebaseCurrentBranchOnto.group(1)}';
  }
  final autosavePaused = RegExp(r'^Autosave paused: (.+)$').firstMatch(source);
  if (autosavePaused != null) {
    final detail = AleraLocalizations._lookupSimplifiedChinese(
      autosavePaused.group(1)!,
    );
    return '自动保存已暂停：$detail';
  }
  final dropCommitMessage = RegExp(
    r'^Removes (.+) "(.+)" from the current branch\. This cannot be undone\.$',
  ).firstMatch(source);
  if (dropCommitMessage != null) {
    return '这会从当前分支移除 ${dropCommitMessage.group(1)}“${dropCommitMessage.group(2)}”，且无法撤销。';
  }
  final mergeCommitMessage = RegExp(r'^Merge (.+) into (.+)\?$')
      .firstMatch(source);
  if (mergeCommitMessage != null) {
    return '要将 ${mergeCommitMessage.group(1)} 合并到 ${mergeCommitMessage.group(2)} 吗？';
  }
  final rebaseCommitMessage = RegExp(
    r'^Rebase (.+) onto (.+) "(.+)"\. This rewrites the current branch history\.$',
  ).firstMatch(source);
  if (rebaseCommitMessage != null) {
    return '要将 ${rebaseCommitMessage.group(1)} Rebase 到 ${rebaseCommitMessage.group(2)}“${rebaseCommitMessage.group(3)}”。这会重写当前分支的历史记录。';
  }
  final singleDirty = RegExp(r'^(.+) has unsaved changes\.$')
      .firstMatch(source);
  if (singleDirty != null) {
    return '“${singleDirty.group(1)}”有尚未保存的变更。';
  }
  final multipleDirty = RegExp(r'^(\d+) editor tabs have unsaved changes\.$')
      .firstMatch(source);
  if (multipleDirty != null) {
    return '有 ${multipleDirty.group(1)} 个编辑器标签页包含尚未保存的变更。';
  }
  final activeAgent = RegExp(
    r'^An agent is actively working in "(.+)"\. Closing will terminate the session\.$',
  ).firstMatch(source);
  if (activeAgent != null) {
    return '代理正在“${activeAgent.group(1)}”中执行工作。关闭后将终止此会话。';
  }
  final runningProcess = RegExp(
    r'^The process "(.+)" is still running in "(.+)"\. Closing will terminate it\.$',
  ).firstMatch(source);
  if (runningProcess != null) {
    return '进程“${runningProcess.group(1)}”仍在“${runningProcess.group(2)}”中运行。关闭后将终止该进程。';
  }
  final runningCommand = RegExp(
    r'^A command is still running in "(.+)"\. Closing will terminate it\.$',
  ).firstMatch(source);
  if (runningCommand != null) {
    return '“${runningCommand.group(1)}”中仍有命令正在运行。关闭后将终止该命令。';
  }
  final busyTerminals = RegExp(
    r'^(\d+) terminal tabs have running processes or active agents\. Closing will terminate them\.$',
  ).firstMatch(source);
  if (busyTerminals != null) {
    return '有 ${busyTerminals.group(1)} 个终端标签页仍有运行中的进程或活动的代理。关闭后将终止它们。';
  }
  final preservedAttachments = RegExp(
    r'^(\d+) attached (?:item|items) will be preserved\.$',
  ).firstMatch(source);
  if (preservedAttachments != null) {
    return '将保留 ${preservedAttachments.group(1)} 个附件。';
  }

  final effortLabel = RegExp(r'^(.+) Effort$').firstMatch(source);
  if (effortLabel != null) {
    return '推理强度：${effortLabel.group(1)}';
  }
  final chunkCount = RegExp(r'^(\d+) (?:Chunk|Chunks)$').firstMatch(source);
  if (chunkCount != null) {
    return '${chunkCount.group(1)} 个区块';
  }
  final retainedLines = RegExp(r'^Kept (\d+)/(\d+) Changed Lines$')
      .firstMatch(source);
  if (retainedLines != null) {
    return '保留 ${retainedLines.group(1)}/${retainedLines.group(2)} 行变更';
  }
  final chunkPosition = RegExp(r'^Chunk (\d+) of (\d+)$').firstMatch(source);
  if (chunkPosition != null) {
    return '区块 ${chunkPosition.group(1)}/${chunkPosition.group(2)}';
  }
  final generatingChunk = RegExp(r'^Generating chunk (\d+) of (\d+)$')
      .firstMatch(source);
  if (generatingChunk != null) {
    return '正在生成区块 ${generatingChunk.group(1)}/${generatingChunk.group(2)}';
  }
  final repairingChunk = RegExp(r'^Repairing chunk (\d+) of (\d+)$')
      .firstMatch(source);
  if (repairingChunk != null) {
    return '正在修复区块 ${repairingChunk.group(1)}/${repairingChunk.group(2)}';
  }
  final combiningChunks = RegExp(r'^Combining (\d+) (?:chunk|chunks)$')
      .firstMatch(source);
  if (combiningChunks != null) {
    return '正在合并 ${combiningChunks.group(1)} 个区块';
  }

  final shortcutConflict = RegExp(
    r'^(.+) is assigned to "(.+)"\. Reassign it to "(.+)"\?$',
  ).firstMatch(source);
  if (shortcutConflict != null) {
    final conflictLabel = AleraLocalizations._lookupSimplifiedChinese(
      shortcutConflict.group(2)!,
    );
    final targetLabel = AleraLocalizations._lookupSimplifiedChinese(
      shortcutConflict.group(3)!,
    );
    return '${shortcutConflict.group(1)} 已分配给“$conflictLabel”。要重新分配给“$targetLabel”吗？';
  }
  final unsupportedKey = RegExp(r'^Unsupported key: (.+)\.$')
      .firstMatch(source);
  if (unsupportedKey != null) {
    return '不支持的按键：${unsupportedKey.group(1)}。';
  }

  final sortBy = RegExp(r'^Sort By (.+)$').firstMatch(source);
  if (sortBy != null) {
    final label = AleraLocalizations._lookupSimplifiedChinese(sortBy.group(1)!);
    return '按 $label 排序';
  }
  final orphanTerminals = RegExp(r'^(\d+) orphan terminal(?:s)?$')
      .firstMatch(source);
  if (orphanTerminals != null) {
    return '${orphanTerminals.group(1)} 个孤立终端';
  }
  final forceQuitTerminal = RegExp(
    r'^Force-quits (.+)\. Anything running in that terminal is lost\.$',
  ).firstMatch(source);
  if (forceQuitTerminal != null) {
    return '将强制关闭“${forceQuitTerminal.group(1)}”。该终端中正在运行的所有工作都会丢失。';
  }

  final usageCount = RegExp(
    r'^(\d+) (assistant responses|transcript sources|unpriced responses)$',
  ).firstMatch(source);
  if (usageCount != null) {
    final noun = switch (usageCount.group(2)) {
      'assistant responses' => '条 Assistant 回复',
      'transcript sources' => '个转录来源',
      _ => '条未计价回复',
    };
    return '${usageCount.group(1)} $noun';
  }
  final usageInputShare = RegExp(r'^([0-9.]+%) of input$').firstMatch(source);
  if (usageInputShare != null) {
    return '占输入 ${usageInputShare.group(1)}';
  }
  final usageScanSummary = RegExp(
    r'^Scanned (\d+) files in (\d+) ms\. Transcript content stays on this host\.$',
  ).firstMatch(source);
  if (usageScanSummary != null) {
    return '已扫描 ${usageScanSummary.group(1)} 个文件，耗时 ${usageScanSummary.group(2)} ms。转录内容会保留在此 Host。';
  }
  final usagePartial = RegExp(r'^(.+) (.+) is partial\.$').firstMatch(source);
  if (usagePartial != null) {
    return '${usagePartial.group(1)} ${usagePartial.group(2)} 的数据不完整。';
  }
  final dailyUsageSemantics = RegExp(
    r'^Daily Claude Code, Codex, and Grok Build token usage\. (.+)$',
  ).firstMatch(source);
  if (dailyUsageSemantics != null) {
    return 'Claude Code、Codex 与 Grok Build 每日 Token 用量。${dailyUsageSemantics.group(1)}';
  }

  final pullRequestCount = RegExp(r'^(Checks|Comments) \((\d+)\)$')
      .firstMatch(source);
  if (pullRequestCount != null) {
    final label = pullRequestCount.group(1) == 'Checks' ? '检查' : '评论';
    return '$label（${pullRequestCount.group(2)}）';
  }
  final checkGroup = RegExp(r'^(\d+) (failing|in progress|successful) Checks?$')
      .firstMatch(source);
  if (checkGroup != null) {
    final state = switch (checkGroup.group(2)) {
      'failing' => '失败',
      'in progress' => '进行中',
      _ => '成功',
    };
    return '${checkGroup.group(1)} 个$state检查';
  }

  final installProviderCli = RegExp(
    r'^Install `(.+)` and ensure it is on your PATH\.$',
  ).firstMatch(source);
  if (installProviderCli != null) {
    return '请安装 `${installProviderCli.group(1)}`，并确认它位于 PATH 中。';
  }
  final runProviderAuth = RegExp(r'^Run `(.+)` to sign in, then refresh\.$')
      .firstMatch(source);
  if (runProviderAuth != null) {
    return '请执行 `${runProviderAuth.group(1)}` 登录，然后刷新。';
  }

  final accountRuntime = RegExp(r'^Runtime (.+)$').firstMatch(source);
  if (accountRuntime != null) {
    return '运行时 ${accountRuntime.group(1)}';
  }
  final linkIdentityProvider = RegExp(r'^Link (Google|GitHub)$')
      .firstMatch(source);
  if (linkIdentityProvider != null) {
    return '关联 ${linkIdentityProvider.group(1)}';
  }
  final activeMobileSubscriptions = RegExp(
    r'^(\d+) active mobile subscription\(s\)\.$',
  ).firstMatch(source);
  if (activeMobileSubscriptions != null) {
    return '${activeMobileSubscriptions.group(1)} 个活动的移动设备订阅。';
  }
  final transferRuntimeAccount = RegExp(
    r'^Transfer this runtime and its mobile subscriptions to account (.+)\? This installation will sign out\.$',
  ).firstMatch(source);
  if (transferRuntimeAccount != null) {
    return '要将此运行时及其移动设备订阅转移到账号 ${transferRuntimeAccount.group(1)} 吗？此安装将会退出登录。';
  }
  final signInFailure = RegExp(r'^Sign in failed: (.+)$').firstMatch(source);
  if (signInFailure != null) {
    return '登录失败：${signInFailure.group(1)}';
  }

  final resizeMasterList = RegExp(r'^Resize (.+) List$').firstMatch(source);
  if (resizeMasterList != null) {
    final label = AleraLocalizations._lookupSimplifiedChinese(
      resizeMasterList.group(1)!,
    );
    return '调整$label列表大小';
  }

  final allAutomationFilter = RegExp(r'^All (State|Project|Profile|Tag)$')
      .firstMatch(source);
  if (allAutomationFilter != null) {
    final label = AleraLocalizations._lookupSimplifiedChinese(
      allAutomationFilter.group(1)!,
    );
    return '所有$label';
  }

  final unknownPromptVariable = RegExp(r'^Unknown prompt variable: (.+)$')
      .firstMatch(source);
  if (unknownPromptVariable != null) {
    return '未知的提示词变量：${unknownPromptVariable.group(1)}';
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
    return '下载在 ${dictationInterrupted.group(1)} 时中断，可在准备好后继续。';
  }
  final dictationDownloadSize = RegExp(
    r'^Download size (\d+(?:\.\d+)? (?:KiB|MiB))\.$',
  ).firstMatch(source);
  if (dictationDownloadSize != null) {
    return '下载大小 ${dictationDownloadSize.group(1)}。';
  }
  final systemRecognitionFailed = RegExp(
    r'^System speech recognition failed: (.+)$',
  ).firstMatch(source);
  if (systemRecognitionFailed != null) {
    return '系统语音识别失败：${systemRecognitionFailed.group(1)}';
  }
  final systemRecognitionStartFailed = RegExp(
    r'^System speech recognition could not start: (.+)$',
  ).firstMatch(source);
  if (systemRecognitionStartFailed != null) {
    return '无法启动系统语音识别：${systemRecognitionStartFailed.group(1)}';
  }
  final speechProcessingFallback = RegExp(
    r'^The transcript was inserted without speech processing: (.+)$',
  ).firstMatch(source);
  if (speechProcessingFallback != null) {
    return '语音处理失败，已插入原始转录内容：${speechProcessingFallback.group(1)}';
  }

  final originalPath = RegExp(r'^Original \((.+)\)$').firstMatch(source);
  if (originalPath != null) {
    return '原始（${originalPath.group(1)}）';
  }

  final runtimeBusyWithSuffix = RegExp(
    r'^The runtime has (.+)\. (Force stop terminates them\.|You can quit and leave the runtime running, or force stop it\.)$',
  ).firstMatch(source);
  if (runtimeBusyWithSuffix != null) {
    final details = _translateRuntimeBusyItemsSimplifiedChinese(
      runtimeBusyWithSuffix.group(1)!,
    );
    final suffix = runtimeBusyWithSuffix.group(2)!;
    final translatedSuffix = switch (suffix) {
      'Force stop terminates them.' => '强制停止会终止这些工作。',
      _ => '你可以结束 Alera 并让运行时保持运行，或强制停止它。',
    };
    return '运行时当前有 $details。$translatedSuffix';
  }

  final runtimeBusy = RegExp(r'^The runtime has (.+)\.$').firstMatch(source);
  if (runtimeBusy != null) {
    final details = _translateRuntimeBusyItemsSimplifiedChinese(
      runtimeBusy.group(1)!,
    );
    return '运行时当前有 $details。';
  }

  final generatedBranchExists = RegExp(
    r'^(?:Bad state: )?The generated branch "(.+)" already exists\.$',
  ).firstMatch(source);
  if (generatedBranchExists != null) {
    return '生成的 Branch“${generatedBranchExists.group(1)}”已存在。';
  }
  final workspaceIdentityUnavailable = RegExp(
    r'^(?:Bad state: )?AI Assist could not generate an available workspace identity\.$',
  ).firstMatch(source);
  if (workspaceIdentityUnavailable != null) {
    return 'AI Assist 无法生成可用的工作区标识。';
  }
  final retryAgentRequiresUpdate = RegExp(
    r'^(?:Unsupported operation: )?Update Alera on this host before retrying agent launch safely\.$',
  ).firstMatch(source);
  if (retryAgentRequiresUpdate != null) {
    return '请先更新此 Host 上的 Alera，再安全重试启动 Agent。';
  }
  final originalAgentLaunchUnavailable = RegExp(
    r'^(?:Bad state: )?The original agent launch identity is unavailable\.$',
  ).firstMatch(source);
  if (originalAgentLaunchUnavailable != null) {
    return '原始 Agent 启动标识无法使用。';
  }

  final globalValue = RegExp(r'^Global \((.+)\)$').firstMatch(source);
  if (globalValue != null) {
    final value = globalValue.group(1)!;
    final translatedValue = AleraLocalizations._lookupSimplifiedChinese(value);
    return '全局（$translatedValue）';
  }
  final runningTextAction = RegExp(r'^Running (.+)\.$').firstMatch(source);
  if (runningTextAction != null) {
    return '正在执行“${runningTextAction.group(1)}”。';
  }
  final textActionFailed = RegExp(r'^Text action failed: (.+)$')
      .firstMatch(source);
  if (textActionFailed != null) {
    return '文本操作失败：${textActionFailed.group(1)}';
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
    return '${editorUnavailable.group(1)} 无法使用。';
  }
  final editorNotFound = RegExp(
    r'^(.+) was not found\. Check Settings > Editor\.$',
  ).firstMatch(source);
  if (editorNotFound != null) {
    return '找不到 ${editorNotFound.group(1)}。请检查设置 > 编辑器。';
  }
  final editorVersionFailed = RegExp(
    r'^(.+) was found but its version check failed\.$',
  ).firstMatch(source);
  if (editorVersionFailed != null) {
    return '找到 ${editorVersionFailed.group(1)}，但版本检查失败。';
  }
  final editorNotStartable = RegExp(
    r'^(.+) could not be started\. Check Settings > Editor\.$',
  ).firstMatch(source);
  if (editorNotStartable != null) {
    return '无法启动 ${editorNotStartable.group(1)}。请检查设置 > 编辑器。';
  }
  final editorStartFailed = RegExp(
    r'^Could not start (.+)\. Configure the executable in Settings > Editor\.$',
  ).firstMatch(source);
  if (editorStartFailed != null) {
    return '无法启动 ${editorStartFailed.group(1)}。请在设置 > 编辑器中配置可执行文件。';
  }
  final openPathFailed = RegExp(
    r'^Could not open the requested path in (.+)\. Check Settings > Editor and try again\.$',
  ).firstMatch(source);
  if (openPathFailed != null) {
    return '无法在 ${openPathFailed.group(1)} 中打开请求的路径。请检查设置 > 编辑器后再试。';
  }
  final openTargetMissing = RegExp(
    r'^The requested (.+) file target does not exist\.$',
  ).firstMatch(source);
  if (openTargetMissing != null) {
    return '请求的 ${openTargetMissing.group(1)} 文件目标不存在。';
  }
  final openTargetOutside = RegExp(
    r'^The requested (.+) file target is outside the active Alera workspace\.$',
  ).firstMatch(source);
  if (openTargetOutside != null) {
    return '请求的 ${openTargetOutside.group(1)} 文件目标不在当前的 Alera 工作区内。';
  }
  final openTargetsInvalid = RegExp(
    r'^(?:The|A) requested (.+) file targets? (?:are invalid|is missing or outside the active Alera workspace)\.$',
  ).firstMatch(source);
  if (openTargetsInvalid != null) {
    return '请求的 ${openTargetsInvalid.group(1)} 文件目标无效或不在当前的 Alera 工作区内。';
  }
  final editorPositionInvalid = RegExp(
    r'^(.+) line and column values must be positive\.$',
  ).firstMatch(source);
  if (editorPositionInvalid != null) {
    return '${editorPositionInvalid.group(1)} 的行与列值必须为正数。';
  }
  final openInEditor = RegExp(r'^Open in (.+)$').firstMatch(source);
  if (openInEditor != null) {
    return '在 ${openInEditor.group(1)} 中打开';
  }
  final openChangesInEditor = RegExp(r'^Open Changes in (.+)$')
      .firstMatch(source);
  if (openChangesInEditor != null) {
    return '在 ${openChangesInEditor.group(1)} 中打开变更';
  }
  final couldNotOpenIn = RegExp(
    r'^Could not open (file|item|changed files|workspace) in (.+)\.$',
  ).firstMatch(source);
  if (couldNotOpenIn != null) {
    final noun = switch (couldNotOpenIn.group(1)) {
      'file' => '文件',
      'item' => '项目',
      'changed files' => '变更文件',
      _ => '工作区',
    };
    return '无法在 ${couldNotOpenIn.group(2)} 中打开$noun。';
  }
  final workspaceCreatedNoEditor = RegExp(
    r'^Workspace created, but (.+) could not be opened\.$',
  ).firstMatch(source);
  if (workspaceCreatedNoEditor != null) {
    return '工作区已创建，但无法打开 ${workspaceCreatedNoEditor.group(1)}。';
  }
  final customExecutable = RegExp(r'^Custom (.+) Executable$')
      .firstMatch(source);
  if (customExecutable != null) {
    return '自定义 ${customExecutable.group(1)} 可执行文件';
  }
  final commandEnvironment = RegExp(
    r'^Off uses the (.+) command from the local command environment\.$',
  ).firstMatch(source);
  if (commandEnvironment != null) {
    return '关闭时会使用本地命令环境中的 ${commandEnvironment.group(1)} 命令。';
  }
  final executableTitle = RegExp(r'^(Zed|VS Code) Executable$')
      .firstMatch(source);
  if (executableTitle != null) {
    return '${executableTitle.group(1)} 可执行文件';
  }
  final executablePathDescription = RegExp(
    r'^Full path to the (.+) executable on this machine\.$',
  ).firstMatch(source);
  if (executablePathDescription != null) {
    return '此机器上 ${executablePathDescription.group(1)} 可执行文件的完整路径。';
  }
  final executablePathHint = RegExp(r'^Path to (.+) or its executable$')
      .firstMatch(source);
  if (executablePathHint != null) {
    return '${executablePathHint.group(1)} 或其可执行文件的路径';
  }
  final newWindowPerWorktree = RegExp(
    r'^Open each Alera worktree as a separate (.+) window instead of reusing the last one\.$',
  ).firstMatch(source);
  if (newWindowPerWorktree != null) {
    return '将每个 Alera worktree 打开为独立的 ${newWindowPerWorktree.group(1)} 窗口，而不是复用上一个窗口。';
  }
  final autoOpenInEditor = RegExp(
    r'^After Alera creates a linked workspace, open that workspace in (.+) automatically\.$',
  ).firstMatch(source);
  if (autoOpenInEditor != null) {
    return 'Alera 创建关联工作区后，自动在 ${autoOpenInEditor.group(1)} 中打开该工作区。';
  }
  final checkEditor = RegExp(r'^Check (Zed|VS Code)$').firstMatch(source);
  if (checkEditor != null) {
    return '检查 ${checkEditor.group(1)}';
  }

  final selectedValue = RegExp(r'^Selected: (.+)$').firstMatch(source);
  if (selectedValue != null) {
    return '已选择：${selectedValue.group(1)}';
  }
  final showingCount = RegExp(r'^Showing (\d+) of (\d+)$').firstMatch(source);
  if (showingCount != null) {
    return '显示 ${showingCount.group(1)} / ${showingCount.group(2)}';
  }

  final interfaceLabel = RegExp(r'^Interface \((.+)\)$').firstMatch(source);
  if (interfaceLabel != null) {
    return '网络接口（${interfaceLabel.group(1)}）';
  }
  final expiresMinutesSeconds = RegExp(r'^Expires in (\d+)m (\d+)s$')
      .firstMatch(source);
  if (expiresMinutesSeconds != null) {
    return '${expiresMinutesSeconds.group(1)} 分 ${expiresMinutesSeconds.group(2)} 秒后到期';
  }
  final expiresMinutes = RegExp(r'^Expires in (\d+)m$').firstMatch(source);
  if (expiresMinutes != null) {
    return '${expiresMinutes.group(1)} 分钟后到期';
  }
  final expiresSeconds = RegExp(r'^Expires in (\d+)s$').firstMatch(source);
  if (expiresSeconds != null) {
    return '${expiresSeconds.group(1)} 秒后到期';
  }
  final mobileTimestamp = RegExp(r'^(Revoked|Paired|Last seen) (.+)$')
      .firstMatch(source);
  if (mobileTimestamp != null) {
    final label = switch (mobileTimestamp.group(1)) {
      'Revoked' => '已撤销',
      'Paired' => '已配对',
      _ => '最后在线',
    };
    return '$label ${mobileTimestamp.group(2)}';
  }
  if (source == 'Connected through relay') {
    return '通过 Relay 连接';
  }
  final versionLabel = RegExp(
    r'^(Current version|Update version) (.+?)(?: \(build (.+)\))?$',
  ).firstMatch(source);
  if (versionLabel != null) {
    final prefix = versionLabel.group(1) == 'Current version' ? '当前版本' : '更新版本';
    final build = versionLabel.group(3);
    return build == null
        ? '$prefix ${versionLabel.group(2)}'
        : '$prefix ${versionLabel.group(2)}（Build $build）';
  }
  final modelPassedTo = RegExp(r'^Model passed to (.+)\.$').firstMatch(source);
  if (modelPassedTo != null) {
    return '传给 ${modelPassedTo.group(1)} 的模型。';
  }
  final usageLabel = RegExp(r'^Usage: (.+)$').firstMatch(source);
  if (usageLabel != null) {
    return 'Usage：${usageLabel.group(1)}';
  }
  final moveEarlier = RegExp(r'^Move (.+) Earlier$').firstMatch(source);
  if (moveEarlier != null) {
    return '将 ${moveEarlier.group(1)} 往前移';
  }
  final moveLater = RegExp(r'^Move (.+) Later$').firstMatch(source);
  if (moveLater != null) {
    return '将 ${moveLater.group(1)} 往后移';
  }
  final downloadingUpdate = RegExp(r'^Downloading update (.+)\.$')
      .firstMatch(source);
  if (downloadingUpdate != null) {
    return '正在下载更新 ${downloadingUpdate.group(1)}。';
  }
  final installingUpdate = RegExp(
    r'^Installing update (.+)\. Alera will restart\.$',
  ).firstMatch(source);
  if (installingUpdate != null) {
    return '正在安装更新 ${installingUpdate.group(1)}。Alera 将重新启动。';
  }
  final updateReady = RegExp(r'^Update (.+) is ready to install\.$')
      .firstMatch(source);
  if (updateReady != null) {
    return '更新 ${updateReady.group(1)} 已可安装。';
  }
  final upgradingThrough = RegExp(
    r'^Upgrading through (.+)\. Alera will close and reopen\.$',
  ).firstMatch(source);
  if (upgradingThrough != null) {
    return '正在通过 ${upgradingThrough.group(1)} 更新。Alera 将关闭后重新打开。';
  }
  final packageManagerClose = RegExp(
    r'^Alera will close, let (.+) install the update, and open again\.$',
  ).firstMatch(source);
  if (packageManagerClose != null) {
    return 'Alera 将关闭，交由 ${packageManagerClose.group(1)} 安装更新后再重新打开。';
  }
  final updateInstallFailed = RegExp(r'^Update installation failed: (.+)$')
      .firstMatch(source);
  if (updateInstallFailed != null) {
    return '更新安装失败：${updateInstallFailed.group(1)}';
  }
  final packageUpgradeFailed = RegExp(
    r'^The (.+) upgrade could not be started: (.+)$',
  ).firstMatch(source);
  if (packageUpgradeFailed != null) {
    return '无法启动 ${packageUpgradeFailed.group(1)} 更新：${packageUpgradeFailed.group(2)}';
  }
  final restartFailed = RegExp(r'^Alera could not restart: (.+)$')
      .firstMatch(source);
  if (restartFailed != null) {
    return 'Alera 无法重新启动：${restartFailed.group(1)}';
  }
  final desktopUpdateUnavailable = RegExp(
    r'^Desktop updates are not available on (.+)\.$',
  ).firstMatch(source);
  if (desktopUpdateUnavailable != null) {
    return '${desktopUpdateUnavailable.group(1)} 当前不支持桌面版自动更新。';
  }

  return null;
}

String _translateRuntimeBusyItemsSimplifiedChinese(String source) {
  return source
      .replaceAllMapped(
        RegExp(r'(\d+) open agent\(s\)'),
        (match) => '${match.group(1)} 个打开中的代理',
      )
      .replaceAllMapped(
        RegExp(r'(\d+) active terminal session\(s\)'),
        (match) => '${match.group(1)} 个活动的终端会话',
      )
      .replaceAllMapped(
        RegExp(r'(\d+) active background job\(s\)'),
        (match) => '${match.group(1)} 个活动的后台任务',
      )
      .replaceAllMapped(
        RegExp(r'(\d+) active push subscription\(s\)'),
        (match) => '${match.group(1)} 个活动的推送订阅',
      )
      .replaceAll(', and ', '、')
      .replaceAll(' and ', '、')
      .replaceAll(', ', '、');
}
