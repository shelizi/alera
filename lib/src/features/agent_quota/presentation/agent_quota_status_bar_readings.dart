part of 'agent_quota_status_bar_content.dart';

class const _QuotaReading({
  required final String label,
  required final double remainingPercent,
  required final int order,
  required final String fullLabel,
  required final DateTime? resetsAt,
  required final String? resetDescription,
  final String? valueText,
  final String? tooltipText,
  final double? amountRemainingPercent,
});

class const _QuotaReadingView({
  required final _QuotaReading reading,
  required final AgentQuotaStatus status,
  required final bool compact,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return Row(
      mainAxisSize: .min,
      children: <Widget>[
        Text(
          reading.label,
          style: AleraTokens.monoStyle.copyWith(
            fontSize: compact ? 8 : 9,
            fontWeight: .w500,
            color: AleraTokens.foregroundFaint,
          ),
        ),
        const SizedBox(width: AleraTokens.space2),
        Text(
          reading.valueText ?? '${reading.remainingPercent.round()}%',
          style: AleraTokens.monoStyle.copyWith(
            fontSize: 10,
            fontWeight: .w600,
            color: _quotaColor(
              status,
              reading.valueText == null
                  ? reading.remainingPercent
                  : reading.amountRemainingPercent,
            ),
          ),
        ),
      ],
    );
  }
}

List<_QuotaReading> _quotaReadings(AgentQuotaSnapshot snapshot) {
  final readings = <_QuotaReading>[
    for (final window in snapshot.windows)
      _QuotaReading(
        label: _windowReadingLabel(snapshot.provider, window.label),
        remainingPercent: window.remainingPercent,
        order: _readingOrder(snapshot.provider, window.label),
        fullLabel: window.label,
        resetsAt: window.resetsAt,
        resetDescription: window.resetDescription,
      ),
    for (final amount in snapshot.amounts)
      _quotaAmountReading(snapshot.provider, amount),
    for (final bucket in snapshot.buckets)
      _QuotaReading(
        label: _bucketReadingLabel(snapshot.provider, bucket),
        remainingPercent: bucket.remainingPercent,
        order: _readingOrder(snapshot.provider, bucket.name),
        fullLabel: bucket.name,
        resetsAt: bucket.resetsAt,
        resetDescription: bucket.resetDescription,
      ),
  ];
  readings.sort((left, right) => left.order.compareTo(right.order));
  return readings;
}

bool _isClaudeExtraUsage(
  AgentQuotaProviderId provider,
  AgentQuotaAmount amount,
) {
  if (provider != AgentQuotaProviderId.claude) return false;
  final label = amount.label.trim().toLowerCase();
  // The prior Host version named the same provider field 'Usage credits'.
  return label == 'extra usage' || label == 'usage credits';
}

double? _claudeExtraRemaining(AgentQuotaAmount amount) {
  final direct = amount.remainingAmount;
  if (direct != null && direct.isFinite && direct >= 0) return direct;
  final spent = amount.spentAmount;
  final limit = amount.limitAmount;
  if (spent == null ||
      limit == null ||
      !spent.isFinite ||
      !limit.isFinite ||
      spent < 0 ||
      limit < 0) {
    return null;
  }
  return (limit - spent).clamp(0.0, double.infinity).toDouble();
}

_QuotaReading _quotaAmountReading(
  AgentQuotaProviderId provider,
  AgentQuotaAmount amount,
) {
  final isExtra = _isClaudeExtraUsage(provider, amount);
  final remaining = isExtra ? _claudeExtraRemaining(amount) : null;
  final limit = amount.limitAmount;
  final double? remainingPercent =
      isExtra &&
          remaining != null &&
          limit != null &&
          limit.isFinite &&
          limit > 0
      ? (remaining / limit * 100).clamp(0.0, 100.0).toDouble()
      : null;
  return _QuotaReading(
    label: isExtra ? 'Extra' : _shortWindowLabel(amount.label),
    remainingPercent: 100,
    order: 20,
    fullLabel: isExtra ? 'Extra Usage' : amount.label,
    resetsAt: amount.resetsAt,
    resetDescription: amount.resetDescription,
    valueText: isExtra
        ? remaining == null
              ? '—'
              : '${_formatQuotaAmountValue(remaining, amount.currency)} left'
        : _formatQuotaAmount(amount),
    amountRemainingPercent: remainingPercent,
    tooltipText: _quotaAmountTooltip(provider, amount),
  );
}

