import 'package:flutter/material.dart';

/// Refocuses a mounted search field and selects its current query without
/// changing the text, search results, or the field's undo history.
///
/// A post-frame request also handles search panels that are being revealed by
/// the same shortcut. The guard prevents a closed panel stealing focus later.
void aleraFocusSearchInput({
  required FocusNode focusNode,
  required TextEditingController controller,
  required bool Function() canFocus,
}) {
  WidgetsBinding.instance.addPostFrameCallback((_) {
    if (!canFocus() || focusNode.context == null) return;
    focusNode.requestFocus();
    controller.selection = TextSelection(
      baseOffset: 0,
      extentOffset: controller.text.length,
    );
  });
}
