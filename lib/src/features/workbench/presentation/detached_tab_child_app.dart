import 'dart:async';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/app/providers.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/feedback/alera_toast_host.dart';
import 'package:alera/src/features/text_actions/presentation/text_actions_scope.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/domain/workspace_tab_record.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';
import 'package:alera/src/features/workbench/presentation/workspace_workbench_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:window_manager/window_manager.dart';
import 'package:logging/logging.dart';

/// Owns the view for an existing tab, never its database record.
class DetachedTabChildApp extends ConsumerStatefulWidget {
  const DetachedTabChildApp({super.key, required this.client});
  final DetachedTabWindowClient client;

  @override
  ConsumerState<DetachedTabChildApp> createState() =>
      _DetachedTabChildAppState();
}

class _DetachedTabChildAppState extends ConsumerState<DetachedTabChildApp>
    with WindowListener {
  bool _closing = false;
  // Dialogs must use a BuildContext below the MaterialApp's Localizations.
  final GlobalKey<NavigatorState> _navigatorKey = GlobalKey<NavigatorState>();

  @override
  void initState() {
    super.initState();
    windowManager.addListener(this);
  }

  @override
  void dispose() {
    windowManager.removeListener(this);
    super.dispose();
  }

  @override
  void onWindowClose() => unawaited(_dock());

  Future<void> _saveCurrentTab() async {
    if (!mounted || _closing) return;
    try {
      await ref
          .read(editorSessionRegistryProvider)
          .save(widget.client.descriptor.tab.id);
    } catch (error) {
      AleraToast.publish(
        message: 'Could not save this file: $error',
        tone: AleraToastTone.error,
      );
    }
  }

  Future<void> _tryDragBackToMain() async {
    Logger('DetachedTabDrag')
        .info('drag-end closing=$_closing mounted=$mounted');
    if (_closing || !mounted) return;
    // The primary window checks WindowFromPoint via its own Win32 runner:
    // desktop bounds alone could include this child window.
    final accepted = await widget.client.canDockAtPointer();
    Logger('DetachedTabDrag')
        .info('drag-back native pointer accepted=$accepted');
    if (accepted && mounted) await _dock();
  }

  Future<void> _dock() async {
    if (_closing) return;
    setState(() => _closing = true);
    Logger('DetachedTabDrag').info('closing native detached child');
    try {
      final tab = widget.client.descriptor.tab;
      final sessions = ref.read(editorSessionRegistryProvider);
      if (sessions.isDirty(tab.id)) {
        final shouldSave = await showDialog<bool>(
          context: _navigatorKey.currentContext!,
          builder: (context) => AlertDialog(
            title: const Text('Unsaved changes'),
            content: const Text(
              'Save the file before returning this tab to the main window.',
            ),
            actions: [
              TextButton(
                onPressed: () => Navigator.pop(context, false),
                child: const Text('Stay here'),
              ),
              FilledButton(
                onPressed: () => Navigator.pop(context, true),
                child: const Text('Save and return'),
              ),
            ],
          ),
        );
        if (shouldSave != true || !mounted) return;
        await sessions.save(tab.id);
        if (sessions.isDirty(tab.id)) {
          AleraToast.publish(
            message: 'The file still has unsaved changes. Please save it.',
            tone: AleraToastTone.error,
          );
          return;
        }
      }
      Logger('DetachedTabDrag').info('begin socket docking');
      await widget.client.dock();
      Logger('DetachedTabDrag').info('socket docked, destroying HWND');
      await windowManager.destroy();
      Logger('DetachedTabDrag').info('HWND destroyed');
    } catch (error) {
      AleraToast.publish(
        message: 'Could not return this tab: ' + error.toString(),
        tone: AleraToastTone.error,
      );
    } finally {
      if (mounted) setState(() => _closing = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final descriptor = widget.client.descriptor;
    final language = ref.watch(
      settingsControllerProvider.select(
        (settings) => settings.general.language,
      ),
    );
    return MaterialApp(
      navigatorKey: _navigatorKey,
      title: 'Alera - ' + descriptor.tab.title,
      locale: language == AppLanguage.system
          ? null
          : resolveAleraLocale(language, null),
      localeResolutionCallback: (locale, supportedLocales) =>
          resolveAleraLocale(AppLanguage.system, locale),
      theme: aleraDarkTheme,
      darkTheme: aleraDarkTheme,
      themeMode: ThemeMode.dark,
      debugShowCheckedModeBanner: false,
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
            onDragEnd: (_) => unawaited(_tryDragBackToMain()),
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
          actions: [
            TextButton.icon(
              onPressed: _closing ? null : _dock,
              icon: const Icon(Icons.call_merge),
              label: const Text('Return to Alera'),
            ),
          ],
        ),
        body: DetachedTabSaveShortcuts(
          kind: descriptor.tab.kind,
          onSave: () => unawaited(_saveCurrentTab()),
          child: Consumer(
            builder: (context, ref, _) => DetachedWorkspaceTabSurface(
              workspace: descriptor.workspace,
              tab: descriptor.tab,
              terminalRuntime: ref.watch(terminalRuntimeProvider),
              onOpenFile: widget.client.requestOpenFile,
            ),
          ),
        ),
      ),
    );
  }
}

/* Wrap the focused detached editor, not the whole app: other tab types keep
 * their own platform shortcuts (in particular Ctrl+S in a terminal). */
class DetachedTabSaveShortcuts extends StatelessWidget {
  const DetachedTabSaveShortcuts({
    super.key,
    required this.kind,
    required this.onSave,
    required this.child,
  });

  final WorkspaceTabKind kind;
  final VoidCallback onSave;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    if (kind != WorkspaceTabKind.editor) return child;
    return CallbackShortcuts(
      bindings: <ShortcutActivator, VoidCallback>{
        const SingleActivator(LogicalKeyboardKey.keyS, control: true): onSave,
        const SingleActivator(LogicalKeyboardKey.keyS, meta: true): onSave,
      },
      child: child,
    );
  }
}
