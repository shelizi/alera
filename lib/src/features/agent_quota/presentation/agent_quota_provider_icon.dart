import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_quota/domain/agent_quota_provider_agent_type.dart';
import 'package:alera/src/features/agent_status/presentation/agent_identity_icon.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

class const AgentQuotaProviderIcon({
  super.key,
  required final AgentQuotaProviderId provider,
  final double size = 14,
  final bool showTooltip = true,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final agentType = agentTypeForQuotaProvider(provider);
    if (agentType != null) {
      return AgentIdentityIcon(
        agentType: agentType,
        size: size,
        color: AleraTokens.foregroundMuted,
        showTooltip: showTooltip,
      );
    }
    final asset = switch (provider) {
      AgentQuotaProviderId.kimi => 'assets/agents/kimi.svg',
      AgentQuotaProviderId.minimax => 'assets/agents/minimax.svg',
      AgentQuotaProviderId.zai => 'assets/agents/zai.svg',
      _ => throw StateError('Provider icon is not configured: $provider'),
    };
    final icon = Semantics(
      label: provider.label,
      child: SvgPicture.asset(asset, width: size, height: size),
    );
    return showTooltip ? Tooltip(message: provider.label, child: icon) : icon;
  }
}
