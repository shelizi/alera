import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/buttons/alera_icon_button.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/icons/alera_icons.dart';
import 'package:alera/src/design_system/layout/alera_dialog.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';

/// Registers account labels only. It intentionally does not mutate or
/// impersonate the shared AGY OS credential and never promises quota access.
class const AgyQuotaProfilesControl({
  super.key,
  required final List<AgyQuotaProfileSettings> profiles,
  required final ValueChanged<List<AgyQuotaProfileSettings>> onChanged,
  required final String selectedAccountId,
  required final ValueChanged<String> onSelected,
}) extends StatelessWidget {
  Future<void> _edit(
    BuildContext context, [
    AgyQuotaProfileSettings? initial,
  ]) async {
    final updated = await showDialog<AgyQuotaProfileSettings>(
      context: context,
      builder: (_) => _AgyAccountDialog(initial: initial, profiles: profiles),
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
      DropdownButton<String>(
        isExpanded: true,
        value: profiles.any((entry) => entry.accountId == selectedAccountId)
            ? selectedAccountId
            : 'default',
        items: [
          DropdownMenuItem<String>(
            value: 'default',
            child: Text(context.tr('Default Google account')),
          ),
          for (final profile in profiles)
            DropdownMenuItem<String>(
              value: profile.accountId,
              child: Text(
                '${profile.alias} (${profile.sshTargetId == null ? 'login unverified' : 'isolated SSH Host'})',
              ),
            ),
        ],
        onChanged: (value) {
          if (value != null) onSelected(value);
        },
      ),
      if (selectedAccountId != 'default')
        Padding(
          padding: const EdgeInsets.only(bottom: AleraTokens.space8),
          child: Text(
            context.tr(
              'A separate Windows OS-user Host is required. Registered but '
              'unpaired accounts cannot launch. An SSH-paired account is '
              'verified again before every new launch. Existing terminals are unchanged.',
            ),
            style: Theme.of(context).textTheme.bodySmall,
          ),
        ),
      if (profiles.isEmpty)
        Text(
          context.tr('No additional Antigravity accounts registered'),
          style: Theme.of(context).textTheme.bodySmall,
          textAlign: .right,
        ),
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
                    Text(profile.accountId, style: AleraTokens.monoStyle),
                    Text(
                      context.tr(
                        profile.sshTargetId == null
                            ? 'Pending isolated login - quota and launch unavailable'
                            : 'Paired SSH Host: ${profile.sshTargetId} (Google login checked on refresh)',
                      ),
                      style: Theme.of(context).textTheme.bodySmall,
                    ),
                  ],
                ),
              ),
              AleraIconButton(
                tooltip: 'Edit Antigravity Registration',
                icon: AleraIcons.edit,
                onPressed: () => _edit(context, profile),
              ),
              AleraIconButton(
                tooltip: 'Remove Antigravity Registration',
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
          label: Text(context.tr('Add Google Account')),
        ),
      ),
    ],
  );
}

class const _AgyAccountDialog({
  required final AgyQuotaProfileSettings? initial,
  required final List<AgyQuotaProfileSettings> profiles,
}) extends StatefulWidget {
  @override
  State<_AgyAccountDialog> createState() => _AgyAccountDialogState();
}

class _AgyAccountDialogState extends State<_AgyAccountDialog> {
  late final _alias = TextEditingController(text: widget.initial?.alias);
  late final _accountId = TextEditingController(
    text: widget.initial?.accountId,
  );
  String? _error;

  @override
  void dispose() {
    _alias.dispose();
    _accountId.dispose();
    super.dispose();
  }

  void _save() {
    final alias = _alias.text.trim();
    final accountId = _accountId.text.trim();
    if (widget.initial?.sshTargetId != null &&
        accountId != widget.initial!.accountId) {
      setState(() => _error =
          'A paired account ID is immutable; unpair the SSH target before renaming the ID.');
      return;
    }
    if (alias.isEmpty ||
        alias.length > 80 ||
        alias.contains(RegExp(r'[\x00-\x1f]')) ||
        accountId == 'default' ||
        !RegExp(r'^[a-zA-Z0-9_-]{1,64}$').hasMatch(accountId)) {
      setState(
        () => _error =
            'Enter a name and a unique account ID (letters, digits, _ or -).',
      );
      return;
    }
    if (widget.profiles.any(
      (entry) =>
          entry != widget.initial &&
          (entry.accountId == accountId || entry.alias == alias),
    )) {
      setState(() => _error = 'Account name and ID must be unique.');
      return;
    }
    Navigator.of(context).pop(
      widget.initial?.copyWith(alias: alias, accountId: accountId) ??
          AgyQuotaProfileSettings(alias: alias, accountId: accountId),
    );
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
                  ? 'Add Google Account'
                  : 'Edit Google Account Registration',
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
            controller: _accountId,
            labelText: 'Local Account ID',
            onSubmitted: (_) => _save(),
          ),
          const SizedBox(height: AleraTokens.space12),
          Text(
            context.tr(
              'Registration only. This does not sign in to Google. '
              'Antigravity currently uses a shared OS credential: '
              'separate quota checks, launches, and history copying stay '
              'disabled until an isolated login backend is verified.',
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
                child: Text(context.tr('Save Registration')),
              ),
            ],
          ),
        ],
      ),
    ),
  );
}
