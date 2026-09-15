part of 'workspace_git_diff_surface.dart';

final class _EditableWorkingTreeDocument {
  _EditableWorkingTreeDocument({
    required this.relativePath,
    required this.loaded,
  }) : currentText = loaded.displayContent;

  final String relativePath;
  native.WorkspaceEditorTextFile loaded;
  String currentText;
  bool saving = false;
  Object? error;

  bool get dirty => currentText != loaded.displayContent;
}

final class _LiveDiffStats {
  const _LiveDiffStats({required this.added, required this.removed});

  final int added;
  final int removed;
}

_LiveDiffStats _liveDiffStats(
  String baseline,
  String current,
  GitDiffWhitespaceMode whitespaceMode,
) {
  final oldLines = _splitFullFileLines(baseline)
      .map((line) => _normalizeLiveDiffLine(line, whitespaceMode))
      .toList(growable: false);
  final newLines = _splitFullFileLines(current)
      .map((line) => _normalizeLiveDiffLine(line, whitespaceMode))
      .toList(growable: false);
  var prefix = 0;
  while (prefix < oldLines.length &&
      prefix < newLines.length &&
      oldLines[prefix] == newLines[prefix]) {
    prefix += 1;
  }
  var oldSuffix = oldLines.length;
  var newSuffix = newLines.length;
  while (oldSuffix > prefix &&
      newSuffix > prefix &&
      oldLines[oldSuffix - 1] == newLines[newSuffix - 1]) {
    oldSuffix -= 1;
    newSuffix -= 1;
  }
  return _LiveDiffStats(added: newSuffix - prefix, removed: oldSuffix - prefix);
}

String _normalizeLiveDiffLine(String line, GitDiffWhitespaceMode mode) {
  return switch (mode) {
    GitDiffWhitespaceMode.normal => line,
    GitDiffWhitespaceMode.ignoreEol => line.replaceFirst(
      RegExp(r'[ \t]+$'),
      '',
    ),
    GitDiffWhitespaceMode.ignoreChanges =>
      line.replaceAll(RegExp(r'[ \t]+'), ' ').replaceFirst(RegExp(r' $'), ''),
    GitDiffWhitespaceMode.ignoreAll => line.replaceAll(RegExp(r'[ \t]+'), ''),
  };
}

extension _WorkspaceGitDiffEditable on _WorkspaceGitDiffSurfaceState {
  bool _canEditWorkingTreeFile(GitDiffFile file) {
    return widget.tab.gitDiffSource == WorkspaceGitDiffSource.workingTree &&
        file.area != GitChangeArea.staged &&
        (file.area == GitChangeArea.unstaged ||
            file.area == GitChangeArea.untracked) &&
        file.status != GitChangeStatus.deleted &&
        !file.isBinary &&
        !file.isLarge &&
        !file.isGitlink;
  }

  Future<void> _ensureEditableDocument({
    required GitDiffFile file,
    required WorkspaceSourceControlScope sourceControlScope,
    required int loadGeneration,
  }) async {
    if (!_canEditWorkingTreeFile(file)) return;
    final relativePath = sourceControlScope.toWorkspaceRelativePath(file.path);
    if (relativePath == null || _editableDocuments.containsKey(file.path)) {
      return;
    }
    try {
      final tabSize = ref
          .read(settingsControllerProvider)
          .editor
          .tabSize
          .clamp(1, 8)
          .toInt();
      final loaded = await ref
          .read(workspaceFileServiceProvider)
          .readEditorTextFile(
            workspacePath: widget.workspace.path,
            relativePath: relativePath,
            tabSize: tabSize,
            encoding: _encodingSelection.encoding,
          );
      if (!mounted || loadGeneration != _diffLoadGeneration) return;
      _updateDiffState(() {
        _editableDocuments[file.path] = _EditableWorkingTreeDocument(
          relativePath: relativePath,
          loaded: loaded,
        );
      });
    } catch (_) {
      // The ordinary read-only diff remains available when the workspace file
      // cannot be opened through the editor pipeline.
    }
  }

