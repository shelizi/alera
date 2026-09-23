import 'dart:async';

import 'package:alera/src/app/theme/alera_tokens.dart';
import 'package:alera/src/design_system/feedback/alera_toast.dart';
import 'package:alera/src/design_system/surfaces/alera_hover_card.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_activity.dart';
import 'package:alera/src/features/language_intelligence/application/language_intelligence_providers.dart';
import 'package:alera/src/features/language_intelligence/domain/language_server_session_state.dart';
import 'package:alera/src/features/workbench/application/workbench_controller.dart';
import 'package:alera/src/features/workbench/domain/workspace.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

class LanguageIntelligenceStatusBarControl extends ConsumerStatefulWidget {
  const LanguageIntelligenceStatusBarControl({super.key});

  @override
  ConsumerState<LanguageIntelligenceStatusBarControl> createState() =>
      _LanguageIntelligenceStatusBarControlState();
}

class _LanguageIntelligenceStatusBarControlState
    extends ConsumerState<LanguageIntelligenceStatusBarControl> {
  final AleraHoverCardController _hoverCard = AleraHoverCardController();
  bool _busy = false;

  @override
  Widget build(BuildContext context) {
    final workspace = ref.watch(
      workbenchControllerProvider.select((state) => state.activeWorkspace),
    );
    if (workspace == null) return const SizedBox.shrink();
    final activity = ref.watch(languageIntelligenceActivityProvider);
    return StreamBuilder<LanguageIntelligenceActivitySnapshot>(
      stream: activity.changes,
      initialData: activity.snapshot,
      builder: (context, asyncSnapshot) {
        final snapshot =
            asyncSnapshot.data ??
            const LanguageIntelligenceActivitySnapshot.empty();
        final sessions = snapshot.serverSessions.values
            .where((session) => session.workspaceId == workspace.id)
            .toList(growable: false);
        final progress = snapshot.progressForWorkspace(workspace.id);
        if (sessions.isEmpty && progress.isEmpty) {
          return const SizedBox.shrink();
        }
        return AleraHoverCard(
          controller: _hoverCard,
          pinOnTap: false,
          semanticsLabel: 'Language Intelligence',
          card: _LanguageIntelligenceStatusPanel(
            workspace: workspace,
            sessions: sessions,
            progress: progress,
            busy: _busy,
            onRestartProvider: (providerId) =>
                unawaited(_restartProvider(workspace.id, providerId)),
            onReindexWorkspace: () =>
                unawaited(_reindexWorkspace(workspace.id)),
          ),
          child: _LanguageIntelligenceStatusChip(
            sessions: sessions,
            progress: progress,
            onPressed: _hoverCard.togglePin,
          ),
        );
      },
    );
  }

  Future<void> _restartProvider(String workspaceId, String providerId) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      final restarted = await ref
          .read(languageIntelligenceManagerProvider)
          .restartProvider(workspaceId: workspaceId, providerId: providerId);
      if (!mounted) return;
      AleraToast.show(
        context,
        message: restarted
            ? 'Language server restarted. Workspace analysis will run again.'
            : 'No active language server was found.',
      );
    } catch (error) {
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Could not restart language server: $error',
          tone: .error,
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _reindexWorkspace(String workspaceId) async {
    if (_busy) return;
    setState(() => _busy = true);
    try {
      final restarted = await ref
          .read(languageIntelligenceManagerProvider)
          .restartWorkspace(workspaceId);
      if (!mounted) return;
      AleraToast.show(
        context,
        message: restarted == 0
            ? 'No active language servers were found for this workspace.'
            : 'Restarted $restarted language server(s). '
                  'Workspace indexing will run again.',
      );
    } catch (error) {
      if (mounted) {
        AleraToast.show(
          context,
          message: 'Could not reindex workspace: $error',
          tone: .error,
        );
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }
}

class _LanguageIntelligenceStatusChip extends StatelessWidget {
  const _LanguageIntelligenceStatusChip({
    required this.sessions,
    required this.progress,
    required this.onPressed,
  });

  final List<LanguageServerSessionSnapshot> sessions;
  final List<LanguageServerProgressSnapshot> progress;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final primary = progress.length == 1 ? progress.single : null;
    final percentage = primary?.percentage;
    final failed = sessions.any(
      (session) =>
          session.state == LanguageServerSessionState.failed ||
          session.state == LanguageServerSessionState.missingExecutable,
    );
    final readyCount = sessions
        .where((session) => session.state == LanguageServerSessionState.ready)
        .length;
    final label = progress.length > 1
        ? 'LSP ${progress.length} jobs'
        : primary != null
        ? percentage == null
              ? 'LSP …'
              : 'LSP ${percentage.round()}%'
        : failed
        ? 'LSP !'
        : 'LSP $readyCount';
    final progressValue = percentage == null
        ? null
        : (percentage / 100).clamp(0.0, 1.0);
    return Material(
      color: Colors.transparent,
      child: InkWell(
        onTap: onPressed,
        mouseCursor: WidgetStateMouseCursor.clickable,
        child: Tooltip(
          message:
              primary?.title ?? primary?.message ?? 'Language Intelligence',
          child: Container(
            height: AleraTokens.statusBarHeight,
            padding: const EdgeInsets.symmetric(horizontal: AleraTokens.space8),
            decoration: const BoxDecoration(
              border: Border(left: BorderSide(color: AleraTokens.borderSubtle)),
            ),
            child: Row(
              mainAxisSize: .min,
              children: <Widget>[
                if (progress.isNotEmpty)
                  SizedBox.square(
                    dimension: 12,
                    child: CircularProgressIndicator(
                      strokeWidth: 1.5,
                      value: progress.length == 1 ? progressValue : null,
                    ),
                  )
                else
                  Icon(
                    Icons.account_tree_outlined,
                    size: 13,
                    color: failed
                        ? AleraTokens.error
                        : AleraTokens.foregroundMuted,
                  ),
                const SizedBox(width: AleraTokens.space6),
                Text(
                  label,
                  style: AleraTokens.monoStyle.copyWith(fontSize: 10),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }
}

class _LanguageIntelligenceStatusPanel extends StatelessWidget {
  const _LanguageIntelligenceStatusPanel({
    required this.workspace,
    required this.sessions,
    required this.progress,
    required this.busy,
    required this.onRestartProvider,
    required this.onReindexWorkspace,
  });

  final Workspace workspace;
  final List<LanguageServerSessionSnapshot> sessions;
  final List<LanguageServerProgressSnapshot> progress;
  final bool busy;
  final ValueChanged<String> onRestartProvider;
  final VoidCallback onReindexWorkspace;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Container(
      width: 400,
      padding: const EdgeInsets.all(AleraTokens.space12),
      decoration: BoxDecoration(
        color: theme.colorScheme.surface,
        border: Border.all(color: AleraTokens.borderSubtle),
        borderRadius: BorderRadius.circular(AleraTokens.radiusLg),
      ),
      child: Column(
        mainAxisSize: .min,
        crossAxisAlignment: .stretch,
        children: <Widget>[
          Text('Language Intelligence', style: theme.textTheme.titleSmall),
          const SizedBox(height: AleraTokens.space4),
          Text(
            workspace.name,
            maxLines: 1,
            overflow: .ellipsis,
            style: theme.textTheme.bodySmall?.copyWith(
              color: AleraTokens.foregroundMuted,
            ),
          ),
          if (progress.isNotEmpty) ...<Widget>[
            const SizedBox(height: AleraTokens.space12),
            ...progress.map((item) => _ProgressRow(progress: item)),
          ],
          const SizedBox(height: AleraTokens.space12),
          ...sessions.map(
            (session) => _ServerRow(
              session: session,
              busy: busy,
              onRestart: () => onRestartProvider(session.providerId),
            ),
          ),
          const SizedBox(height: AleraTokens.space12),
          Align(
            alignment: Alignment.centerRight,
            child: OutlinedButton.icon(
              onPressed: busy ? null : onReindexWorkspace,
              icon: busy
                  ? const SizedBox.square(
                      dimension: 14,
                      child: CircularProgressIndicator(strokeWidth: 1.5),
                    )
                  : const Icon(Icons.refresh, size: 16),
              label: const Text('Reindex Workspace'),
            ),
          ),
        ],
      ),
    );
  }
}

class _ProgressRow extends StatelessWidget {
  const _ProgressRow({required this.progress});

  final LanguageServerProgressSnapshot progress;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final percentage = progress.percentage;
    return Padding(
      padding: const EdgeInsets.only(bottom: AleraTokens.space8),
      child: Column(
        crossAxisAlignment: .stretch,
        children: <Widget>[
          Row(
            children: <Widget>[
              Expanded(
                child: Text(
                  progress.title ?? progress.providerId,
                  maxLines: 1,
                  overflow: .ellipsis,
                  style: theme.textTheme.bodySmall,
                ),
              ),
              if (percentage != null)
                Text(
                  '${percentage.round()}%',
                  style: AleraTokens.monoStyle.copyWith(fontSize: 10),
                ),
            ],
          ),
          const SizedBox(height: AleraTokens.space4),
          LinearProgressIndicator(
            value: percentage == null
                ? null
                : (percentage / 100).clamp(0.0, 1.0),
          ),
          if (progress.message case final message?) ...<Widget>[
            const SizedBox(height: AleraTokens.space4),
            Text(
              message,
              maxLines: 2,
              overflow: .ellipsis,
              style: theme.textTheme.bodySmall?.copyWith(
                color: AleraTokens.foregroundMuted,
              ),
            ),
          ],
        ],
      ),
    );
  }
}

class _ServerRow extends StatelessWidget {
  const _ServerRow({
    required this.session,
    required this.busy,
    required this.onRestart,
  });

  final LanguageServerSessionSnapshot session;
  final bool busy;
  final VoidCallback onRestart;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final failed =
        session.state == LanguageServerSessionState.failed ||
        session.state == LanguageServerSessionState.missingExecutable;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: AleraTokens.space4),
      child: Row(
        children: <Widget>[
          Icon(
            session.state == LanguageServerSessionState.ready
                ? Icons.check_circle_outline
                : failed
                ? Icons.error_outline
                : Icons.sync,
            size: 15,
            color: failed ? AleraTokens.error : AleraTokens.foregroundMuted,
          ),
          const SizedBox(width: AleraTokens.space8),
          Expanded(
            child: Column(
              crossAxisAlignment: .start,
              children: <Widget>[
                Text(
                  session.providerId.split('.').last,
                  style: theme.textTheme.bodySmall,
                ),
                Text(
                  '${_stateLabel(session.state)} · '
                  '${session.activeDocumentCount} document(s)'
                  '${session.lastError == null ? '' : ' · ${session.lastError}'}',
                  maxLines: 2,
                  overflow: .ellipsis,
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: AleraTokens.foregroundMuted,
                  ),
                ),
              ],
            ),
          ),
          IconButton(
            tooltip: 'Restart language server',
            onPressed: busy ? null : onRestart,
            icon: const Icon(Icons.restart_alt, size: 17),
            visualDensity: VisualDensity.compact,
          ),
        ],
      ),
    );
  }

  static String _stateLabel(LanguageServerSessionState state) =>
      switch (state) {
        LanguageServerSessionState.disabled => 'Disabled',
        LanguageServerSessionState.available => 'Idle',
        LanguageServerSessionState.resolvingExecutable => 'Resolving',
        LanguageServerSessionState.missingExecutable => 'Missing',
        LanguageServerSessionState.starting => 'Starting',
        LanguageServerSessionState.initializing => 'Initializing',
        LanguageServerSessionState.ready => 'Running',
        LanguageServerSessionState.stopping => 'Stopping',
        LanguageServerSessionState.failed => 'Failed',
      };
}
