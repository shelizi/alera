part of 'workspace_git_diff_panel.dart';

typedef _GitChangeSelectionCallback = void Function(
  GitChangeEntry entry,
  List<String> visibleIds, {
  required bool contextMenu,
});

extension _WorkspaceGitDiffPanelSelection on _WorkspaceGitDiffPanelState {
  void _selectChangeEntry(
    GitChangeEntry entry,
    List<String> visibleIds, {
    required bool contextMenu,
  }) {
    final keyboard = HardwareKeyboard.instance;
    _setPanelState(() {
      if (contextMenu) {
        _changeSelection.contextSelect(entry.id);
      } else {
        _changeSelection.tap(
          entry.id,
          visibleIds: visibleIds,
          shift: keyboard.isShiftPressed,
          toggle: keyboard.isControlPressed || keyboard.isMetaPressed,
        );
      }
    });
  }

  List<GitChangeEntry> _selectedChangeEntries(
    WorkspaceSourceControlState? current,
  ) {
    if (current == null || _changeSelection.isEmpty) {
      return const <GitChangeEntry>[];
    }
    return _filteredStatus(current.status).entries
        .where((entry) => _changeSelection.contains(entry.id))
        .toList(growable: false);
  }

  List<GitChangeEntry> _selectedOrEntry(GitChangeEntry entry) {
    if (!_changeSelection.contains(entry.id)) {
      return <GitChangeEntry>[entry];
    }
    final current = ref
        .read(
          workspaceSourceControlControllerProvider(
            widget.sourceControlScope.path,
          ),
        )
        .asData
        ?.value;
    final selected = _selectedChangeEntries(current);
    return selected.isEmpty ? <GitChangeEntry>[entry] : selected;
  }

  Future<void> _stageSelectedOrEntry(GitChangeEntry entry) =>
      _stageSelectedEntries(_selectedOrEntry(entry));

  Future<void> _unstageSelectedOrEntry(GitChangeEntry entry) =>
      _unstageSelectedEntries(_selectedOrEntry(entry));

  Future<void> _discardSelectedOrEntry(GitChangeEntry entry) =>
      _discardSelectedEntries(_selectedOrEntry(entry));

  Future<void> _stageSelectedEntries(List<GitChangeEntry> entries) async {
    final eligible = entries
        .where((entry) => entry.canStageFromParent)
        .toList(growable: false);
    if (eligible.isEmpty) return;
    final completed = await _run(
      () => _notifier.stageEntries(eligible),
      successMessage: eligible.length == 1 ? 'Staged' : 'Files staged',
    );
    if (completed && mounted) _setPanelState(_changeSelection.clear);
  }

  Future<void> _unstageSelectedEntries(List<GitChangeEntry> entries) async {
    final eligible = entries
        .where((entry) => entry.canUnstageFromParent)
        .toList(growable: false);
    if (eligible.isEmpty) return;
    final completed = await _run(
      () => _notifier.unstageEntries(eligible),
      successMessage: eligible.length == 1 ? 'Unstaged' : 'Files unstaged',
    );
    if (completed && mounted) _setPanelState(_changeSelection.clear);
  }

  Future<void> _discardSelectedEntries(List<GitChangeEntry> entries) async {
    final eligible = entries
        .where((entry) => entry.canDiscardFromParent)
        .toList(growable: false);
    if (eligible.isEmpty) return;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (_) => AleraConfirmDialog(
        title: 'Discard Changes?',
        message: eligible.length == 1
            ? 'This permanently discards unstaged and untracked changes in "${eligible.single.path}".'
            : 'This permanently discards changes in ${eligible.length} selected files. Staged changes and unselected files are not included.',
        confirmLabel: 'Discard',
        destructive: true,
      ),
    );
    if (confirmed != true || !mounted) return;
    final completed = await _run(
      () => _notifier.discardEntries(eligible),
      successMessage: eligible.length == 1
          ? 'Change discarded'
          : 'Selected changes discarded',
    );
    if (completed && mounted) _setPanelState(_changeSelection.clear);
  }
}

class const _GitSelectedChangesBar({
  required final List<GitChangeEntry> selected,
  required final bool busy,
  required final VoidCallback onStage,
  required final VoidCallback onUnstage,
  required final VoidCallback onDiscard,
  required final VoidCallback onClear,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: AleraTokens.space8,
        vertical: AleraTokens.space4,
      ),
      child: Row(
        children: <Widget>[
          Expanded(
            child: Text(
              '${selected.length} Selected',
              maxLines: 1,
              overflow: .ellipsis,
              style: Theme.of(context).textTheme.labelSmall,
            ),
          ),
          AleraIconButton(
            tooltip: 'Stage Selected',
            icon: AleraIcons.gitStage,
            onPressed:
                !busy && selected.any((entry) => entry.canStageFromParent)
                ? onStage
                : null,
          ),
          AleraIconButton(
            tooltip: 'Unstage Selected',
            icon: AleraIcons.gitUnstage,
            onPressed:
                !busy && selected.any((entry) => entry.canUnstageFromParent)
                ? onUnstage
                : null,
          ),
          AleraIconButton(
            tooltip: 'Discard Selected',
            icon: AleraIcons.gitDiscard,
            onPressed:
                !busy && selected.any((entry) => entry.canDiscardFromParent)
                ? onDiscard
                : null,
          ),
          AleraIconButton(
            tooltip: 'Clear Selection',
            icon: AleraIcons.close,
            onPressed: onClear,
          ),
        ],
      ),
    );
  }
}