String _quotaAmountTooltip(
  AgentQuotaProviderId provider,
  AgentQuotaAmount amount,
) {
  final isExtra = _isClaudeExtraUsage(provider, amount);
  final parts = <String>[];
  if (isExtra) {
    final remaining = _claudeExtraRemaining(amount);
    parts.add(
      remaining == null
          ? 'Remaining unavailable'
          : '${_formatQuotaAmountValue(remaining, amount.currency)} left',
    );
  }
  if (amount.spentAmount case final spent?) {
    parts.add('Used: ${_formatQuotaAmountValue(spent, amount.currency)}');
  }
  if (amount.limitAmount case final limit?) {
    parts.add('Limit: ${_formatQuotaAmountValue(limit, amount.currency)}');
  }
  final remainingAmount = amount.remainingAmount;
  if (!isExtra && remainingAmount != null) {
    parts.add(
      'Remaining: ${_formatQuotaAmountValue(remainingAmount, amount.currency)}',
    );
  }
  if (_resetText(amount.resetsAt, amount.resetDescription) case final reset?) {
    parts.add(reset);
  }
  return '${isExtra ? "Extra Usage" : amount.label}: ${parts.join(" - ")}';
}

String _formatQuotaAmount(AgentQuotaAmount amount) {
  final value =
      amount.spentAmount ?? amount.remainingAmount ?? amount.limitAmount;
  if (value == null) return 'n/a';
  return _formatQuotaAmountValue(value, amount.currency);
}

String _formatQuotaAmountValue(double value, String currency) {
  if (currency == 'USD') {
    return "${r'US$'}${value.toStringAsFixed(2)}";
  }
  if (currency == 'credits') {
    return '${value.toStringAsFixed(2)} credits';
  }
  return '$currency ${value.toStringAsFixed(2)}';
}

String _windowReadingLabel(AgentQuotaProviderId provider, String label) {
  if (provider == AgentQuotaProviderId.claude &&
      label.toLowerCase().contains('fable')) {
    return 'Fable';
  }
  return _shortWindowLabel(label);
}

String _bucketReadingLabel(
  AgentQuotaProviderId provider,
  AgentQuotaBucket bucket,
) {
  final lower = bucket.name.toLowerCase();
  if (provider == AgentQuotaProviderId.claude && lower.contains('fable')) {
    return 'Fable';
  }
  if (provider == AgentQuotaProviderId.agy) {
    final group = lower.contains('argon')
        ? 'Ar'
        : lower.contains('gemini')
        ? 'G'
        : 'C/G';
    return '$group·${_shortWindowLabel(bucket.name)}';
  }
  if (provider == AgentQuotaProviderId.zai) {
    return lower.contains('mcp') ? 'MCP' : _shortWindowLabel(bucket.name);
  }
  if (provider == AgentQuotaProviderId.minimax) {
    final model = bucket.name
        .replaceFirst(RegExp(r'\s+Weekly$', caseSensitive: false), '')
        .replaceFirst(RegExp(r'^MiniMax-', caseSensitive: false), '');
    final compactModel = switch (model.trim().toLowerCase()) {
      'general' => 'G',
      'video' => 'V',
      _ => _compactModelLabel(model),
    };
    return '$compactModel·${_shortWindowLabel(bucket.name)}';
  }
  return _shortWindowLabel(bucket.name);
}

String _shortWindowLabel(String label) {
  final lower = label.toLowerCase();
  if (lower.contains('5 hour') || lower.contains('5h')) {
    return '5H';
  }
  if (lower.contains('weekly') || lower.contains('week')) {
    return 'W';
  }
  if (lower.contains('month')) {
    return 'M';
  }
  if (lower.contains('daily') || lower.contains('day')) {
    return 'D';
  }
  return label.length <= 4 ? label.toUpperCase() : 'Q';
}

String _compactModelLabel(String value) {
  final trimmed = value.trim();
  if (trimmed.length <= 6) {
    return trimmed;
  }
  final version = RegExp(
    r'M\d+(?:\.\d+)*',
    caseSensitive: false,
  ).firstMatch(trimmed)?.group(0);
  return version ?? trimmed.substring(0, 6);
}

int _readingOrder(AgentQuotaProviderId provider, String label) {
  final lower = label.toLowerCase();
  if (provider == AgentQuotaProviderId.claude) {
    if (lower.contains('5 hour') || lower.contains('5h')) {
      return 0;
    }
    if (lower.contains('fable')) {
      return 2;
    }
    if (lower.contains('week')) {
      return 1;
    }
  }
  if (provider == AgentQuotaProviderId.devin) {
    if (lower.contains('daily') || lower.contains('day')) {
      return 0;
    }
    if (lower.contains('week')) {
      return 1;
    }
  }
  if (provider == AgentQuotaProviderId.agy) {
    final group = lower.contains('argon')
        ? 20
        : lower.contains('gemini')
        ? 0
        : 10;
    final window = lower.contains('5 hour') || lower.contains('5h') ? 0 : 1;
    return group + window;
  }
  if (lower.contains('5 hour') || lower.contains('5h')) {
    return 0;
  }
  if (lower.contains('week')) {
    return 1;
  }
  if (lower.contains('month')) {
    return 2;
  }
  return 3;
}
