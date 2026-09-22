import 'dart:async';

import 'package:alera/src/app/localization/alera_localizations.dart';
import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/forms/alera_dropdown_field.dart';
import 'package:alera/src/design_system/forms/alera_text_field.dart';
import 'package:alera/src/design_system/layout/alera_settings_group.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_activity.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_status_port.dart';
import 'package:alera/src/features/language_intelligence/application/language_provider_registry.dart';
import 'package:alera/src/features/language_intelligence/domain/language_extension_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_settings.dart';
import 'package:alera/src/features/language_intelligence/domain/language_intelligence_status.dart';
import 'package:alera/src/features/language_intelligence/domain/language_provider_descriptor.dart';
import 'package:alera/src/features/language_intelligence/domain/language_server_session_state.dart';
import 'package:alera/src/features/settings/domain/alera_settings.dart';
import 'package:flutter/material.dart';

class const LanguageIntelligenceSettingsGroup({
  super.key,
  required final EditorSettings settings,
  required final LanguageExtensionRegistry registry,
  required final LanguageIntelligenceStatusPort statusPort,
  required final LanguageIntelligenceActivityPort activity,
  required final ValueChanged<EditorSettings Function(EditorSettings)>
  onChanged,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final languages = registry.languages.toList(growable: false);
    return AleraSettingsGroup(
      key: const ValueKey<String>('language-intelligence-settings-group'),
      title: 'Language Intelligence',
      description: 'Enable project-aware definition and reference navigation per language. Semantic servers stay off until you enable them.',
      children: <Widget>[
        for (final language in languages)
          _LanguageIntelligenceLanguageSetting(
            key: ValueKey<String>('language-intelligence-${language.id.value}'),
            language: language,
            registry: registry,
            statusPort: statusPort,
            activity: activity,
            activation: settings.languageIntelligence.forLanguage(
              language.id,
              structuralParserDefaultEnabled:
                  language.structuralParserDefaultEnabled,
            ),
            onChanged: (activation) => onChanged(
              (current) => current.copyWith(
                languageIntelligence: current.languageIntelligence.withLanguage(
                  language.id,
                  activation,
                ),
              ),
            ),
          ),
      ],
    );
  }
}

class _LanguageIntelligenceLanguageSetting extends StatefulWidget {
  const _LanguageIntelligenceLanguageSetting({
    super.key,
    required this.language,
    required this.registry,
    required this.statusPort,
    required this.activity,
    required this.activation,
    required this.onChanged,
  });

  final LanguageExtensionDescriptor language;
  final LanguageExtensionRegistry registry;
  final LanguageIntelligenceStatusPort statusPort;
  final LanguageIntelligenceActivityPort activity;
  final LanguageActivationSettings activation;
  final ValueChanged<LanguageActivationSettings> onChanged;

  @override
  State<_LanguageIntelligenceLanguageSetting> createState() =>
      _LanguageIntelligenceLanguageSettingState();
}

