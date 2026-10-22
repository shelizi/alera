part of 'agent_quota_status_bar_content.dart';

const double _quotaHoverCardWidth = 360;
const double _quotaHoverCardMaxHeight = 480;

class const _AgentQuotaHoverCard({
  required final List<AgentQuotaSnapshot> snapshots,
  final Map<String, String> profileLabels = const <String, String>{},
  final String hostId = 'local',
  final AgentQuotaInlineActions actions = const AgentQuotaInlineActions(),
  final AgentQuotaSnapshotRefresh? onRefreshSnapshot,
  final bool snapshotRefreshEnabled = true,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Container(
      width: _quotaHoverCardWidth,
      constraints: const BoxConstraints(maxHeight: _quotaHoverCardMaxHeight),
      decoration: BoxDecoration(
        color: AleraTokens.surfaceElevated,
        borderRadius: BorderRadius.circular(AleraTokens.radiusLg),
        border: Border.all(color: AleraTokens.border),
        boxShadow: const <BoxShadow>[
          BoxShadow(
            color: AleraTokens.shadowSoft,
            blurRadius: 16,
            offset: Offset(0, 8),
          ),
        ],
      ),
      child: ClipRRect(
        borderRadius: BorderRadius.circular(AleraTokens.radiusLg),
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: .min,
            crossAxisAlignment: .stretch,
            children: <Widget>[
              if (snapshots.isEmpty)
                Padding(
                  padding: const EdgeInsets.all(AleraTokens.space12),
                  child: Text(
                    _normalizeQuotaText('Quota data unavailable'),
                    style: Theme.of(context).textTheme.bodySmall
                        ?.copyWith(color: AleraTokens.foregroundMuted),
                  ),
                ),
              for (final (index, snapshot) in snapshots.indexed) ...<Widget>[
                if (index > 0)
                  const Divider(height: 1, color: AleraTokens.border),
                _AgentQuotaHoverSection(
                  snapshot: snapshot,
                  profileLabel: profileLabels[snapshot.key],
                  hostId: hostId,
                  actions: actions,
                  onRefreshSnapshot: onRefreshSnapshot,
                  refreshEnabled: snapshotRefreshEnabled,
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}

class const _AgentQuotaHoverSection({
  required final AgentQuotaSnapshot snapshot,
  required final String? profileLabel,
  required final String hostId,
  final AgentQuotaInlineActions actions = const AgentQuotaInlineActions(),
  final AgentQuotaSnapshotRefresh? onRefreshSnapshot,
  final bool refreshEnabled = true,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final entries = _quotaHoverEntries(snapshot);
    final statusColor = _quotaColor(snapshot.status, snapshot.remainingPercent);
    return Padding(
      padding: const EdgeInsets.all(AleraTokens.space12),
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: <Widget>[
          Row(
            children: <Widget>[
              AgentQuotaProviderIcon(
                provider: snapshot.provider,
                size: 18,
                showTooltip: false,
              ),
              const SizedBox(width: AleraTokens.space8),
              Expanded(
                child: Column(
                  crossAxisAlignment: .start,
                  children: <Widget>[
                    Text(
                      snapshot.provider.label,
                      style: Theme.of(context).textTheme.labelLarge?.copyWith(
                        color: AleraTokens.foreground,
                        fontWeight: .w600,
                      ),
                    ),
                    if (profileLabel != null)
                      Text(
                        profileLabel!,
                        style: AleraTokens.monoStyle.copyWith(
                          fontSize: 10,
                          color: AleraTokens.foregroundMuted,
                        ),
                      ),
                  ],
                ),
              ),
              AleraBadge(
                label: _quotaStatusLabel(snapshot),
                color: statusColor.withAlpha(28),
                foregroundColor: statusColor,
              ),
              if (onRefreshSnapshot != null) ...<Widget>[
                const SizedBox(width: AleraTokens.space6),
                _QuotaSnapshotRefreshButton(
                  key: ValueKey<String>('quota-refresh:${snapshot.key}'),
                  enabled: refreshEnabled,
                  onRefresh: () => onRefreshSnapshot!(snapshot),
                ),
              ],
            ],
          ),
          const SizedBox(height: AleraTokens.space12),
          actions.buildAccountSwitch(hostId: hostId, snapshot: snapshot),
          if (entries.isEmpty)
            _QuotaHoverEmptyState(snapshot: snapshot)
          else
            for (final (index, entry) in entries.indexed) ...<Widget>[
              if (index > 0) const SizedBox(height: AleraTokens.space12),
              _QuotaHoverReading(entry: entry, status: snapshot.status),
            ],
          if (snapshot.provider == AgentQuotaProviderId.codex &&
              snapshot.rateLimitResetCredits != null)
            actions.buildCodexReset(hostId: hostId, snapshot: snapshot),
          if (_shouldOfferClaudeTui(snapshot)) ...<Widget>[
            const SizedBox(height: AleraTokens.space12),
            actions.buildClaudeTui(hostId: hostId, snapshot: snapshot),
          ],
        ],
      ),
    );
  }
}

bool _shouldOfferClaudeTui(AgentQuotaSnapshot snapshot) {
  return snapshot.provider == AgentQuotaProviderId.claude &&
      snapshot.status != AgentQuotaStatus.ok;
}

class const _QuotaHoverReading({
  required final _QuotaHoverEntry entry,
  required final AgentQuotaStatus status,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    if (entry.amount != null &&
        (entry.provider == AgentQuotaProviderId.claude ||
         entry.provider == AgentQuotaProviderId.codex)) {
      return _QuotaNativeAmountReading(amount: entry.amount!, status: status);
    }
    final color = _quotaColor(status, entry.remainingPercent);
    final label = _quotaHoverLabel(entry.provider, entry.label);
    final isNativeUsage = entry.provider == AgentQuotaProviderId.claude ||
        entry.provider == AgentQuotaProviderId.codex;
    final usedPercent = 100 - entry.remainingPercent;
    final reset = _resetText(entry.resetsAt, entry.resetDescription);
    final small = AleraTokens.monoStyle.copyWith(
      fontSize: 10,
      color: AleraTokens.foregroundFaint,
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Row(
          children: <Widget>[
            Expanded(
              child: Text(
                label,
                overflow: TextOverflow.ellipsis,
                style: Theme.of(context).textTheme.bodySmall?.copyWith(
                  color: AleraTokens.foregroundMuted,
                  fontWeight: .w500,
                ),
              ),
            ),
            if (!isNativeUsage) ...<Widget>[
              const SizedBox(width: AleraTokens.space12),
              Text(
                entry.valueText ?? '${entry.remainingPercent.round()}% Left',
                style: AleraTokens.monoStyle.copyWith(
                  fontSize: 11, color: color, fontWeight: FontWeight.w600,
                ),
              ),
            ],
          ],
        ),
        const SizedBox(height: AleraTokens.space6),
        if (entry.valueText == null)
          LinearProgressIndicator(
            value: (isNativeUsage ? usedPercent : entry.remainingPercent) / 100,
            minHeight: AleraTokens.space4,
            color: color,
            backgroundColor: AleraTokens.surfaceVariant,
            borderRadius: BorderRadius.circular(AleraTokens.radiusPill),
            semanticsLabel: isNativeUsage ? '$label Used' : '$label Remaining',
            semanticsValue: '${(isNativeUsage ? usedPercent : entry.remainingPercent).round()}%',
          ),
        const SizedBox(height: AleraTokens.space6),
        if (isNativeUsage)
          Row(
            children: <Widget>[
              Text('${usedPercent.round()}% used', style: small),
              const Spacer(),
              if (reset != null) Flexible(child: Text(reset, style: small, textAlign: TextAlign.right)),
            ],
          )
        else
          Text(reset ?? 'Reset time unavailable', style: small),
      ],
    );
  }
}

class _QuotaNativeAmountReading extends StatelessWidget {
  const _QuotaNativeAmountReading({required this.amount, required this.status});

  final AgentQuotaAmount amount;
  final AgentQuotaStatus status;

  @override
  Widget build(BuildContext context) {
    final spent = amount.spentAmount;
    final limit = amount.limitAmount;
    // Only derive the bar ratio from provider-reported amounts. No token or
    // model-price estimate is used to manufacture an actual billed cost.
    final usedFraction = spent != null && limit != null && limit > 0
        ? (spent / limit).clamp(0.0, 1.0).toDouble()
        : null;
    final color = _quotaColor(
      status,
      usedFraction == null ? null : 100 - usedFraction * 100,
    );
    final valueText = spent != null && limit != null
        ? '${_formatQuotaAmountValue(spent, amount.currency)} / ${_formatQuotaAmountValue(limit, amount.currency)}'
        : _formatQuotaAmount(amount);
    final small = AleraTokens.monoStyle.copyWith(
      fontSize: 10, color: AleraTokens.foregroundFaint,
    );
    final reset = _resetText(amount.resetsAt, amount.resetDescription);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        Text(
          amount.label,
          style: Theme.of(context).textTheme.bodySmall?.copyWith(
            color: AleraTokens.foregroundMuted,
            fontWeight: FontWeight.w500,
          ),
        ),
        if (usedFraction != null) ...<Widget>[
          const SizedBox(height: AleraTokens.space6),
          LinearProgressIndicator(
            value: usedFraction,
            minHeight: AleraTokens.space4,
            color: color,
            backgroundColor: AleraTokens.surfaceVariant,
            borderRadius: BorderRadius.circular(AleraTokens.radiusPill),
            semanticsLabel: '${amount.label} Used',
            semanticsValue: '${(usedFraction * 100).round()}%',
          ),
        ],
        const SizedBox(height: AleraTokens.space6),
        Row(
          children: <Widget>[
            Expanded(child: Text(valueText, style: small)),
            if (usedFraction != null)
              Text('${(usedFraction * 100).round()}% used', style: small),
          ],
        ),
        if (reset != null) ...<Widget>[
          const SizedBox(height: AleraTokens.space4),
          Text(reset, style: small),
        ],
      ],
    );
  }
}

class const _QuotaHoverEmptyState({required final AgentQuotaSnapshot snapshot})
    extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Text(
      snapshot.error == null
          ? 'Quota data unavailable'
          : _normalizeQuotaText(snapshot.error!),
      style: Theme.of(context).textTheme.bodySmall
          ?.copyWith(color: AleraTokens.foregroundMuted),
    );
  }
}

