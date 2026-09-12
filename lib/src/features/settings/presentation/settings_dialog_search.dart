part of 'settings_dialog.dart';

bool _matchesLocalizedSection(
  BuildContext context,
  SettingsSectionData section,
  String query,
) {
  if (section.matches(query)) {
    return true;
  }
  if (query.isEmpty) {
    return true;
  }
  final localizedValues = <String>[
    context.tr(section.title),
    context.tr(section.description),
    for (final group in section.groups) context.tr(group.title),
    for (final entry in section.entries) ...<String>[
      context.tr(entry.title),
      if (entry.description != null) context.tr(entry.description!),
    ],
  ];
  return localizedValues.any((value) => value.toLowerCase().contains(query));
}

String? _firstLocalizedMatchingGroupId(
  BuildContext context,
  SettingsSectionData section,
  String query,
) {
  final direct = section.firstMatchingGroupId(query);
  if (direct != null || query.isEmpty) {
    return direct;
  }
  for (final group in section.groups) {
    if (context.tr(group.title).toLowerCase().contains(query)) {
      return group.id;
    }
    for (final entry in section.entries) {
      if (entry.groupId != group.id) {
        continue;
      }
      final values = <String>[
        context.tr(entry.title),
        if (entry.description != null) context.tr(entry.description!),
      ];
      if (values.any((value) => value.toLowerCase().contains(query))) {
        return group.id;
      }
    }
  }
  return null;
}
