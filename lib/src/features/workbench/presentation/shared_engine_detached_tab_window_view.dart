part of 'shared_engine_detached_tab_window.dart';

/// Per-View MaterialApp only. No second ProviderScope or Rust/SQLite startup.
/// A new tab widget shares the primary's EditorSessionRegistry, TerminalRuntime
/// and provider state; its scoped native View ID controls close/title.
final class _SharedDetachedTabWindow extends ConsumerWidget {
  const _SharedDetachedTabWindow({
    required this.descriptor,
    required this.viewId,
    required this.controller,
  });

  final DetachedTabDescriptor descriptor;
  final int viewId;
  final SharedEngineDetachedTabWindows controller;

  Future<void> _saveEditor(WidgetRef ref, String tabId) async {
    try {
      await ref.read(editorSessionRegistryProvider).save(tabId);
    } catch (error) {
      AleraToast.publish(
        message: 'Could not save this file: $error',
        tone: AleraToastTone.error,
        viewId: viewId,
      );
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final language = ref.watch(
      settingsControllerProvider.select(
        (settings) => settings.general.language,
      ),
    );
    return MaterialApp(
      title: 'Alera - ${descriptor.tab.title}',
      debugShowCheckedModeBanner: false,
      theme: aleraDarkTheme,
      darkTheme: aleraDarkTheme,
      themeMode: ThemeMode.dark,
      locale: language == AppLanguage.system
          ? null
          : resolveAleraLocale(language, null),
      localeResolutionCallback: (locale, supportedLocales) =>
          resolveAleraLocale(AppLanguage.system, locale),
      supportedLocales: supportedAleraLocales,
      localizationsDelegates: const [
        AleraLocalizationsDelegate(),
        ...GlobalMaterialLocalizations.delegates,
      ],
      builder: (context, child) => Stack(
        children: [
          TextActionsScope(child: child ?? const SizedBox.shrink()),
          const AleraToastHost(),
        ],
      ),
      home: Scaffold(
        body: Column(
          children: <Widget>[
            _DetachedWindowTabStrip(controller: controller, viewId: viewId),
            Expanded(
              child: EarlyInputAttachScope(
                child: ListenableBuilder(
                  listenable: controller,
                  builder: (context, _) {
                    final tabs = controller.tabsForView(viewId);
                    final activeId = controller.activeTabIdForView(viewId);
                    DetachedTabDescriptor? active;
                    for (final candidate in tabs) {
                      if (candidate.tab.id == activeId) {
                        active = candidate;
                        break;
                      }
                    }
                    active ??= tabs.isEmpty ? null : tabs.last;
                    final current = active;
                    if (current == null) {
                      return const Center(child: CircularProgressIndicator());
                    }
                    final canRender = controller.canRender(
                      current.tab.id,
                      viewId,
                    );
                    return DetachedTabSaveShortcuts(
                      kind: current.tab.kind,
                      onSave: () => unawaited(_saveEditor(ref, current.tab.id)),
                      child: canRender
                          ? _SharedTabFrame(
                              key: ValueKey(
                                'shared-tab-${current.tab.id}-$viewId',
                              ),
                              descriptor: current,
                              controller: controller,
                              viewId: viewId,
                            )
                          : const Center(child: CircularProgressIndicator()),
                    );
                  },
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Slim strip that replaces the old Material AppBar in detached windows:
/// one chip per hosted tab, active chip highlighted, a per-chip merge-back
/// affordance and a trailing button that returns every tab to the main
/// window. Chips stay draggable onto the main window or another detached
/// window to move the tab.
final class _DetachedWindowTabStrip extends StatelessWidget {
  const _DetachedWindowTabStrip({
    required this.controller,
    required this.viewId,
  });

  final SharedEngineDetachedTabWindows controller;
  final int viewId;

  @override
  Widget build(BuildContext context) {
    return ListenableBuilder(
      listenable: controller,
      builder: (context, _) {
        final tabs = controller.tabsForView(viewId);
        final activeId = controller.activeTabIdForView(viewId);
        return Container(
          height: 34,
          decoration: const BoxDecoration(
            color: AleraTokens.surface,
            border: Border(bottom: BorderSide(color: AleraTokens.borderSubtle)),
          ),
          child: Row(
            children: <Widget>[
              const SizedBox(width: AleraTokens.space8),
              Expanded(
                child: ListView(
                  scrollDirection: Axis.horizontal,
                  children: <Widget>[
                    for (final entry in tabs)
                      _DetachedWindowTabChip(
                        controller: controller,
                        viewId: viewId,
                        descriptor: entry,
                        active: entry.tab.id == activeId,
                      ),
                  ],
                ),
              ),
              Tooltip(
                message: 'Return all tabs to the main window',
                child: InkWell(
                  onTap: () => unawaited(controller.closeWindowTabs(viewId)),
                  mouseCursor: SystemMouseCursors.click,
                  borderRadius: BorderRadius.circular(AleraTokens.radiusSm),
                  child: const Padding(
                    padding: EdgeInsets.all(AleraTokens.space6),
                    child: Icon(
                      Icons.call_merge,
                      size: 14,
                      color: AleraTokens.foregroundMuted,
                    ),
                  ),
                ),
              ),
              const SizedBox(width: AleraTokens.space8),
            ],
          ),
        );
      },
    );
  }
}

final class _DetachedWindowTabChip extends ConsumerWidget {
  const _DetachedWindowTabChip({
    required this.controller,
    required this.viewId,
    required this.descriptor,
    required this.active,
  });

  final SharedEngineDetachedTabWindows controller;
  final int viewId;
  final DetachedTabDescriptor descriptor;
  final bool active;

  Future<void> _openChipMenu(
    BuildContext context,
    WidgetRef ref,
    Offset globalPosition,
    WorkspaceTabRecord tab,
  ) async {
    final liveTabs = ref
        .read(workbenchControllerProvider)
        .tabsFor(descriptor.workspace.id);
    final groupTabs = <WorkspaceTabRecord>[
      for (final entry in controller.tabsForView(viewId))
        liveTabs
                .where((candidate) => candidate.id == entry.tab.id)
                .firstOrNull ??
            entry.tab,
    ];
    await showWorkspaceTabContextMenu(
      context,
      globalPosition: globalPosition,
      ref: ref,
      workspace: descriptor.workspace,
      sourceControlScope: null,
      tab: tab,
      terminalSession: tab.kind == WorkspaceTabKind.terminal
          ? ref.read(terminalRuntimeProvider).peekSession(tab.id)
          : null,
      groupTabs: groupTabs,
      onSplit: null,
      onClose: () => unawaited(_closeTabs(context, ref, <String>[tab.id])),
      onCloseTabs: (tabIds) => unawaited(_closeTabs(context, ref, tabIds)),
      onRename: (title) => unawaited(
        ref
            .read(workbenchControllerProvider.notifier)
            .renameWorkspaceTab(tabId: tab.id, title: title),
      ),
      onOpenExternalTerminal: tab.kind == WorkspaceTabKind.terminal
          ? (target) async {
              final result = await ref
                  .read(externalTerminalLauncherProvider)
                  .open(
                    ExternalTerminalOpenRequest(
                      workspacePath: descriptor.workspace.path,
                      terminalSessionId: target.terminalSessionId,
                      title: target.title,
                    ),
                  );
              if (!result.ok) {
                AleraToast.publish(
                  message:
                      result.message ??
                      'Could not open the terminal externally.',
                  tone: AleraToastTone.error,
                  duration: const Duration(seconds: 6),
                  viewId: viewId,
                );
              }
            }
          : null,
      detachedActions: WorkspaceTabDetachedMenuActions(
        onMoveToMainWindow: () => controller.closeTabWindow(tab.id, viewId),
        onMoveToNewWindow: () => controller.moveTabToNewWindow(tab.id, viewId),
      ),
    );
  }

  /// Closes the tab records like the primary strip does. The record close
  /// releases the PTY/editor session; [SharedEngineDetachedTabWindows
  /// .forgetClosedTab] then retires the detach bookkeeping instead of
  /// restoring a tab that no longer exists.
  Future<void> _closeTabs(
    BuildContext context,
    WidgetRef ref,
    List<String> tabIds,
  ) async {
    if (tabIds.isEmpty) {
      return;
    }
    final tabs = ref
        .read(workbenchControllerProvider)
        .tabsFor(descriptor.workspace.id);
    if (!context.mounted ||
        !await confirmCloseWorkspaceTabs(context, ref, tabs, tabIds)) {
      return;
    }
    await ref
        .read(workbenchControllerProvider.notifier)
        .closeWorkspaceTabs(workspace: descriptor.workspace, tabIds: tabIds);
    for (final tabId in tabIds) {
      await controller.forgetClosedTab(tabId);
    }
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // The stored descriptor predates renames and pin toggles, so resolve the
    // live record for both the label and the menu's item state.
    final tab =
        ref.watch(
          workbenchControllerProvider.select(
            (state) => state
                .tabsFor(descriptor.workspace.id)
                .where((candidate) => candidate.id == descriptor.tab.id)
                .firstOrNull,
          ),
        ) ??
        descriptor.tab;
    final chip = Container(
      margin: const EdgeInsets.symmetric(vertical: 5),
      padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space8),
      decoration: BoxDecoration(
        color: active ? AleraTokens.surfaceElevated : Colors.transparent,
        borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
        border: Border.all(
          color: active ? AleraTokens.border : AleraTokens.borderSubtle,
        ),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          _DetachedTabIcon(
            kind: tab.kind,
            tab: tab,
            color: active
                ? AleraTokens.foreground
                : AleraTokens.foregroundMuted,
          ),
          const SizedBox(width: AleraTokens.space6),
          ConstrainedBox(
            constraints: const BoxConstraints(maxWidth: 140),
            child: Text(
              tab.title,
              maxLines: 1,
              softWrap: false,
              overflow: TextOverflow.ellipsis,
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: active
                    ? AleraTokens.foreground
                    : AleraTokens.foregroundMuted,
              ),
            ),
          ),
          const SizedBox(width: AleraTokens.space6),
          Tooltip(
            message: 'Return this tab to the main window',
            child: InkWell(
              onTap: () => unawaited(controller.closeTabWindow(tab.id, viewId)),
              mouseCursor: SystemMouseCursors.click,
              borderRadius: BorderRadius.circular(AleraTokens.radiusSm),
              child: const Padding(
                padding: EdgeInsets.all(2),
                child: Icon(
                  Icons.close,
                  size: 11,
                  color: AleraTokens.foregroundMuted,
                ),
              ),
            ),
          ),
        ],
      ),
    );
    return Padding(
      padding: const EdgeInsets.only(right: AleraTokens.space4),
      child: Draggable<String>(
        data: tab.id,
        dragAnchorStrategy: pointerDragAnchorStrategy,
        onDragEnd: (_) =>
            unawaited(controller.tryReturnTabAtPointer(tab.id, viewId)),
        feedback: Material(
          color: Colors.transparent,
          child: DecoratedBox(
            decoration: BoxDecoration(
              color: AleraTokens.surfaceElevated,
              borderRadius: BorderRadius.circular(AleraTokens.radiusMd),
              border: Border.all(color: AleraTokens.border),
            ),
            child: Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: AleraTokens.space8,
                vertical: AleraTokens.space4,
              ),
              child: Text(
                tab.title,
                style: const TextStyle(
                  color: AleraTokens.foreground,
                  fontSize: 11,
                ),
              ),
            ),
          ),
        ),
        childWhenDragging: Opacity(opacity: 0.45, child: chip),
        child: GestureDetector(
          onTap: () => controller.selectDetachedTab(tab.id, viewId),
          onSecondaryTapDown: (details) => unawaited(
            _openChipMenu(context, ref, details.globalPosition, tab),
          ),
          // Middle-click release closes the tab, same as the primary strip.
          onTertiaryTapUp: (_) =>
              unawaited(_closeTabs(context, ref, <String>[tab.id])),
          child: chip,
        ),
      ),
    );
  }
}

final class _DetachedTabIcon extends StatelessWidget {
  const _DetachedTabIcon({
    required this.kind,
    required this.tab,
    required this.color,
  });

  final WorkspaceTabKind kind;
  final WorkspaceTabRecord tab;
  final Color color;

  @override
  Widget build(BuildContext context) {
    return switch (kind) {
      WorkspaceTabKind.editor ||
      WorkspaceTabKind.markdownViewer ||
      WorkspaceTabKind.pdf => AleraFileIcon(
        pathOrName: tab.filePath ?? tab.title,
        kind: .file,
        size: 12,
        fallbackColor: color,
      ),
      WorkspaceTabKind.gitDiff => Icon(
        AleraIcons.gitBranch,
        size: 12,
        color: color,
      ),
      WorkspaceTabKind.gitHistory => Icon(
        AleraIcons.gitGraph,
        size: 12,
        color: color,
      ),
      WorkspaceTabKind.terminal => Icon(
        AleraIcons.terminal,
        size: 12,
        color: color,
      ),
    };
  }
}

final class _SharedTabFrame extends ConsumerStatefulWidget {
  const _SharedTabFrame({
    super.key,
    required this.descriptor,
    required this.controller,
    required this.viewId,
  });
  final DetachedTabDescriptor descriptor;
  final SharedEngineDetachedTabWindows controller;
  final int viewId;

  @override
  ConsumerState<_SharedTabFrame> createState() => _SharedTabFrameState();
}

final class _SharedTabFrameState extends ConsumerState<_SharedTabFrame> {
  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (mounted) {
        widget.controller.reportContentMounted(
          widget.descriptor.tab.id,
          widget.viewId,
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) => DetachedWorkspaceTabSurface(
    workspace: widget.descriptor.workspace,
    tab: widget.descriptor.tab,
    terminalRuntime: ref.read(terminalRuntimeProvider),
    onOpenFile: (path) {
      final handler = detachedTabWindows.onOpenFile;
      if (handler != null) {
        unawaited(handler(widget.descriptor, path));
      }
    },
  );
}
