part of 'workspace_git_diff_panel.dart';

class const _CommitMessageField({
  required final TextEditingController controller,
  required final FocusNode focusNode,
  required final bool enabled,
  required final bool generating,
  required final ValueChanged<String> onChanged,
  required final ValueChanged<String> onSubmitted,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final field = TextField(
      key: const ValueKey<String>('source-control-message-field'),
      controller: controller,
      focusNode: focusNode,
      contextMenuBuilder: AleraTextActionsScope.buildContextMenu,
      enabled: enabled && !generating,
      minLines: 3,
      maxLines: 6,
      onChanged: onChanged,
      onSubmitted: onSubmitted,
      style: theme.textTheme.bodySmall?.copyWith(color: AleraTokens.foreground),
      cursorColor: AleraTokens.foreground,
      decoration: InputDecoration(
        isDense: true,
        filled: true,
        fillColor: AleraTokens.surface,
        hintText: 'Message',
        hintStyle: theme.textTheme.bodySmall?.copyWith(
          color: AleraTokens.foregroundFaint,
        ),
        contentPadding: const EdgeInsets.fromLTRB(
          AleraTokens.space8,
          AleraTokens.space16,
          AleraTokens.space48,
          AleraTokens.space8,
        ),
        border: _messageBorder(AleraTokens.borderSubtle),
        enabledBorder: _messageBorder(AleraTokens.borderSubtle),
        focusedBorder: _messageBorder(AleraTokens.border),
      ),
    );
    if (!generating) {
      return AiDictationFieldOverlay(
        controller: controller,
        focusNode: focusNode,
        initialPrompt:
            'The user is writing a Git commit message for staged changes.',
        controlKey: const ValueKey<String>('source-control-dictation-control'),
        enabled: enabled,
        child: field,
      );
    }
    return Stack(
      children: <Widget>[
        field,
        Positioned.fill(
          child: AbsorbPointer(
            child: DecoratedBox(
              decoration: BoxDecoration(
                color: AleraTokens.barrierDark,
                borderRadius: BorderRadius.circular(AleraTokens.radiusLg),
                border: Border.all(color: AleraTokens.borderSubtle),
              ),
              child: Center(
                child: DecoratedBox(
                  decoration: BoxDecoration(
                    color: AleraTokens.surfaceElevated,
                    borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
                    border: Border.all(color: AleraTokens.border),
                  ),
                  child: Padding(
                    padding: const EdgeInsets.symmetric(
                      horizontal: AleraTokens.space12,
                      vertical: AleraTokens.space8,
                    ),
                    child: Row(
                      mainAxisSize: .min,
                      children: <Widget>[
                        const SizedBox(
                          width: 14,
                          height: 14,
                          child: CircularProgressIndicator(
                            strokeWidth: 2,
                            color: AleraTokens.foregroundMuted,
                          ),
                        ),
                        const SizedBox(width: AleraTokens.space8),
                        Text(
                          'Generating with AI',
                          style: theme.textTheme.labelMedium?.copyWith(
                            color: AleraTokens.foregroundMuted,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  OutlineInputBorder _messageBorder(Color color) => OutlineInputBorder(
    borderRadius: .circular(AleraTokens.radiusLg),
    borderSide: BorderSide(color: color),
  );
}

extension _WorkspaceGitDiffPanelCommitActions on _WorkspaceGitDiffPanelState {
  Future<void> _commitAction(_SourceControlMenuAction action) async {
    final message = _messageController.text.trim();
    if (message.isEmpty) {
      return;
    }
    final committed = await switch (action) {
      _SourceControlMenuAction.commit => _run(
        () => _notifier.commit(message),
        successMessage: 'Committed',
      ),
      _SourceControlMenuAction.commitPush => _run(
        () => _notifier.commitAndPush(message),
        successMessage: 'Committed and pushed',
      ),
      _SourceControlMenuAction.commitSync => _run(
        () => _notifier.commitAndSync(message),
        successMessage: 'Committed and synced',
      ),
      _ => Future<bool>.value(false),
    };
    if (committed && mounted) {
      _messageController.clear();
      _setPanelState(() {});
    }
  }

  Future<void> _generateCommitMessage() async {
    final state = ref
        .read(
          workspaceSourceControlControllerProvider(
            widget.sourceControlScope.path,
          ),
        )
        .asData
        ?.value;
    final settings = ref.read(settingsControllerProvider).aiAssist;
    if (_generatingCommitMessage ||
        state == null ||
        !settings.enabled ||
        !state.hasStagedChanges ||
        state.repositoryState.hasConflicts ||
        state.isBusy) {
      return;
    }
    final requestWorkspacePath = widget.sourceControlScope.path;
    final generationId = _commitMessageGenerationId + 1;
    final initialText = _messageController.text;
    _setPanelState(() {
      _commitMessageGenerationId = generationId;
      _generatingCommitMessage = true;
    });
    try {
      final result = await ref
          .read(aiAssistServiceProvider)
          .generate(
            AiAssistRequest(
              operation: .commitMessage,
              workspacePath: requestWorkspacePath,
              settings: settings,
            ),
          );
      if (!mounted) {
        return;
      }
      if (!_isCurrentCommitMessageGeneration(
        workspacePath: requestWorkspacePath,
        generationId: generationId,
      )) {
        return;
      }
      if (_messageController.text == initialText) {
        _messageController.text = result.text;
        _messageController.selection = TextSelection.collapsed(
          offset: _messageController.text.length,
        );
        _setPanelState(() {});
        AleraToast.show(
          context,
          message: 'Commit message generated with ${result.agentLabel}',
          tone: .success,
        );
      } else {
        AleraToast.show(
          context,
          message:
              'Generated message was not applied because the field changed.',
          tone: .info,
        );
      }
    } on AiAssistCanceledException {
      return;
    } catch (error) {
      if (_isCurrentCommitMessageGeneration(
        workspacePath: requestWorkspacePath,
        generationId: generationId,
      )) {
        AleraToast.show(context, message: _messageFor(error), tone: .error);
      }
    } finally {
      if (_isCurrentCommitMessageGeneration(
        workspacePath: requestWorkspacePath,
        generationId: generationId,
      )) {
        _setPanelState(() => _generatingCommitMessage = false);
      }
    }
  }

  bool _isCurrentCommitMessageGeneration({
    required String workspacePath,
    required int generationId,
  }) {
    return mounted &&
        widget.sourceControlScope.path == workspacePath &&
        _commitMessageGenerationId == generationId;
  }

  void _cancelGenerateCommitMessage() {
    _aiAssistService.cancel(widget.sourceControlScope.path, .commitMessage);
  }

  Future<void> _amendAction() async {
    final state = ref
        .read(
          workspaceSourceControlControllerProvider(
            widget.sourceControlScope.path,
          ),
        )
        .asData
        ?.value;
    final initialMessage = state?.repositoryState.headMessage;
    if (initialMessage == null || initialMessage.trim().isEmpty) {
      return;
    }
    final message = await showDialog<String>(
      context: context,
      builder: (_) => _AmendCommitDialog(initialMessage: initialMessage),
    );
    if (message == null || !mounted) {
      return;
    }
    await _run(
      () => _notifier.amendCommit(message),
      successMessage: 'Commit amended',
    );
  }

  bool _actionRequiresMessage(_SourceControlMenuAction action) {
    return switch (action) {
      _SourceControlMenuAction.commit ||
      _SourceControlMenuAction.commitPush ||
      _SourceControlMenuAction.commitSync => true,
      _ => false,
    };
  }
}
