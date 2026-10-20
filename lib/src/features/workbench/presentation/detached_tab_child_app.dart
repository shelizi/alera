import 'dart:async';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_dark_theme.dart';
import 'package:alera/src/app/providers.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/presentation/terminal_runtime_providers.dart';
import 'package:alera/src/features/workbench/presentation/workspace_workbench_view.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:window_manager/window_manager.dart';

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

  Future<void> _dock() async {
    if (_closing) return;
    setState(() => _closing = true);
    try {
      final tab = widget.client.descriptor.tab;
      final sessions = ref.read(editorSessionRegistryProvider);
      if (sessions.isDirty(tab.id)) {
        final shouldSave = await showDialog<bool>(
          context: context,
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
      await widget.client.dock();
      await windowManager.destroy();
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
    return MaterialApp(
      title: 'Alera - ' + descriptor.tab.title,
      theme: aleraDarkTheme,
      darkTheme: aleraDarkTheme,
      themeMode: ThemeMode.dark,
      debugShowCheckedModeBanner: false,
      supportedLocales: supportedAleraLocales,
      localizationsDelegates: const [
        AleraLocalizationsDelegate(),
        ...GlobalMaterialLocalizations.delegates,
      ],
      home: Scaffold(
        appBar: AppBar(
          title: Text(descriptor.tab.title),
          actions: [
            TextButton.icon(
              onPressed: _closing ? null : _dock,
              icon: const Icon(Icons.call_merge),
              label: const Text('Return to Alera'),
            ),
          ],
        ),
        body: Consumer(
          builder: (context, ref, _) => DetachedWorkspaceTabSurface(
            workspace: descriptor.workspace,
            tab: descriptor.tab,
            terminalRuntime: ref.watch(terminalRuntimeProvider),
            onOpenFile: widget.client.requestOpenFile,
          ),
        ),
      ),
    );
  }
}
