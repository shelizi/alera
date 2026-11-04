# Alera MCP — cross-project navigation (TDD)

## Scope

The built-in MCP agent may navigate *visible, active and nonarchived* Alera
workspaces in other projects without obtaining another project's Agent or PTY
control credentials. This is a UI operation, not an Agent-message or shell API.

## Tool contracts

The following MCP tools support project-scoped navigation:

| Tool | Parameters | Effect |
| --- | --- | --- |
| `alera.list_projects` | optional `query`, `limit` | Discover project IDs |
| `alera.list_workspaces` | optional `projectId`, `limit` | Discover Workspace IDs |
| `alera.list_tabs` | optional `workspaceId`, `projectId`, `limit` | Read only metadata for current or explicitly selected Workspace |
| `alera.focus_workspace` | required `workspaceId`, optional `projectId`, `windowId` | Select existing Workspace without creating an initial Terminal |
| `alera.select_tab` | required `workspaceId`, `tabId`; optional `projectId`, `windowId` | Select an existing Tab after target validation |
| `alera.open_file` | required `relativePath`; optional `workspaceId`, `projectId`, `editor`, `openMode`, `line`, `column`, `windowId` | Open in Alera or a supported installed external Editor, optionally at line and column |
| `alera.reveal_file` | required `relativePath`; optional `workspaceId`, `projectId`, `windowId` | Reveal target file in Explorer without opening it |

`editor` defaults to `alera` **regardless of the App's global default
code-open setting**. The only other allowed values are `zed` and `vscode`,
matching Alera's existing external editor registry. Explicit external opens
reuse the existing configured launcher/executable (including user-configured
paths), but the Agent cannot supply an arbitrary command or executable.
The requested editor must be installed; failures do **not** silently fall
back to the internal editor. An external launch does not create a Tab or
change Alera's active Workspace, and its successful receipt confirms process
startup, **not** external window focus or exact caret placement.

`openMode`: `reuse` is the default. `new` creates an independent permanent
**internal** Editor Tab, even if the target file is already open. `new` is
rejected for an external editor, which follows its own window policy.

`line` is an optional **1-based** integer (1 through 1,000,000,000).
`column` is an optional **1-based** integer with the same bounds and requires
`line`. With an internal editor, the caret column is clamped to the end of
the selected line; with Zed and VS Code, Alera forwards both coordinates to
the existing editor-specific CLI argument builder (`file:line:column` or
`--goto file:line:column` respectively).
When present, Alera moves the caret to the requested line and scrolls it into
view. If it exceeds the loaded document's line count, Alera clamps to the last
line (matching the built-in Editor Go to Line behavior). Without `line`,
opening a file preserves its previous viewport and caret. An applied MCP
receipt is sent only after the Editor has accepted the scroll, even when a
newly opened tab needs to finish loading. Unavailable/failed scrolls time out
and report failure instead of falsely claiming success.

For any target Workspace other than the calling Agent's own, **both**
`workspaceId` and `projectId` must be provided; the Host checks that they
belong together, that the project exists, and that the Workspace is active and
not archived. `alera.list_tabs` enforces the same explicit scope.

Example:

```json
{
  "name": "alera.open_file",
  "arguments": {
    "projectId": "project-b",
    "workspaceId": "workspace-b",
    "relativePath": "lib/main.dart",
    "openMode": "new",
    "line": 35,
    "column": 8,
    "editor": "alera"
  }
}
```

External-editor example:

```json
{
  "name": "alera.open_file",
  "arguments": {
    "projectId": "project-b",
    "workspaceId": "workspace-b",
    "relativePath": "lib/main.dart",
    "editor": "vscode",
    "line": 120,
    "column": 8
  }
}
```

## Safety invariants

- The caller's Host-minted per-PTY token and live source Workspace still
  authorize every UI command. Target IDs do not impersonate a target Agent.
- Cross-project UI navigation is an allowlist. `alera.focus_agent`, settings
  changes, Agent output and Agent messaging remain constrained to the caller's
  original scope; no target PTY input is forwarded by navigation tools.
- Host validates the target's durable Project, Workspace and (for
  `select_tab`) Tab ownership before delivering an event to a local App.
  Unknown/removed/archived/mismatched targets fail closed.
- The App independently checks its live visible Workspace/Project collection.
  `select_tab` and file opening confirm the selected Tab after a rendered frame.
  A status of `accepted` is *not* `applied`; only the registered local App
  can acknowledge the command, and a `select_tab` acknowledgment must match
  the requested Tab ID.
- File paths remain workspace-relative and must exist within the resolved
  target Workspace, including symlink boundary checks. External editor
  launches repeat canonical path containment checks in the existing launcher.
- External execution is limited to supported editor kinds through Alera's
  preconfigured launcher. It never accepts an arbitrary executable or adds
  a target Agent/PTY capability. If no editor is installed or process startup
  fails, MCP returns an explicit error without switching to Alera.
- An explicit `windowId` is required when multiple Workbench windows are
  registered; no arbitrary window is selected silently.

## Red → green coverage

- `mcp::tests::tool_arguments_fail_closed_on_unknown_or_unbounded_values`:
  RED on the missing `alera.focus_workspace`, GREEN after registering the
  strict contracts.
- `WorkspaceTabService opening a requested new editor tab...`: RED on missing
  `forceNew`, GREEN on new independent Tab + unchanged default reuse.
- `mcp_cross_project_navigation_tests`: Host target, App ACK, foreign Tab,
  missing project, archived Workspace and Agent scope.
- `MCP cross-project navigation ACK follows visible Workspace and Tab`:
  Flutter Shell navigation with an actual Runtime Host event and a fake local
  App endpoint, checking both visible selection and rejected targets.

## Release status

This implementation belongs to the built-in MCP PR and does not by itself
change the previously documented MCP native-host/multi-platform release gates.
Run current PR checks and required native Host/PTY acceptance before merging
to `main` or shipping.
