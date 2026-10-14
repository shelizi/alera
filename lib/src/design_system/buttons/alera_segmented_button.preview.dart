import 'package:alera/src/design_system/alera_preview.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_segmented_button.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:flutter/material.dart';

@AleraPreview(name: 'Three options', group: 'Segmented button')
Widget aleraSegmentedButtonPreview() => AleraSegmentedButton<int>(
  selected: 0,
  onSelectionChanged: (_) {},
  segments: const <ButtonSegment<int>>[
    ButtonSegment<int>(value: 0, icon: Icon(AleraIcons.square)),
    ButtonSegment<int>(value: 1, icon: Icon(AleraIcons.text)),
    ButtonSegment<int>(value: 2, icon: Icon(AleraIcons.remove)),
  ],
);

@AleraPreview(name: 'Compact Filters', group: 'Segmented button')
Widget aleraSegmentedButtonCompactFiltersPreview() => AleraSegmentedButton<int>(
  selected: 0,
  onSelectionChanged: (_) {},
  dense: true,
  padding: const EdgeInsets.symmetric(
    horizontal: AleraTokens.space6,
    vertical: AleraTokens.space4,
  ),
  borderColor: AleraTokens.borderSubtle,
  selectedBackgroundColor: AleraTokens.surfaceElevated,
  segments: const [
    ButtonSegment(value: 0, label: Text('All')),
    ButtonSegment(value: 1, label: Text('A')),
    ButtonSegment(value: 2, label: Text('M')),
    ButtonSegment(value: 3, label: Text('D')),
    ButtonSegment(value: 4, label: Text('R')),
    ButtonSegment(value: 5, label: Text('C')),
  ],
);
