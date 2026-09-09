import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:flutter/material.dart';

class const AutomationRunNowChoiceDialog({super.key}) extends StatefulWidget {
  @override
  State<AutomationRunNowChoiceDialog> createState() =>
      _AutomationRunNowChoiceDialogState();
}

class _AutomationRunNowChoiceDialogState
    extends State<AutomationRunNowChoiceDialog> {
  bool _precheck = true;
  bool _draftTest = false;
  bool _exactRevision = false;
  String _overlap = 'skip';

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(context.tr('Run Now')),
      content: Column(
        mainAxisSize: .min,
        children: <Widget>[
          SwitchListTile.adaptive(
            title: Text(context.tr('Run Precheck')),
            value: _precheck,
            onChanged: (value) => setState(() => _precheck = value),
          ),
          SwitchListTile.adaptive(
            title: Text(context.tr('Audited Draft Test')),
            value: _draftTest,
            onChanged: (value) => setState(() {
              _draftTest = value;
              if (value) _exactRevision = false;
            }),
          ),
          SwitchListTile.adaptive(
            title: Text(context.tr('Approve Exact Revision')),
            value: _exactRevision,
            onChanged: (value) => setState(() {
              _exactRevision = value;
              if (value) _draftTest = false;
            }),
          ),
          DropdownButtonFormField<String>(
            initialValue: _overlap,
            decoration: InputDecoration(labelText: context.tr('Overlap')),
            items: <DropdownMenuItem<String>>[
              DropdownMenuItem(value: 'skip', child: Text(context.tr('Skip'))),
              DropdownMenuItem(
                value: 'queue',
                child: Text(context.tr('Queue')),
              ),
              DropdownMenuItem(
                value: 'runLatestOnce',
                child: Text(context.tr('Run Latest Once')),
              ),
              DropdownMenuItem(
                value: 'forceParallel',
                child: Text(context.tr('Force Parallel')),
              ),
            ],
            onChanged: (value) => setState(() => _overlap = value!),
          ),
        ],
      ),
      actions: <Widget>[
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(context.tr('Cancel')),
        ),
        FilledButton(
          onPressed: () =>
              Navigator.of(context)
                  .pop((_precheck, _overlap, _draftTest, _exactRevision)),
          child: Text(context.tr('Run')),
        ),
      ],
    );
  }
}

class const AutomationPauseChoiceDialog({super.key}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      title: Text(context.tr('Pause Automation')),
      content: Text(context.tr('Choose what to do with active runs.')),
      actions: <Widget>[
        TextButton(
          onPressed: () => Navigator.of(context).pop('continue-active'),
          child: Text(context.tr('Continue Active')),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop('cancel-active'),
          child: Text(context.tr('Cancel Active')),
        ),
      ],
    );
  }
}
