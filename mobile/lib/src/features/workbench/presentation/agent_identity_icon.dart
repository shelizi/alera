import 'package:alera_mobile/src/app/theme/alera_tokens.dart';
import 'package:alera_mobile/src/core/agent_descriptors.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

export 'package:alera_mobile/src/core/agent_descriptors.dart'
    show canonicalAgentId;

/// Agent brand glyph matching the desktop sidebar identity icons.
class const AgentIdentityIcon({
  super.key,
  required final String agentType,
  final double size = 14,
  final Color color = AleraTokens.foregroundMuted,
  final bool showTooltip = true,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final label = agentDisplayName(agentType);
    final asset = _agentAsset(agentType);
    final icon = Semantics(
      label: label,
      child: asset == null
          ? Icon(Icons.smart_toy_outlined, size: size, color: color)
          : asset.raster
          ? Image.asset(
              asset.path,
              width: size,
              height: size,
              filterQuality: .medium,
            )
          : SvgPicture.asset(
              asset.path,
              width: size,
              height: size,
              colorFilter: asset.tintable
                  ? ColorFilter.mode(color, .srcIn)
                  : null,
            ),
    );
    return showTooltip ? Tooltip(message: label, child: icon) : icon;
  }
}

class const _AgentIconAsset({
  required final String path,
  final bool tintable = true,
  final bool raster = false,
});

String agentDisplayName(String agentType) =>
    canonicalAgentDisplayNames[canonicalAgentId(agentType)] ?? 'Agent';

const Map<String, _AgentIconAsset> _agentIconAssets = <String, _AgentIconAsset>{
  'codex': _AgentIconAsset(path: 'assets/agents/codex.svg'),
  'claude': _AgentIconAsset(path: 'assets/agents/claude.svg', tintable: false),
  'copilot': _AgentIconAsset(path: 'assets/agents/copilot.svg'),
  'cursor': _AgentIconAsset(path: 'assets/agents/cursor.png', raster: true),
  'agy': _AgentIconAsset(path: 'assets/agents/agy.png', raster: true),
  'opencode': _AgentIconAsset(path: 'assets/agents/opencode.png', raster: true),
  'opencode2': _AgentIconAsset(
    path: 'assets/agents/opencode.png',
    raster: true,
  ),
  'pi': _AgentIconAsset(path: 'assets/agents/pi.svg'),
  'amp': _AgentIconAsset(path: 'assets/agents/amp.png', raster: true),
  'grok': _AgentIconAsset(path: 'assets/agents/grok.svg'),
  'fx': _AgentIconAsset(path: 'assets/agents/fx.svg'),
};

_AgentIconAsset? _agentAsset(String agentType) =>
    _agentIconAssets[canonicalAgentId(agentType)];