class _LanguageIntelligenceLanguageSettingState
    extends State<_LanguageIntelligenceLanguageSetting> {
  late final TextEditingController _executableController;
  Future<LanguageIntelligenceProviderStatus>? _statusFuture;
  late LanguageIntelligenceActivitySnapshot _activity;
  StreamSubscription<LanguageIntelligenceActivitySnapshot>?
  _activitySubscription;

  @override
  void initState() {
    super.initState();
    _executableController = TextEditingController(
      text: widget.activation.executablePath ?? '',
    );
    _activity = widget.activity.snapshot;
    _activitySubscription = widget.activity.changes.listen((snapshot) {
      if (mounted) setState(() => _activity = snapshot);
    });
    _refreshStatus();
  }

  @override
  void didUpdateWidget(_LanguageIntelligenceLanguageSetting oldWidget) {
    super.didUpdateWidget(oldWidget);
    final nextPath = widget.activation.executablePath ?? '';
    if (nextPath != _executableController.text) {
      _executableController.text = nextPath;
    }
    if (_statusInputChanged(oldWidget)) {
      _refreshStatus();
    }
  }

  @override
  void dispose() {
    _activitySubscription?.cancel();
    _executableController.dispose();
    super.dispose();
  }

  bool _statusInputChanged(_LanguageIntelligenceLanguageSetting oldWidget) =>
      oldWidget.activation.enabled != widget.activation.enabled ||
      oldWidget.activation.semanticProviderId !=
          widget.activation.semanticProviderId ||
      oldWidget.activation.executablePath != widget.activation.executablePath ||
      oldWidget.statusPort != widget.statusPort;

  LanguageProviderDescriptor? get _provider =>
      widget.registry.semanticProviderFor(
        widget.language.id,
        preferredProviderId: widget.activation.semanticProviderId,
      );

  void _refreshStatus() {
    final provider = _provider;
    if (!widget.activation.enabled) {
      _statusFuture = Future<LanguageIntelligenceProviderStatus>.value(
        const LanguageIntelligenceProviderStatus.disabled(),
      );
      return;
    }
    if (provider == null) {
      _statusFuture = Future<LanguageIntelligenceProviderStatus>.value(
        const LanguageIntelligenceProviderStatus(
          kind: LanguageIntelligenceStatusKind.failed,
          detail: 'No semantic provider is registered for this language.',
        ),
      );
      return;
    }
    _statusFuture = widget.statusPort.resolve(
      provider: provider,
      settings: widget.activation,
    );
  }

  void _commitExecutable() {
    final value = _executableController.text.trim();
    final current = widget.activation.executablePath ?? '';
    if (value == current) {
      return;
    }
    widget.onChanged(
      widget.activation.copyWith(executablePath: value.isEmpty ? null : value),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final provider = _provider;
    final providerIds = widget.language.semanticProviderIds;
    final executableHint =
        provider == null || provider.executableCandidates.isEmpty
        ? null
        : provider.executableCandidates.first;
    return Padding(
      padding: const EdgeInsets.all(AleraTokens.space16),
      child: Column(
        crossAxisAlignment: .stretch,
        children: <Widget>[
          Row(
            children: <Widget>[
              Expanded(
                child: Column(
                  crossAxisAlignment: .start,
                  children: <Widget>[
                    Text(
                      widget.language.displayName,
                      style: theme.textTheme.bodyMedium?.copyWith(
                        color: AleraTokens.foreground,
                        fontWeight: .w600,
                      ),
                    ),
                    const SizedBox(height: AleraTokens.space4),
                    Text(
                      context.tr(
                        'Semantic navigation is disabled until you enable this language.',
                      ),
                      style: theme.textTheme.bodySmall?.copyWith(
                        color: AleraTokens.foregroundMuted,
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(width: AleraTokens.space16),
              Switch(
                key: ValueKey<String>(
                  'language-intelligence-${widget.language.id.value}-enabled',
                ),
                value: widget.activation.enabled,
                onChanged: (enabled) => widget.onChanged(
                  widget.activation.copyWith(enabled: enabled),
                ),
              ),
            ],
          ),
          const SizedBox(height: AleraTokens.space12),
          _LabeledLanguageControl(
            label: 'Structural Parser',
            helperText: 'Retained parser for syntax, outline, folding, and structural editing.',
            child: Align(
              alignment: Alignment.centerLeft,
              child: Switch(
                key: ValueKey<String>(
                  'language-intelligence-${widget.language.id.value}-parser',
                ),
                value: widget.activation.structuralParserEnabled,
                onChanged: (enabled) => widget.onChanged(
                  widget.activation.copyWith(structuralParserEnabled: enabled),
                ),
              ),
            ),
          ),
          if (widget.activation.enabled) ...<Widget>[
            const SizedBox(height: AleraTokens.space12),
            _LabeledLanguageControl(
              label: 'Semantic Server',
              child: AleraDropdownField<String>(
                key: ValueKey<String>(
                  'language-intelligence-${widget.language.id.value}-provider',
                ),
                value: provider?.id,
                entries: <AleraDropdownFieldEntry<String>>[
                  for (final providerId in providerIds)
                    AleraDropdownFieldEntry<String>(
                      value: providerId,
                      label: _providerLabel(
                        widget.registry.provider(providerId),
                        providerId,
                      ),
                      localizeLabel: false,
                    ),
                ],
                onChanged: (providerId) => widget.onChanged(
                  widget.activation.copyWith(semanticProviderId: providerId),
                ),
              ),
            ),
            const SizedBox(height: AleraTokens.space12),
            _LabeledLanguageControl(
              label: 'Executable Override',
              helperText: 'Leave blank to use PATH first, then Alera-managed installation when supported.',
              child: AleraTextField(
                key: ValueKey<String>(
                  'language-intelligence-${widget.language.id.value}-executable',
                ),
                controller: _executableController,
                hintText: executableHint,
                onSubmitted: (_) => _commitExecutable(),
                onEditingComplete: _commitExecutable,
              ),
            ),
          ],
          const SizedBox(height: AleraTokens.space8),
          _LanguageIntelligenceRuntimeStatus(
            acquisition: provider == null
                ? null
                : _activity.acquisitionFor(provider.id),
            server: provider == null ? null : _activity.serverFor(provider.id),
            parser: _activity.parserFor(widget.language.id),
            semanticEnabled: widget.activation.enabled && provider != null,
            parserEnabled: widget.activation.structuralParserEnabled,
          ),
          const SizedBox(height: AleraTokens.space8),
          _LanguageIntelligenceStatusLine(
            key: ValueKey<String>(
              'language-intelligence-${widget.language.id.value}-status',
            ),
            future: _statusFuture,
            onRetry: widget.activation.enabled
                ? () => setState(_refreshStatus)
                : null,
          ),
        ],
      ),
    );
  }

  static String _providerLabel(
    LanguageProviderDescriptor? provider,
    String fallback,
  ) {
    if (provider == null || provider.executableCandidates.isEmpty) {
      return fallback;
    }
    return provider.executableCandidates.first;
  }
}

class const _LanguageIntelligenceRuntimeStatus({
  required final ManagedLanguageServerAcquisitionSnapshot? acquisition,
  required final SemanticServerActivitySnapshot? server,
  required final StructuralParserActivitySnapshot? parser,
  required final bool semanticEnabled,
  required final bool parserEnabled,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final parserStatus = parser;
    final semanticServer = server;
    final parserProgress =
        parserStatus?.state == StructuralParserActivityState.parsing
        ? parserStatus?.progressFraction
        : null;

    return Column(
      crossAxisAlignment: .stretch,
      children: <Widget>[
        _RuntimeStatusRow(
          label: 'Download',
          value: _acquisitionLabel(acquisition, server, semanticEnabled),
          detail: acquisition?.detail,
          busy:
              acquisition?.state ==
                  ManagedLanguageServerAcquisitionState.checking ||
              acquisition?.state ==
                  ManagedLanguageServerAcquisitionState.installing ||
              acquisition?.state ==
                  ManagedLanguageServerAcquisitionState.verifying,
        ),
        _RuntimeStatusRow(
          label: 'Semantic Server',
          value: _serverLabel(server, semanticEnabled),
          detail: semanticServer == null
              ? null
              : '${semanticServer.activeDocumentCount} active document(s)'
                    '${semanticServer.restartAttempts > 0 ? ', restart ${semanticServer.restartAttempts}' : ''}',
          busy:
              server?.state == LanguageServerSessionState.resolvingExecutable ||
              server?.state == LanguageServerSessionState.starting ||
              server?.state == LanguageServerSessionState.initializing ||
              server?.state == LanguageServerSessionState.stopping,
        ),
        _RuntimeStatusRow(
          label: 'Parser',
          value: _parserLabel(parser, parserEnabled),
          detail: _parserDetail(parserStatus),
          busy: parser?.state == StructuralParserActivityState.parsing,
          progress: parserProgress,
        ),
      ],
    );
  }

  static String _acquisitionLabel(
    ManagedLanguageServerAcquisitionSnapshot? snapshot,
    SemanticServerActivitySnapshot? server,
    bool enabled,
  ) {
    if (snapshot == null) {
      if (!enabled) return 'Idle';
      if (server?.executable != null) return 'Not needed';
      return 'Waiting';
    }
    return switch (snapshot.state) {
      ManagedLanguageServerAcquisitionState.checking => 'Checking',
      ManagedLanguageServerAcquisitionState.installing => 'Installing',
      ManagedLanguageServerAcquisitionState.verifying => 'Verifying',
      ManagedLanguageServerAcquisitionState.ready =>
        snapshot.version == null ? 'Ready' : 'Ready 繚 ${snapshot.version}',
      ManagedLanguageServerAcquisitionState.missing => 'Unavailable',
      ManagedLanguageServerAcquisitionState.failed => 'Failed',
    };
  }

  static String _serverLabel(
    SemanticServerActivitySnapshot? snapshot,
    bool enabled,
  ) {
    if (snapshot == null) return enabled ? 'Waiting for document' : 'Disabled';
    return switch (snapshot.state) {
      LanguageServerSessionState.disabled => 'Disabled',
      LanguageServerSessionState.available => 'Idle',
      LanguageServerSessionState.resolvingExecutable => 'Resolving',
      LanguageServerSessionState.starting => 'Starting',
      LanguageServerSessionState.initializing => 'Initializing',
      LanguageServerSessionState.ready => 'Running',
      LanguageServerSessionState.stopping => 'Stopping',
      LanguageServerSessionState.missingExecutable => 'Missing',
      LanguageServerSessionState.failed => 'Failed',
    };
  }

  static String _parserLabel(
    StructuralParserActivitySnapshot? snapshot,
    bool enabled,
  ) {
    if (!enabled) return 'Disabled';
    if (snapshot == null) return 'Waiting for document';
    return switch (snapshot.state) {
      StructuralParserActivityState.parsing => 'Parsing',
      StructuralParserActivityState.ready => 'Ready',
      StructuralParserActivityState.unsupported => 'Unsupported',
      StructuralParserActivityState.failed => 'Failed',
    };
  }

  static String? _parserDetail(StructuralParserActivitySnapshot? snapshot) {
    if (snapshot == null) return null;
    final parts = <String>[];
    if (snapshot.state == StructuralParserActivityState.parsing &&
        snapshot.currentByteOffset != null &&
        snapshot.totalBytes != null &&
        snapshot.totalBytes! > 0) {
      parts.add(
        '${_formatBytes(snapshot.currentByteOffset!)} / '
        '${_formatBytes(snapshot.totalBytes!)}',
      );
    }
    parts.add('${snapshot.activeDocumentCount} active document(s)');
    if (snapshot.revision != null) {
      parts.add('revision ${snapshot.revision}');
    }
    return parts.join(' · ');
  }

  static String _formatBytes(int bytes) {
    if (bytes < 1024) return '$bytes B';
    final kib = bytes / 1024;
    if (kib < 1024) return '${kib.toStringAsFixed(kib < 10 ? 1 : 0)} KB';
    final mib = kib / 1024;
    if (mib < 1024) return '${mib.toStringAsFixed(mib < 10 ? 1 : 0)} MB';
    final gib = mib / 1024;
    return '${gib.toStringAsFixed(gib < 10 ? 1 : 0)} GB';
  }
}

class const _RuntimeStatusRow({
  required final String label,
  required final String value,
  final String? detail,
  final bool busy = false,
  final double? progress,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: AleraTokens.space4),
      child: Row(
        children: <Widget>[
          SizedBox(width: 170, child: Text(context.tr(label))),
          if (progress != null) ...<Widget>[
            SizedBox.square(
              dimension: 12,
              child: CircularProgressIndicator(
                strokeWidth: 1.5,
                value: progress!.clamp(0.0, 1.0),
              ),
            ),
            const SizedBox(width: AleraTokens.space8),
          ] else if (busy) ...<Widget>[
            const SizedBox.square(
              dimension: 12,
              child: CircularProgressIndicator(strokeWidth: 1.5),
            ),
            const SizedBox(width: AleraTokens.space8),
          ],
          Text(
            context.tr(value),
            style: theme.textTheme.bodySmall?.copyWith(
              color: AleraTokens.foreground,
              fontWeight: .w500,
            ),
          ),
          if (progress != null) ...<Widget>[
            const SizedBox(width: AleraTokens.space4),
            Text(
              '${(progress!.clamp(0.0, 1.0) * 100).round()}%',
              style: theme.textTheme.bodySmall?.copyWith(
                color: AleraTokens.foregroundMuted,
                fontFeatures: const <FontFeature>[FontFeature.tabularFigures()],
              ),
            ),
          ],
          if (detail != null && detail!.trim().isNotEmpty) ...<Widget>[
            const SizedBox(width: AleraTokens.space8),
            Expanded(
              child: Text(
                context.tr(detail!),
                maxLines: 2,
                overflow: .ellipsis,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: AleraTokens.foregroundMuted,
                ),
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class const _LabeledLanguageControl({
  required final String label,
  required final Widget child,
  final String? helperText,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Row(
      crossAxisAlignment: .start,
      children: <Widget>[
        SizedBox(
          width: 170,
          child: Column(
            crossAxisAlignment: .start,
            children: <Widget>[
              Text(
                context.tr(label),
                style: theme.textTheme.bodySmall?.copyWith(
                  color: AleraTokens.foreground,
                  fontWeight: .w500,
                ),
              ),
              if (helperText != null) ...<Widget>[
                const SizedBox(height: AleraTokens.space2),
                Text(
                  context.tr(helperText!),
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: AleraTokens.foregroundFaint,
                  ),
                ),
              ],
            ],
          ),
        ),
        const SizedBox(width: AleraTokens.space12),
        Expanded(child: child),
      ],
    );
  }
}

class const _LanguageIntelligenceStatusLine({
  super.key,
  required final Future<LanguageIntelligenceProviderStatus>? future,
  final VoidCallback? onRetry,
}) extends StatelessWidget {
  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return FutureBuilder<LanguageIntelligenceProviderStatus>(
      future: future,
      builder: (context, snapshot) {
        final status = snapshot.data;
        final waiting =
            snapshot.connectionState != ConnectionState.done || status == null;
        final label = waiting ? 'Checking' : _statusLabel(status.kind);
        final detail = waiting
            ? null
            : status.executable ?? status.detail?.trim();
        return Column(
          crossAxisAlignment: .stretch,
          children: <Widget>[
            Row(
              children: <Widget>[
                Text(
                  '${context.tr('Status')}: ${context.tr(label)}',
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: _statusColor(status?.kind),
                    fontWeight: .w500,
                  ),
                ),
                const Spacer(),
                if (onRetry != null && !waiting)
                  TextButton(
                    onPressed: onRetry,
                    child: Text(context.tr('Check Again')),
                  ),
              ],
            ),
            if (detail != null && detail.isNotEmpty)
              Text(
                context.tr(detail),
                maxLines: 4,
                overflow: .ellipsis,
                style: theme.textTheme.bodySmall?.copyWith(
                  color: AleraTokens.foregroundMuted,
                ),
              ),
          ],
        );
      },
    );
  }

  static String _statusLabel(LanguageIntelligenceStatusKind kind) =>
      switch (kind) {
        LanguageIntelligenceStatusKind.disabled => 'Disabled',
        LanguageIntelligenceStatusKind.ready => 'Ready',
        LanguageIntelligenceStatusKind.missing => 'Missing',
        LanguageIntelligenceStatusKind.failed => 'Failed',
      };

  static Color _statusColor(LanguageIntelligenceStatusKind? kind) =>
      switch (kind) {
        LanguageIntelligenceStatusKind.ready => AleraTokens.success,
        LanguageIntelligenceStatusKind.missing ||
        LanguageIntelligenceStatusKind.failed => AleraTokens.error,
        _ => AleraTokens.foregroundMuted,
      };
}
