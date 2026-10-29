part of 'project_workbench_sidebar.dart';

/// A project may have several worktrees. Never guess which one should own an
/// existing PTY: let the user select a concrete active destination Workspace.
/// This target stays disabled until a host-backed transfer callback is wired.
class const _TerminalTabProjectDropTarget({
  required final Project project,
  required final List<Workspace> workspaces,
  required final Future<void> Function(WorkbenchTabDragData, Workspace)?
  onTransfer,
  required final Widget child,
}) extends StatelessWidget {
  List<Workspace> _destinations(WorkbenchTabDragData drag) => <Workspace>[
    for (final workspace in workspaces)
      if (workspace.isActive &&
          drag.canTransferTo(
            targetWorkspaceId: workspace.id,
            targetHostId: workspace.hostId,
            targetArchived: workspace.isArchived,
          ))
        workspace,
  ];

  Future<void> _onDrop(BuildContext context, WorkbenchTabDragData data) async {
    final transfer = onTransfer;
    if (transfer == null) return;
    final choices = _destinations(data);
    if (choices.isEmpty) return;

    final Workspace? destination;
    if (choices.length == 1) {
      destination = choices.single;
    } else {
      destination = await showDialog<Workspace>(
        context: context,
        builder: (dialogContext) => SimpleDialog(
          title: Text('Move Terminal to ${project.name}'),
          children: <Widget>[
            for (final workspace in choices)
              SimpleDialogOption(
                onPressed: () => Navigator.of(dialogContext).pop(workspace),
                child: Text(workspace.name),
              ),
          ],
        ),
      );
    }
    if (destination == null || !context.mounted) return;
    try {
      await transfer(data, destination);
    } on Object catch (error) {
      AleraToast.publish(message: 'Could not move Terminal tab: $error');
    }
  }

  @override
  Widget build(BuildContext context) {
    return DragTarget<WorkbenchTabDragData>(
      onWillAcceptWithDetails: (details) =>
          onTransfer != null && _destinations(details.data).isNotEmpty,
      onAcceptWithDetails: (details) =>
          unawaited(_onDrop(context, details.data)),
      builder: (context, candidates, rejected) => candidates.isEmpty
          ? child
          : DecoratedBox(
              decoration: BoxDecoration(
                border: Border.all(color: AleraTokens.accent),
                borderRadius: BorderRadius.circular(AleraTokens.radiusLg),
              ),
              child: child,
            ),
    );
  }
}
