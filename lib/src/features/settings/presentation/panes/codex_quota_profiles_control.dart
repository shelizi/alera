import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';
import 'package:path/path.dart' as p;

class const CodexQuotaProfilesControl({
  super.key,
  required final List<CodexQuotaProfileSettings> profiles,
  required final ValueChanged<List<CodexQuotaProfileSettings>> onChanged,
}) extends StatelessWidget {
  Future<void> _edit(
    BuildContext context, [
    CodexQuotaProfileSettings? initial,
  ]) async {
    final updated = await showDialog<CodexQuotaProfileSettings>(
      context: context,
      builder: (_) => _CodexAccountDialog(initial: initial, profiles: profiles),
    );
    if (updated == null) return;
    onChanged([
      for (final profile in profiles)
        if (profile != initial) profile else updated,
      if (initial == null) updated,
    ]);
  }

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: .stretch,
    children: [
      for (final profile in profiles)
        Padding(
          padding: const EdgeInsets.only(bottom: AleraTokens.space8),
          child: Row(
            children: [
              Expanded(
                child: Column(
                  crossAxisAlignment: .start,
                  children: [
                    Text(profile.alias, overflow: .ellipsis),
                    Text(
                      profile.profile,
                      overflow: .ellipsis,
                      style: AleraTokens.monoStyle,
                    ),
                  ],
                ),
              ),
              AleraIconButton(
                tooltip: 'Edit Account',
                icon: AleraIcons.edit,
                onPressed: () => _edit(context, profile),
              ),
              AleraIconButton(
                tooltip: 'Remove Account',
                icon: AleraIcons.delete,
                onPressed: () => onChanged([
                  for (final entry in profiles)
                    if (entry != profile) entry,
                ]),
              ),
            ],
          ),
        ),
      Align(
        alignment: Alignment.centerRight,
        child: TextButton.icon(
          onPressed: () => _edit(context),
          icon: const Icon(AleraIcons.add),
          label: Text(context.tr('Add Account')),
        ),
      ),
    ],
  );
}

class const _CodexAccountDialog({
  required final CodexQuotaProfileSettings? initial,
  required final List<CodexQuotaProfileSettings> profiles,
}) extends StatefulWidget {
  @override
  State<_CodexAccountDialog> createState() => _CodexAccountDialogState();
}

class _CodexAccountDialogState extends State<_CodexAccountDialog> {
  late final _alias = TextEditingController(text: widget.initial?.alias);
  late final _directory = TextEditingController(text: widget.initial?.profile);
  String? _error;

  @override
  void dispose() {
    _alias.dispose();
    _directory.dispose();
    super.dispose();
  }

  void _save() {
    final alias = _alias.text.trim();
    final directory = _directory.text.trim();
    if (alias.isEmpty ||
        !(p.posix.isAbsolute(directory) || p.windows.isAbsolute(directory))) {
      setState(
        () => _error = 'Enter a name and an absolute CODEX_HOME directory.',
      );
      return;
    }
    if (widget.profiles.any(
      (entry) =>
          entry != widget.initial &&
          (entry.profile == directory || entry.alias == alias),
    )) {
      setState(() => _error = 'The name and directory must be unique.');
      return;
    }
    Navigator.of(context)
        .pop(CodexQuotaProfileSettings(alias: alias, profile: directory));
  }

  @override
  Widget build(BuildContext context) => AleraDialog(
    child: Padding(
      padding: const EdgeInsets.all(AleraTokens.space24),
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: [
          Text(
            context.tr(
              widget.initial == null
                  ? 'Add Codex Account'
                  : 'Edit Codex Account',
            ),
            style: Theme.of(context).textTheme.titleLarge,
          ),
          const SizedBox(height: AleraTokens.space16),
          AleraTextField(
            controller: _alias,
            labelText: 'Account Name',
            autofocus: true,
          ),
          const SizedBox(height: AleraTokens.space12),
          AleraTextField(
            controller: _directory,
            labelText: 'CODEX_HOME Directory',
            onSubmitted: (_) => _save(),
          ),
          const SizedBox(height: AleraTokens.space12),
          Text(
            context.tr(
              'Sign in with Codex using this directory first. Login data stays with the CLI.',
            ),
            style: Theme.of(context).textTheme.bodySmall,
          ),
          if (_error != null) ...[
            const SizedBox(height: AleraTokens.space8),
            Text(
              context.tr(_error!),
              style: Theme.of(context).textTheme.bodySmall
                  ?.copyWith(color: AleraTokens.error),
            ),
          ],
          const SizedBox(height: AleraTokens.space20),
          Row(
            mainAxisAlignment: .end,
            children: [
              TextButton(
                onPressed: () => Navigator.of(context).pop(),
                child: Text(context.tr('Cancel')),
              ),
              const SizedBox(width: AleraTokens.space8),
              FilledButton(
                onPressed: _save,
                child: Text(context.tr('Save Account')),
              ),
            ],
          ),
        ],
      ),
    ),
  );
}
