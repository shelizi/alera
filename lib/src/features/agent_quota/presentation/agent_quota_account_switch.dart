import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/forms/alera_dropdown_field.dart';
import 'package:alera/src/features/settings/application/settings_controller.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class const AgentQuotaAccountSwitch({
  super.key,
  required final String hostId,
  required final AgentQuotaProviderId provider,
}) extends ConsumerStatefulWidget {
  @override
  ConsumerState<AgentQuotaAccountSwitch> createState() =>
      _AgentQuotaAccountSwitchState();
}

class _AgentQuotaAccountSwitchState
    extends ConsumerState<AgentQuotaAccountSwitch> {
  bool _saving = false;

  Future<void> _select(String profile) async {
    if (_saving) return;
    setState(() => _saving = true);
    try {
      final controller = ref.read(settingsControllerProvider.notifier);
      if (widget.provider == AgentQuotaProviderId.codex) {
        await controller.setSelectedCodexQuotaProfile(
          hostId: widget.hostId,
          profile: profile,
        );
      } else {
        await controller.setSelectedClaudeQuotaProfile(
          hostId: widget.hostId,
          profile: profile,
        );
      }
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Account selected for new terminals. Existing sessions keep their current account.',
        );
      }
    } catch (error) {
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Could not switch account: $error',
          tone: AleraToastTone.error,
        );
      }
    } finally {
      if (mounted) setState(() => _saving = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    if (widget.provider != AgentQuotaProviderId.codex &&
        widget.provider != AgentQuotaProviderId.claude) {
      return const SizedBox.shrink();
    }
    final settings = ref.watch(
      settingsControllerProvider.select(
        (settings) => settings.agents.quotas.forHost(widget.hostId),
      ),
    );
    final codex = widget.provider == AgentQuotaProviderId.codex;
    final entries = <AleraDropdownFieldEntry<String>>[
      const AleraDropdownFieldEntry(value: 'default', label: 'Default'),
      if (codex)
        for (final profile in settings.codexProfiles)
          AleraDropdownFieldEntry(
            value: profile.profile,
            label: profile.alias,
            localizeLabel: false,
          )
      else
        for (final profile in settings.claudeProfiles)
          AleraDropdownFieldEntry(
            value: profile.profile,
            label: profile.alias,
            localizeLabel: false,
          ),
    ];
    if (entries.length == 1) return const SizedBox.shrink();
    final selected = codex
        ? settings.selectedCodexProfile
        : settings.selectedClaudeProfile;
    return Padding(
      padding: const EdgeInsets.only(bottom: AleraTokens.space12),
      child: AleraDropdownField<String>(
        labelText: 'Active Account',
        value: entries.any((entry) => entry.value == selected)
            ? selected
            : 'default',
        entries: entries,
        enabled: !_saving,
        onChanged: _select,
      ),
    );
  }
}
