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

  Future<void> _saveEditor(WidgetRef ref) async {
    try {
      await ref.read(editorSessionRegistryProvider).save(descriptor.tab.id);
    } catch (error) {
      AleraToast.publish(
        message: 'Could not save this file: $error',
        tone: AleraToastTone.error,
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
        appBar: AppBar(
          title: Draggable<String>(
            data: descriptor.tab.id,
            dragAnchorStrategy: pointerDragAnchorStrategy,
            onDragEnd: (_) => unawaited(
              controller.tryReturnTabAtPointer(descriptor.tab.id, viewId),
            ),
            feedback: Material(
              color: Colors.transparent,
              child: DecoratedBox(
                decoration: const BoxDecoration(color: Color(0xFF303030)),
                child: Padding(
                  padding: const EdgeInsets.all(10),
                  child: Text(
                    descriptor.tab.title,
                    style: const TextStyle(color: Colors.white, fontSize: 13),
                  ),
                ),
              ),
            ),
            child: Tooltip(
              message: 'Drag onto the main Alera window to return this tab',
              child: Text(descriptor.tab.title),
            ),
          ),
          actions: <Widget>[
            IconButton(
              tooltip: 'Return tab to main window',
              icon: const Icon(Icons.call_merge),
              onPressed: () => unawaited(
                controller.closeTabWindow(descriptor.tab.id, viewId),
              ),
            ),
          ],
        ),
        body: EarlyInputAttachScope(
          child: DetachedTabSaveShortcuts(
            kind: descriptor.tab.kind,
            onSave: () => unawaited(_saveEditor(ref)),
            child: ListenableBuilder(
              listenable: controller,
              builder: (context, _) {
                if (!controller.canRender(descriptor.tab.id, viewId)) {
                  return const Center(child: CircularProgressIndicator());
                }
                return _SharedTabFrame(
                  key: ValueKey('shared-tab-${descriptor.tab.id}-$viewId'),
                  descriptor: descriptor,
                  controller: controller,
                  viewId: viewId,
                );
              },
            ),
          ),
        ),
      ),
    );
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
