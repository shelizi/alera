import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/features/agent_status/domain/agent_status.dart';
import 'package:flutter/material.dart';
import 'package:flutter_svg/flutter_svg.dart';

class const AgentIdentityIcon({
  super.key,
  required final AgentType agentType,
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
      child: asset.raster
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

const Map<AgentType, _AgentIconAsset>
_agentIconAssets = <AgentType, _AgentIconAsset>{
  AgentType.codex: _AgentIconAsset(path: 'assets/agents/codex.svg'),
  AgentType.claude: _AgentIconAsset(
    path: 'assets/agents/claude.svg',
    tintable: false,
  ),
  AgentType.copilot: _AgentIconAsset(path: 'assets/agents/copilot.svg'),
  AgentType.cursor: _AgentIconAsset(
    path: 'assets/agents/cursor.png',
    raster: true,
  ),
  AgentType.agy: _AgentIconAsset(path: 'assets/agents/agy.png', raster: true),
  AgentType.opencode: _AgentIconAsset(
    path: 'assets/agents/opencode.png',
    raster: true,
  ),
  AgentType.opencode2: _AgentIconAsset(
    path: 'assets/agents/opencode.png',
    raster: true,
  ),
  AgentType.pi: _AgentIconAsset(path: 'assets/agents/pi.svg'),
  AgentType.amp: _AgentIconAsset(path: 'assets/agents/amp.png', raster: true),
  AgentType.grok: _AgentIconAsset(path: 'assets/agents/grok.svg'),
  AgentType.devin: _AgentIconAsset(path: 'assets/agents/devin.svg'),
  AgentType.fx: _AgentIconAsset(path: 'assets/agents/fx.svg'),
};

_AgentIconAsset _agentAsset(AgentType agentType) =>
    _agentIconAssets[agentType]!;
