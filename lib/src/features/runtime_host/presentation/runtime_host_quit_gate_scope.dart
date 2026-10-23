import 'dart:async';

import 'package:alera/src/design_system/layout/alera_choice_dialog.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/features/app_window/application/app_window_platform.dart';
import 'package:alera/src/features/app_window/application/app_window_providers.dart';
import 'package:alera/src/features/runtime_host/application/runtime_host_lifecycle_providers.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_quit_decision.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/workbench/application/detached_tab_window_service.dart';
import 'package:alera/src/features/workbench/application/shared_engine_detached_quit_guard.dart';
import 'package:alera/src/features/workbench/application/workbench_providers.dart';
import 'package:alera/src/shared/infra/storage/storage_providers.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

/// Binds the runtime stop-on-quit gate once the app database is ready.
class const RuntimeHostQuitGateScope({super.key, required final Widget child})
    extends ConsumerStatefulWidget {
  @override
  ConsumerState<RuntimeHostQuitGateScope> createState() =>
      _RuntimeHostQuitGateScopeState();
}

class _RuntimeHostQuitGateScopeState
    extends ConsumerState<RuntimeHostQuitGateScope> {
  static const MethodChannel _sharedEngineCloseChannel = MethodChannel(
    'dev.leynier.alera/detached_tabs',
  );
  bool _bound = false;
  bool _sharedCloseHandlerRegistered = false;

  @override
  void initState() {
    super.initState();
    if (detachedTabWindows.hasExperimentalSharedBackend) {
      _sharedEngineCloseChannel.setMethodCallHandler(_handleNativeMainClose);
      _sharedCloseHandlerRegistered = true;
    }
  }

  Future<void> _handleNativeMainClose(MethodCall call) async {
    if (call.method != 'mainCloseRequested') {
      throw MissingPluginException(
        'Unexpected native close method: ${call.method}',
      );
    }
    if (!mounted || !_bound) return;
    // The runner vetoes native WM_CLOSE until Alera's normal lifecycle
    // coordinator can apply hide-on-close and the async quit/save gate.
    ref.read(appWindowLifecycleCoordinatorProvider).onWindowClose();
  }

  @override
  void dispose() {
    if (_sharedCloseHandlerRegistered) {
      _sharedEngineCloseChannel.setMethodCallHandler(null);
    }
    super.dispose();
  }

  Future<bool> _closeGate() async {
    var visualQuitCommitted = false;
    try {
      if (detachedTabWindows.hasExperimentalSharedBackend) {
        final sessions = ref.read(editorSessionRegistryProvider);
        final safeToQuit = await const SharedEngineDetachedQuitGuard()
            .confirmAndSave(
              tabIds: detachedTabWindows.sharedDetachedTabIds,
              isDirty: sessions.isDirty,
              confirm: _confirmSaveDetachedEditors,
              save: sessions.save,
              onSaveFailure: (error) {
                AleraToast.publish(
                  message: 'Could not save detached Editor: $error',
                  tone: AleraToastTone.error,
                );
              },
            );
        if (!safeToQuit) return false;
      }
      final keepRuntimeOpen = ref
          .read(settingsControllerProvider)
          .terminal
          .keepRuntimeOpenOnAppQuit;
      return await ref
          .read(runtimeHostLifecycleServiceProvider)
          .prepareAppQuit(
            keepRuntimeOpen: keepRuntimeOpen,
            confirmBusyQuit: _confirmBusyQuit,
            onBusyQuitCommitted: () {
              visualQuitCommitted = true;
              _commitVisualQuit();
            },
          );
    } catch (_) {
      if (visualQuitCommitted) {
        _restoreAfterFailedCommittedQuit();
      }
      rethrow;
    }
  }

  void _commitVisualQuit() {
    if (!mounted) {
      return;
    }
    final window = ref.read(appWindowControllerProvider);
    // Do not await visibility verification. The native hide request is sent
    // immediately while runtime teardown continues behind the hidden window.
    unawaited(window.hide().catchError((Object _) {}));
  }

  void _restoreAfterFailedCommittedQuit() {
    if (!mounted) {
      return;
    }
    final window = ref.read(appWindowControllerProvider);
    unawaited(window.show().catchError((Object _) {}));
  }

  Future<bool> _confirmSaveDetachedEditors(List<String> dirtyTabIds) async {
    if (!mounted) return false;
    await _showWindowForQuitConfirmation();
    if (!mounted) return false;
    final confirmed = await showDialog<bool>(
      context: context,
      barrierDismissible: false,
      builder: (dialogContext) => AlertDialog(
        title: const Text('Unsaved detached editors'),
        content: Text(
          '${dirtyTabIds.length} detached Editor tab(s) have unsaved changes. '
          'Save them before closing Alera?',
        ),
        actions: [
          TextButton(
            onPressed: () => Navigator.pop(dialogContext, false),
            child: const Text('Cancel'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(dialogContext, true),
            child: const Text('Save and Quit'),
          ),
        ],
      ),
    );
    return confirmed ?? false;
  }

  Future<RuntimeHostQuitDecision> _confirmBusyQuit({
    required String title,
    required String message,
  }) async {
    if (!mounted) {
      return RuntimeHostQuitDecision.cancel;
    }
    // Tray Quit can fire while the window is hidden. The confirmation dialog
    // is in-window, so the window has to be visible first.
    await _showWindowForQuitConfirmation();
    if (!mounted) {
      return RuntimeHostQuitDecision.cancel;
    }
    final decision = await showDialog<RuntimeHostQuitDecision>(
      context: context,
      builder: (_) => AleraChoiceDialog<RuntimeHostQuitDecision>(
        title: title,
        message: message,
        primaryLabel: 'Quit And Leave Runtime Open',
        primaryValue: .leaveRuntimeOpen,
        secondaryLabel: 'Force Stop And Quit',
        secondaryValue: .forceStop,
        destructiveSecondary: true,
      ),
    );
    return decision ?? RuntimeHostQuitDecision.cancel;
  }

  Future<void> _showWindowForQuitConfirmation() async {
    final window = ref.read(appWindowControllerProvider);
    try {
      if (!await window.isVisible()) {
        await window.show();
      }
      if (await window.isMinimized()) {
        await window.restore();
      }
      await window.focus();
    } catch (_) {
      // The dialog still tries to open; a hidden window is worse than a
      // failed restore.
    }
  }

  @override
  Widget build(BuildContext context) {
    if (supportsDesktopAppWindowState) {
      final db = ref.watch(aleraDatabaseProvider);
      if (db.hasValue && !_bound) {
        _bound = true;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (!mounted) {
            return;
          }
          ref
              .read(appWindowLifecycleCoordinatorProvider)
              .bindCloseGate(_closeGate);
        });
      }
    }
    return widget.child;
  }
}
