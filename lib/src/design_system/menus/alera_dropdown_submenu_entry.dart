import 'dart:async';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:flutter/material.dart';

/// A [PopupMenuEntry] with split semantics: tapping the label pops with
/// [primaryValue], while the trailing chevron (or hovering it briefly) opens a
/// nested menu built from [children]. When [primaryValue] is null the whole row
/// opens the submenu instead of popping.
///
/// Submenu items produce values of type [C]; [childResult] maps a picked child
/// value back to the parent menu's value type so the parent route pops with a
/// single result. [onChildResult] additionally reports the raw child value for
/// callers whose result type cannot encode it (for example an enum action that
/// needs the picked editor kind on the side).
class const AleraDropdownSubmenuEntry<T, C>({
  super.key,
  required final T? primaryValue,
  required final String label,
  required final List<PopupMenuEntry<C>> children,
  required final T Function(C value) childResult,
  final ValueChanged<C>? onChildResult,
  final Widget? leading,
  final bool localizeLabel = true,
  final bool enabled = true,
}) extends PopupMenuEntry<T> {
  @override
  double get height => 36;

  @override
  bool represents(T? value) =>
      enabled && primaryValue != null && primaryValue == value;

  @override
  State<AleraDropdownSubmenuEntry<T, C>> createState() =>
      _AleraDropdownSubmenuEntryState<T, C>();
}

class _AleraDropdownSubmenuEntryState<T, C>
    extends State<AleraDropdownSubmenuEntry<T, C>> {
  Timer? _hoverTimer;
  bool _submenuOpen = false;

  @override
  void dispose() {
    _hoverTimer?.cancel();
    super.dispose();
  }

  void _scheduleSubmenu() {
    if (!widget.enabled || widget.children.isEmpty || _submenuOpen) {
      return;
    }
    _hoverTimer ??= Timer(const Duration(milliseconds: 300), _openSubmenu);
  }

  void _cancelHover() {
    _hoverTimer?.cancel();
    _hoverTimer = null;
  }

  void _openSubmenu() {
    _cancelHover();
    if (_submenuOpen || !widget.enabled || widget.children.isEmpty) {
      return;
    }
    final box = context.findRenderObject();
    final overlay = Navigator.of(context).overlay?.context.findRenderObject();
    if (box is! RenderBox || overlay is! RenderBox) {
      return;
    }
    _submenuOpen = true;
    final topRight = box.localToGlobal(
      box.size.topRight(Offset.zero),
      ancestor: overlay,
    );
    final position = RelativeRect.fromRect(
      Rect.fromLTWH(topRight.dx - 4, topRight.dy, 0, 0),
      Offset.zero & overlay.size,
    );
    showMenu<C>(
      context: context,
      position: position,
      items: widget.children,
    ).then((value) {
      _submenuOpen = false;
      if (value != null && mounted) {
        widget.onChildResult?.call(value);
        Navigator.of(context).pop(widget.childResult(value));
      }
    });
  }

  void _handlePrimaryTap() {
    final primary = widget.primaryValue;
    if (primary != null) {
      Navigator.of(context).pop(primary);
      return;
    }
    _openSubmenu();
  }

  @override
  Widget build(BuildContext context) {
    final enabled = widget.enabled;
    final hasChildren = widget.children.isNotEmpty;
    final color = enabled
        ? AleraTokens.foreground
        : AleraTokens.foregroundFaint;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 1),
      child: Row(
        children: <Widget>[
          Expanded(
            child: InkWell(
              onTap: enabled ? _handlePrimaryTap : null,
              mouseCursor: enabled
                  ? SystemMouseCursors.click
                  : SystemMouseCursors.basic,
              borderRadius: .circular(AleraTokens.radiusLg),
              child: Padding(
                padding: const EdgeInsets.symmetric(
                  horizontal: AleraTokens.space8,
                  vertical: AleraTokens.space4,
                ),
                child: Row(
                  children: <Widget>[
                    if (widget.leading != null) ...<Widget>[
                      widget.leading!,
                      const SizedBox(width: AleraTokens.space8),
                    ],
                    Expanded(
                      child: Text(
                        widget.localizeLabel
                            ? context.tr(widget.label)
                            : widget.label,
                        style: Theme.of(context).textTheme.bodyMedium
                            ?.copyWith(color: color),
                      ),
                    ),
                  ],
                ),
              ),
            ),
          ),
          if (hasChildren)
            MouseRegion(
              onEnter: (_) => _scheduleSubmenu(),
              onExit: (_) => _cancelHover(),
              child: GestureDetector(
                behavior: HitTestBehavior.opaque,
                onTap: enabled ? _openSubmenu : null,
                child: Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: AleraTokens.space8,
                    vertical: AleraTokens.space4,
                  ),
                  child: Icon(AleraIcons.chevronRight, size: 16, color: color),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
