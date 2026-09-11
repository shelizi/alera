import 'dart:async';

import 'package:alera/src/design_system/layout/alera_choice_dialog.dart';
import 'package:alera/src/features/app_window/application/app_window_platform.dart';
import 'package:alera/src/features/app_window/application/app_window_providers.dart';
import 'package:alera/src/features/runtime_host/application/runtime_host_lifecycle_providers.dart';
import 'package:alera/src/features/runtime_host/domain/runtime_host_quit_decision.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/shared/infra/storage/storage_providers.dart';
import 'package:flutter/material.dart';
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
  bool _bound = false;

  Future<bool> _closeGate() async {
    var visualQuitCommitted = false;
    try {
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