  void _editWorkingTreeDocument(GitDiffFile file, String text) {
    final document = _editableDocuments[file.path];
    if (document == null || document.currentText == text) return;
    _updateDiffState(() {
      document.currentText = text;
      document.error = null;
    });
  }

  Future<void> _saveWorkingTreeDocument(GitDiffFile file) async {
    final document = _editableDocuments[file.path];
    if (document == null || document.saving || !document.dirty) return;
    _updateDiffState(() {
      document.saving = true;
      document.error = null;
    });
    final contentBeingSaved = document.currentText;
    try {
      final saved = await _writeWorkingTreeDocument(
        document,
        overwriteIfChanged: false,
      );
      if (!mounted) return;
      final unchanged = _acceptWorkingTreeSave(
        document,
        saved,
        contentBeingSaved: contentBeingSaved,
      );
      if (unchanged) {
        _load();
      }
    } catch (error) {
      if (!mounted) return;
      if (error is native.WorkspaceFileError &&
          error.kind == native.WorkspaceFileErrorKind.conflict) {
        final overwrite = await showDialog<bool>(
          context: context,
          builder: (context) => const AleraConfirmDialog(
            title: 'File changed on disk',
            message: 'Overwrite the file with the diff editor contents?',
            confirmLabel: 'Overwrite',
            destructive: true,
          ),
        );
        if (overwrite == true && mounted) {
          final overwriteContent = document.currentText;
          try {
            final saved = await _writeWorkingTreeDocument(
              document,
              overwriteIfChanged: true,
            );
            if (!mounted) return;
            final unchanged = _acceptWorkingTreeSave(
              document,
              saved,
              contentBeingSaved: overwriteContent,
            );
            if (unchanged) {
              _load();
            }
          } catch (overwriteError) {
            if (mounted) {
              _updateDiffState(() => document.error = overwriteError);
            }
          }
        }
      } else {
        _updateDiffState(() => document.error = error);
      }
    } finally {
      if (mounted && identical(_editableDocuments[file.path], document)) {
        _updateDiffState(() => document.saving = false);
      }
    }
  }

  Future<native.WorkspaceEditorTextFile> _writeWorkingTreeDocument(
    _EditableWorkingTreeDocument document, {
    required bool overwriteIfChanged,
  }) {
    final tabSize = ref
        .read(settingsControllerProvider)
        .editor
        .tabSize
        .clamp(1, 8)
        .toInt();
    return ref
        .read(workspaceFileServiceProvider)
        .writeEditorTextFile(
          workspacePath: widget.workspace.path,
          relativePath: document.relativePath,
          currentDisplayContent: document.currentText,
          originalRawContent: document.loaded.rawContent,
          originalDisplayContent: document.loaded.displayContent,
          expectedContentToken: document.loaded.contentToken,
          overwriteIfChanged: overwriteIfChanged,
          tabSize: tabSize,
          encoding: document.loaded.encoding,
        );
  }

  bool _acceptWorkingTreeSave(
    _EditableWorkingTreeDocument document,
    native.WorkspaceEditorTextFile saved, {
    required String contentBeingSaved,
  }) {
    final unchanged = document.currentText == contentBeingSaved;
    document.loaded = saved;
    if (unchanged) {
      document.currentText = saved.displayContent;
    }
    document.error = null;
    return unchanged;
  }
}

class _EditableWorkingTreeDiff extends StatefulWidget {
  const _EditableWorkingTreeDiff({
    required this.file,
    required this.baseline,
    required this.document,
    required this.whitespaceMode,
    required this.onChanged,
    required this.onSave,
  });

  final GitDiffFile file;
  final String baseline;
  final _EditableWorkingTreeDocument document;
  final GitDiffWhitespaceMode whitespaceMode;
  final ValueChanged<String> onChanged;
  final VoidCallback onSave;

  @override
  State<_EditableWorkingTreeDiff> createState() =>
      _EditableWorkingTreeDiffState();
}

class _EditableWorkingTreeDiffState extends State<_EditableWorkingTreeDiff> {
  late final TextEditingController _controller;

