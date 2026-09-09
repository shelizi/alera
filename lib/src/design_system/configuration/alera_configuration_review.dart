import 'dart:convert';

import 'package:alera_configuration/alera_configuration.dart';
import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:flutter/material.dart';

class const AleraConfigurationReview({
  super.key,
  required final String target,
  required final ConfigurationScreenState state,
  required this.onRefresh,
  required this.onHistory,
  required final ValueChanged<int> onRestore,
  required final void Function(ConfigurationDifference, ConfigurationChoice)
  onChoice,
  required final void Function(ConfigurationDifference, String) onRename,
  required final ValueChanged<ConfigurationChoice> onChooseAll,
  required final ValueChanged<bool> onApply,
  required this.onRetry,
}) extends StatelessWidget {
  final VoidCallback onRefresh, onHistory, onRetry;

  @override
  Widget build(BuildContext context) {
    final review = state.review;
    final differences = review?.merge.differences ?? [];
    return Column(
      crossAxisAlignment: .stretch,
      children: [
        Text(
          '${context.tr('Target')}: $target',
          style: Theme.of(context).textTheme.titleMedium,
        ),
        const SizedBox(height: AleraTokens.space12),
        Text(
          context.tr(
            'Configuration is stored in your Alera account and can be read by the service. Review custom commands and prompts for embedded secrets. Credentials and device permissions stay local.',
          ),
        ),
        const SizedBox(height: AleraTokens.space12),
        Wrap(
          spacing: AleraTokens.space8,
          runSpacing: AleraTokens.space8,
          children: [
            OutlinedButton(
              onPressed: state.busy ? null : onRefresh,
              child: Text(context.tr('Review Changes')),
            ),
            OutlinedButton(
              onPressed: state.busy ? null : onHistory,
              child: Text(context.tr('History')),
            ),
            if (review?.local.pending != null)
              OutlinedButton(
                onPressed: state.busy ? null : onRetry,
                child: Text(context.tr('Retry Pending Upload')),
              ),
          ],
        ),
        if (state.busy) const LinearProgressIndicator(),
        if (state.error != null)
          Padding(
            padding: const EdgeInsets.symmetric(vertical: AleraTokens.space12),
            child: SelectableText(
              state.error!,
              style: TextStyle(color: Theme.of(context).colorScheme.error),
            ),
          ),
        if (review != null) ...[
          const SizedBox(height: AleraTokens.space12),
          Text(
            '${context.tr('Shared version')}: ${context.tr('${review.head?.revision ?? "None"}')} • ${context.tr('Comparing')}: ${context.tr('${review.source?.revision ?? "Empty"}')}',
          ),
          Text(
            '${differences.length} ${context.tr('differences')} • ${differences.where((d) => d.choice == null).length} ${context.tr('unresolved')}',
          ),
          Wrap(
            spacing: AleraTokens.space8,
            children: [
              TextButton(
                onPressed: state.busy ? null : () => onChooseAll(.local),
                child: Text(context.tr('Keep All Local')),
              ),
              TextButton(
                onPressed: state.busy ? null : () => onChooseAll(.remote),
                child: Text(context.tr('Keep All Remote')),
              ),
            ],
          ),
          for (final difference in differences)
            Padding(
              padding: const EdgeInsets.only(bottom: AleraTokens.space12),
              child: Card(
                child: Padding(
                  padding: const EdgeInsets.all(AleraTokens.space12),
                  child: Column(
                    crossAxisAlignment: .stretch,
                    children: [
                      Text(
                        context.tr(difference.label),
                        style: Theme.of(context).textTheme.titleSmall,
                      ),
                      if (difference.conflict)
                        Text(
                          context.tr(
                            'Both sides changed this value. Choose what to keep.',
                          ),
                        ),
                      const SizedBox(height: AleraTokens.space8),
                      SelectableText(
                        '${context.tr('Local')}: ${context.tr(_display(difference, true))}',
                      ),
                      const SizedBox(height: AleraTokens.space8),
                      SelectableText(
                        '${context.tr('Remote')}: ${context.tr(_display(difference, false))}',
                      ),
                      DropdownButton<ConfigurationChoice>(
                        isExpanded: true,
                        value: difference.choice,
                        hint: Text(context.tr('Choose A Value')),
                        items: [
                          DropdownMenuItem(
                            value: .local,
                            child: Text(context.tr('Keep Local')),
                          ),
                          DropdownMenuItem(
                            value: .remote,
                            child: Text(context.tr('Keep Remote')),
                          ),
                        ],
                        onChanged: state.busy
                            ? null
                            : (value) {
                                if (value != null) onChoice(difference, value);
                              },
                      ),
                      if (difference.choice != null && difference.canRename)
                        TextFormField(
                          key: ValueKey(
                            '${difference.label}/${difference.choice}',
                          ),
                          initialValue:
                              (difference.path.last == 'name'
                                      ? difference.result
                                      : jsonMap(difference.result)['name'])
                                  as String?,
                          decoration: InputDecoration(
                            labelText: difference.path.contains('textActions')
                                ? context.tr('Action Name')
                                : context.tr('Profile Name'),
                          ),
                          onChanged: state.busy
                              ? null
                              : (value) => onRename(difference, value),
                        ),
                      SelectableText(
                        '${context.tr('Result')}: ${difference.choice == null
                            ? context.tr("Unresolved")
                            : difference.customResult != null
                            ? const JsonEncoder.withIndent("  ").convert(difference.result)
                            : _display(difference, difference.choice == ConfigurationChoice.local)}',
                      ),
                    ],
                  ),
                ),
              ),
            ),
          Text('${context.tr('These changes apply only to')} $target.'),
          const SizedBox(height: AleraTokens.space8),
          Wrap(
            spacing: AleraTokens.space8,
            runSpacing: AleraTokens.space8,
            children: [
              FilledButton(
                onPressed: state.busy || review.merge.hasUnresolved
                    ? null
                    : () => onApply(false),
                child: Text(context.tr('Apply To Device')),
              ),
              FilledButton(
                onPressed: state.busy || review.merge.hasUnresolved
                    ? null
                    : () => onApply(true),
                child: Text(context.tr('Apply And Upload')),
              ),
            ],
          ),
          const SizedBox(height: AleraTokens.space12),
          Text(
            context.tr(
              'Leaving this screen without applying keeps your configuration unchanged. If upload fails after applying, local changes remain pending.',
            ),
          ),
        ],
        for (final item in state.history)
          ListTile(
            title: Text(
              '${context.tr('Version')} ${item['revision']} • ${item['deviceName']}',
            ),
            subtitle: Text('${item['createdAt']}\n${item['summary']}'),
            trailing: TextButton(
              onPressed: state.busy
                  ? null
                  : () => onRestore(item['revision'] as int),
              child: Text(context.tr('Compare')),
            ),
          ),
      ],
    );
  }

  String _display(ConfigurationDifference difference, bool local) {
    if (local ? difference.localAbsent : difference.remoteAbsent) {
      return '(Removed)';
    }
    return const JsonEncoder.withIndent('  ')
        .convert(local ? difference.local : difference.remote);
  }
}