class const _QuotaHoverEntry({
  required final AgentQuotaProviderId provider,
  required final String label,
  required final double remainingPercent,
  required final DateTime? resetsAt,
  required final String? resetDescription,
  final String? valueText,
  final AgentQuotaAmount? amount,
});

List<_QuotaHoverEntry> _quotaHoverEntries(AgentQuotaSnapshot snapshot) {
  return <_QuotaHoverEntry>[
    for (final window in snapshot.windows)
      _QuotaHoverEntry(
        provider: snapshot.provider,
        label: window.label,
        remainingPercent: window.remainingPercent,
        resetsAt: window.resetsAt,
        resetDescription: window.resetDescription,
      ),
    for (final bucket in snapshot.buckets)
      _QuotaHoverEntry(
        provider: snapshot.provider,
        label: bucket.name,
        remainingPercent: bucket.remainingPercent,
        resetsAt: bucket.resetsAt,
        resetDescription: bucket.resetDescription,
      ),
    for (final amount in snapshot.amounts)
      _QuotaHoverEntry(
        provider: snapshot.provider,
        label: amount.label,
        remainingPercent: 100,
        resetsAt: amount.resetsAt,
        resetDescription: amount.resetDescription,
        valueText: _formatQuotaAmount(amount),
        amount: amount,
      ),
  ]..sort(
    (left, right) => _readingOrder(
      snapshot.provider,
      left.label,
    ).compareTo(_readingOrder(snapshot.provider, right.label)),
  );
}

String _quotaHoverLabel(AgentQuotaProviderId provider, String value) {
  final normalized = _normalizeQuotaText(value);
  if (provider != AgentQuotaProviderId.minimax || normalized.isEmpty) {
    return normalized;
  }
  return normalized[0].toUpperCase() + normalized.substring(1);
}

String _quotaStatusLabel(AgentQuotaSnapshot snapshot) {
  if (snapshot.status == AgentQuotaStatus.ok &&
      snapshot.dataQuality == 'estimated') {
    return 'Estimated';
  }
  final status = snapshot.status;
  return switch (status) {
    AgentQuotaStatus.ok => 'Live',
    AgentQuotaStatus.stale => 'Stale',
    AgentQuotaStatus.error => 'Error',
    AgentQuotaStatus.unavailable => 'Unavailable',
  };
}