  @override
  void initState() {
    super.initState();
    _controller = TextEditingController(text: widget.document.currentText);
  }

  @override
  void didUpdateWidget(covariant _EditableWorkingTreeDiff oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (_controller.text != widget.document.currentText &&
        oldWidget.document.currentText != widget.document.currentText) {
      _controller.value = TextEditingValue(
        text: widget.document.currentText,
        selection: TextSelection.collapsed(
          offset: widget.document.currentText.length,
        ),
      );
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final stats = _liveDiffStats(
      widget.baseline,
      widget.document.currentText,
      widget.whitespaceMode,
    );
    final lineCount = math.max(
      _splitFullFileLines(widget.baseline).length,
      _splitFullFileLines(widget.document.currentText).length,
    );
    final height = (lineCount * 18.0 + 72).clamp(260.0, 620.0);
    final textStyle = AleraTokens.monoStyle.copyWith(
      fontSize: 12,
      color: AleraTokens.foreground,
      height: 1.5,
    );
    return CallbackShortcuts(
      bindings: <ShortcutActivator, VoidCallback>{
        const SingleActivator(LogicalKeyboardKey.keyS, control: true):
            widget.onSave,
        const SingleActivator(LogicalKeyboardKey.keyS, meta: true):
            widget.onSave,
      },
      child: SizedBox(
        height: height,
        child: Column(
          crossAxisAlignment: .stretch,
          children: <Widget>[
            Container(
              padding: const EdgeInsets.symmetric(
                horizontal: AleraTokens.space12,
                vertical: AleraTokens.space6,
              ),
              decoration: const BoxDecoration(
                color: AleraTokens.surfaceVariant,
                border: Border(
                  bottom: BorderSide(color: AleraTokens.borderSubtle),
                ),
              ),
              child: Row(
                children: <Widget>[
                  const Expanded(child: Text('Original · Read Only')),
                  Expanded(
                    child: Row(
                      children: <Widget>[
                        const Text('Workspace · Editable'),
                        const Spacer(),
                        if (stats.added > 0)
                          Text(
                            '+${stats.added}',
                            style: const TextStyle(color: AleraTokens.success),
                          ),
                        if (stats.removed > 0) ...<Widget>[
                          const SizedBox(width: AleraTokens.space6),
                          Text(
                            '-${stats.removed}',
                            style: const TextStyle(color: AleraTokens.error),
                          ),
                        ],
                        const SizedBox(width: AleraTokens.space8),
                        AleraIconButton(
                          tooltip: widget.document.saving
                              ? 'Saving File'
                              : 'Save File',
                          icon: widget.document.saving
                              ? AleraIcons.loading
                              : AleraIcons.save,
                          onPressed:
                              widget.document.dirty && !widget.document.saving
                              ? widget.onSave
                              : null,
                        ),
                      ],
                    ),
                  ),
                ],
              ),
            ),
            if (widget.document.error != null)
              const _DiffBanner(message: 'Could not save workspace file.'),
            Expanded(
              child: Row(
                crossAxisAlignment: .stretch,
                children: <Widget>[
                  Expanded(
                    child: DecoratedBox(
                      decoration: const BoxDecoration(
                        border: Border(
                          right: BorderSide(color: AleraTokens.borderSubtle),
                        ),
                      ),
                      child: SingleChildScrollView(
                        padding: const EdgeInsets.all(AleraTokens.space8),
                        child: SelectableText(
                          widget.baseline,
                          style: textStyle,
                        ),
                      ),
                    ),
                  ),
                  Expanded(
                    child: TextField(
                      key: ValueKey<String>(
                        'git-diff-working-tree-editor-${widget.file.path}',
                      ),
                      controller: _controller,
                      expands: true,
                      maxLines: null,
                      minLines: null,
                      keyboardType: TextInputType.multiline,
                      style: textStyle,
                      decoration: const InputDecoration(
                        border: InputBorder.none,
                        contentPadding: EdgeInsets.all(AleraTokens.space8),
                      ),
                      onChanged: widget.onChanged,
                    ),
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
