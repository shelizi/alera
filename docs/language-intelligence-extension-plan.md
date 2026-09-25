# Alera Language Intelligence / Parser Extension Plan

Date: 2026-09-20
Status: planning
Scope: optional per-language parser + semantic navigation architecture for the Alera editor

## 1. Goal

Alera should support language-specific code intelligence without making every language engine a mandatory background service.

The first supported language set is:

- C#
- Dart
- Python
- Rust
- Go
- PHP
- TypeScript / TSX
- JavaScript / JSX

The user must be able to enable or disable language intelligence per language, similar to choosing language extensions in an editor. Opening a file must not implicitly start every available external language server.

The first user-visible semantic feature is source navigation:

- go to definition / declaration;
- find references;
- go to implementation where the provider supports it;
- click a result to open the target file and select the target range.

The architecture must make adding a later language mostly a registration/provider task. Adding a language such as Kotlin must not require adding another language-specific `switch` to the editor surface, navigation controller, settings screen, or process owner.

## 2. Important terminology: parser is not enough for cross-file references

There are two different capability classes and Alera should keep them separate.

### 2.1 Structural parser

An in-process parser such as Tree-sitter is good for:

- syntax spans;
- outline/document symbols;
- folding;
- structural selection;
- bracket matching;
- cheap local syntax queries.

It generally does **not** know enough project/type/import/build-system semantics to reliably answer cross-file definition and reference queries by itself.

### 2.2 Semantic language engine

Cross-file navigation belongs behind a semantic provider, initially an LSP provider:

- definition/declaration;
- references;
- implementation/type definition;
- workspace symbols;
- hover;
- diagnostics;
- completion;
- rename;
- semantic tokens.

Therefore the top-level abstraction should be **Language Intelligence**, not a Tree-sitter-specific or LSP-specific API.

## 3. Current Alera / CodeForge baseline

The implementation can reuse substantial existing work rather than creating a second parser or protocol stack.

### 3.1 Existing native parser path

`NativeEditorDocument` already owns retained native parser state:

- Ropey document state and revisions;
- retained Tree-sitter `Parser` / `Tree`;
- incremental scalar edit deltas;
- viewport-bounded syntax queries;
- document symbols;
- folding ranges;
- structural bracket/selection queries;
- stale-revision rejection and explicit close.

Current built-in Tree-sitter grammars cover Dart, Rust, JavaScript/JSX, TypeScript/TSX, Python, JSON/JSONC, C#, Go, and PHP.

Relevant current implementation:

- `third_party/code_forge/rust/src/api/editor_document.rs`
- `third_party/code_forge/rust/Cargo.toml`
- `third_party/code_forge/lib/src/rust/api/editor_document.dart`

### 3.2 Existing LSP protocol path

CodeForge already implements an LSP client surface including:

- `textDocument/definition`;
- `textDocument/declaration`;
- `textDocument/typeDefinition`;
- `textDocument/implementation`;
- `textDocument/references`;
- document/workspace symbols;
- hover, code actions, completion, diagnostics, semantic tokens, etc.

Relevant current implementation:

- `third_party/code_forge/lib/LSP/lsp.dart`
- `third_party/code_forge/lib/LSP/lsp_stdio.dart`
- `third_party/code_forge/lib/code_forge/controller.dart`

### 3.3 Current activation problem

The normal Alera editor path passes a `languageId`, but does not pass an `lspConfig`:

- `lib/src/features/workbench/presentation/workspace_editor_surface.dart`

At the CodeForge widget boundary, a non-empty `languageId` currently calls `configureNativeSyntaxDocument(...)`. That means the language identifier is doing two jobs:

1. describing what language the file is;
2. implicitly enabling a native parser for that language.

These responsibilities need to be separated. Language detection must remain cheap metadata; parser/LSP activation must be controlled by provider policy and user settings.

## 4. Architectural rules

1. **Core code depends on capabilities, not language names.**
   - Editor/navigation code asks for `definition`, `references`, `symbols`, etc.
   - It never branches on `rust`, `python`, `csharp`, and so on.

2. **Language metadata is registry-driven.**
   - Extensions, aliases, display names, grammar provider and semantic provider are data/registration.

3. **Parser and semantic provider are independently replaceable.**
   - Tree-sitter is the initial structural provider.
   - LSP is the initial semantic provider.
   - Neither protocol is exposed as the application-domain interface.

4. **External language servers are opt-in and lazy.**
   - No server process starts at Alera startup just because support is compiled in.
   - Disabled language = no semantic process.
   - Enabled language starts on first document/query that needs it.

5. **Process/session lifetime belongs to an application owner, not widgets.**
   - Widgets must not own `Process`, restart timers, backoff, open-document sets, or server health state.

6. **Riverpod is composition only.**
   - The manager/controllers receive narrow constructor-injected ports.
   - No general-purpose service locator or giant `LanguageServices` bag.

7. **Third-party CodeForge stays behind adapters.**
   - Alera application/domain code must not depend on `LspConfig` as its public contract.
   - This leaves room to move LSP transport to Rust/runtime-host later without rewriting editor actions.

8. **Repository configuration cannot silently execute arbitrary commands.**
   - Executable paths and launch approval remain user/host-owned.
   - Project config may later express provider preference, but must not be an arbitrary executable injection path.

### 4.1 Compatibility with the runtime RPC / DI refactor

This plan must preserve the ownership rules in `docs/runtime-rpc-di-refactor-handoff.md` rather than creating a new path around them.

Hard boundaries:

- the local first release does **not** add language-intelligence persistence to `RuntimeStore`, `runtimeSettings.update`, or `ServerActor`;
- per-device language activation/provider/executable settings are local UI configuration in the first release;
- `LanguageIntelligenceManager` is a provider-session/lifecycle owner, not a service locator and not a persistence bag;
- application consumers receive narrow ports such as `LanguageNavigationPort`; they do not receive the full manager merely to reach unrelated capabilities;
- if remote-workspace language servers are added later, runtime RPC must use the existing typed route-policy/composition pattern and a dedicated narrow language-server owner/handler;
- a future runtime-host implementation must not add direct `RuntimeStore` reads to `requests.rs` merely to launch or query a language server;
- authentication, request projection and runtime event delivery may remain at the runtime actor boundary, while language-server process/session ownership belongs to the dedicated owner that actually owns those processes;
- ephemeral language-server process/session state should remain in memory unless a concrete restart/recovery requirement proves persistence is necessary.

This keeps the language feature orthogonal to the ongoing mobile/configuration/automation persistence cleanup. The current P0.1 mobile settings/pairing work can proceed independently.

## 5. Target architecture

```text
Workspace Editor / Command / References UI
                 |
                 v
       LanguageNavigationController
                 |
                 v
       LanguageIntelligenceManager
                 |
        +--------+---------+--------------------+
        |                  |                    |
        v                  v                    v
 NavigationPort       SymbolsPort         DiagnosticsPort ...
        |                  |                    |
        +------------------+--------------------+
                           |
                    Provider Session
                           |
              +------------+------------+
              |                         |
              v                         v
 TreeSitterStructuralProvider     LspSemanticProvider
 (CodeForge native adapter)       (CodeForge LSP adapter first)
              |                         |
              v                         v
       retained Rope/tree        LanguageServerRuntimePort
                                         |
                              local process now / runtime host later
```

The UI does not know which concrete provider answered a request. It only receives normalized Alera locations/results plus provider/status metadata for diagnostics and settings display.

## 6. Domain model

Create a dedicated feature instead of growing editor widgets with language-specific state:

```text
lib/src/features/language_intelligence/
  domain/
  application/
  infra/
  presentation/
```

### 6.1 `LanguageId`

Use a stable string-backed identifier rather than a closed enum so extensions can add languages without modifying a central enum.

Example canonical ids:

```text
csharp
python
rust
go
php
typescript
tsx
javascript
jsx
```

Aliases belong to descriptors, not scattered normalization switches.

### 6.2 `LanguageCapability`

Initial capabilities:

```text
syntax
documentSymbols
folding
definition
declaration
typeDefinition
references
implementation
workspaceSymbols
hover
diagnostics
semanticTokens
completion
rename
formatting
```

Capabilities are declarative metadata. A provider can support only a subset.

### 6.3 `SourceLocation`

All provider-specific positions must normalize to one Alera domain type:

```text
SourceLocation
  workspaceId
  path
  range

SourceRange
  start: SourcePosition
  end: SourcePosition

SourcePosition
  line
  scalarColumn
```

LSP UTF-16 columns and URIs are converted at the LSP adapter boundary. Tree-sitter byte/scalar locations are converted at the native adapter boundary. The workbench should never need to know the provider's offset encoding.

### 6.4 `LanguageExtensionDescriptor`

The extension descriptor is data, not behavior:

```text
LanguageExtensionDescriptor
  id
  displayName
  fileExtensions
  aliases
  parserProviderId?
  semanticProviderIds[]
  defaultSemanticProviderId?
  capabilities
```

This is the Alera equivalent of the metadata side of an editor extension.

### 6.5 `LanguageProviderDescriptor`

```text
LanguageProviderDescriptor
  id
  kind: structuralParser | semanticServer
  languages[]
  capabilities
  processScope
  launchPolicy
  executableResolutionPolicy?
```

`processScope` should allow at least:

- `document` only for future special cases;
- `workspace` as the normal LSP scope;
- `sharedWorkspaceFamily` for providers that serve JS + TS together.

Do not assume one process per file.

## 7. Narrow capability interfaces

Avoid one giant interface where every provider must fake dozens of methods.

Recommended ports:

```text
LanguageNavigationPort
  definition(...)
  declaration(...)
  typeDefinition(...)
  implementation(...)
  references(...)

LanguageSymbolsPort
  documentSymbols(...)
  workspaceSymbols(...)

LanguageDiagnosticsPort
  diagnostics(...)

LanguageHoverPort
  hover(...)

LanguageDocumentSessionPort
  open(...)
  applyEdits(...)
  save(...)
  close(...)
```

Provider objects expose only the capability ports they actually implement. The registry records capabilities so unsupported calls fail before reaching the provider.

## 8. Registry and extension mechanism

### 8.1 Phase 1: compiled-in extension registry

Start with a compiled-in registry because it gives the desired abstraction without immediately creating a package downloader/security surface.

```text
LanguageExtensionRegistry
  registerLanguage(descriptor)
  registerProvider(descriptor, factory)
  languageForPath(path)
  providersFor(language, capability)
```

Initial language modules should each register themselves through the same contract:

```text
languages/csharp.dart
languages/python.dart
languages/rust.dart
languages/go.dart
languages/php.dart
languages/typescript_javascript.dart
```

The editor surface sees only the registry.

### 8.2 Phase 2+: package-backed extensions

Once the core contract is stable, the same descriptors can be loaded from signed/managed extension packages. Dynamic package loading is deliberately not required for the first delivery.

The important architectural test is that replacing the compiled registry with a package-backed catalog does not change navigation or editor APIs.

## 9. Enablement and settings

Language-intelligence settings deserve their own feature settings rather than being mixed with tab size/theme/process-unrelated editor fields.

Proposed shape:

```text
LanguageIntelligenceSettings
  languages: Map<LanguageId, LanguageActivationSettings>

LanguageActivationSettings
  enabled
  structuralParserEnabled
  semanticProviderId?
  executablePath?
  extraArgs[]
```

Recommended defaults:

- semantic engine: **off** for every language until the user enables it;
- structural parser: preserve current syntax behavior initially, but gate the retained/heavy native parser behind an explicit provider policy so `languageId` alone no longer means "start parser";
- first-release `enabled`, `structuralParserEnabled`, `semanticProviderId`, executable override and extra args: local/per-device only;
- do not project these first-release fields through `runtimeSettings.update` or portable configuration;
- repository/project files must not be allowed to inject arbitrary executable paths.

If a later product requirement wants provider preference to roam between machines, split the portable preference from local activation/executable state explicitly. Do not make the entire language-intelligence settings object runtime-operational just to gain synchronization.

If a simpler first UI is preferred, expose one top-level language toggle first and keep the parser/server sub-settings in the model for later:

```text
[ ] C#
[x] Dart        provider: dart.analysis-server   status: Ready
[x] Rust        provider: <selected provider>   status: Ready
[x] Python      provider: <selected provider>   status: Missing executable
[ ] Go
[ ] PHP
[x] TypeScript / JavaScript
```

The settings UI belongs under **Settings -> Editor -> Language Intelligence**, but its state/domain remains in the dedicated language-intelligence feature.

## 10. Activation state machine

Per `(workspace, provider)` runtime state:

```text
disabled
available
resolvingExecutable
starting
initializing
ready
missingExecutable
failed
stopping
```

Rules:

1. Registering support never starts a process.
2. Opening an enabled language document may request a provider session.
3. The first request creates one workspace-scoped session.
4. Other tabs reuse that session.
5. Documents are reference-counted/owned by the session.
6. Closing the last relevant document schedules idle shutdown.
7. New activity cancels idle shutdown.
8. Provider crash moves to `failed`; bounded restart/backoff is owned by the manager.
9. Disabling a language closes documents and stops the owned server.
10. A stale request is discarded by document/workspace generation before it reaches presentation state.

No process lifecycle timer or open-document map should live in `WorkspaceEditorSurface`.

## 11. Language server runtime boundary

Define a narrow runtime port:

```text
LanguageServerRuntimePort
  resolveExecutable(provider, settings, target)
  start(provider, workspaceRoot, environment)
  stop(session)
  observeExit(session)
```

The first adapter may wrap the existing CodeForge stdio LSP path. Do not expose `Process` or `LspStdioConfig` above the infra adapter.

For the local MVP this port is implemented entirely on the desktop side and therefore requires no `terminal_host/server/requests.rs` changes.

This also gives Alera a clean future path for remote workspaces:

- MVP may support local workspace processes only;
- later a runtime-host adapter can launch the same provider next to the remote workspace through a typed, narrow RPC surface;
- the application/navigation layer remains unchanged.

When that remote adapter is introduced, follow the runtime RPC/DI refactor rules: keep request auth/projection at the actor boundary, inject the smallest useful language-server capability, and keep process/session lifecycle in a dedicated owner rather than turning `ServerActor` into a language-service locator.

Remote unsupported state should be explicit rather than silently running a local server against inaccessible remote paths.

## 12. Structural parser provider

### 12.1 Refactor current hard-coded grammar switch

Move the grammar mapping in `editor_document.rs` out of the native-document implementation into a small registry/module, for example:

```text
third_party/code_forge/rust/src/api/editor_languages.rs
```

The native document should ask the registry for:

```text
NativeLanguageDescriptor
  canonical_id
  grammar
  highlight_query
  aliases
```

This removes language-list growth from the parser/document owner.

### 12.2 First grammar expansion

Already present:

- Python
- Rust
- TypeScript/TSX
- JavaScript/JSX

Add and validate maintained Tree-sitter grammar/query integrations for:

- C#
- Go
- PHP

Grammar addition should not require changes to workbench presentation code.

## 13. Semantic provider adapter

Create an Alera-facing adapter around the existing LSP client.

Responsibilities:

- initialize/stop the transport;
- normalize capabilities;
- normalize URI/path results;
- convert LSP UTF-16 positions to Alera scalar offsets;
- document open/change/save/close sync;
- convert definition/reference responses into `SourceLocation`;
- reject responses from stale document/workspace generations;
- expose health/error metadata without leaking raw transport objects.

The existing `CodeForgeController` LSP helpers are useful implementation material, but the long-term owner should be the language-intelligence feature rather than a widget/controller-specific `LspConfig` field.

## 14. Navigation behavior

### 14.1 Go to definition

Flow:

```text
pointer/key command
  -> current document + scalar position
  -> LanguageNavigationController.definition
  -> manager resolves enabled ready provider
  -> semantic provider request
  -> normalized SourceLocation(s)
  -> workspace navigation service opens target tab
  -> editor selects range + scrolls into view
```

If exactly one location exists, navigate directly. If multiple locations are returned, show a lightweight picker/list rather than silently selecting an arbitrary result.

### 14.2 Find references

References should use a dedicated result model and panel, not a huge inline context-menu payload.

```text
ReferenceResult
  locations[]
  truncated
  providerId
```

UI behavior:

- group by file;
- show relative path + line preview;
- clicking opens the file and selects the range;
- support cancellation when a newer reference query starts;
- design result loading so paging/streaming can be added if a very large workspace returns many hits.

### 14.3 Commands / gestures

Initial wiring can include:

- context menu: Go to Definition;
- context menu: Find References;
- keyboard action registry entries for Go to Definition / Find References;
- Ctrl/Cmd+Click once pointer-token hit testing is confirmed reliable.

Keep commands provider-neutral. Do not add `rustGoToDefinition`, `pythonGoToDefinition`, etc.

## 15. Per-language provider packages

Each first-wave language module should contain only language-specific registration/configuration.

Example module responsibility:

```text
RustLanguageExtension
  file extensions / aliases
  Tree-sitter grammar id
  semantic provider descriptors
  language-server initialization/config defaults
```

The concrete external semantic server for each language should be selected through a separate compatibility/smoke-test gate. Server choices are not hard-coded into the core abstraction and can later be replaced or offered as alternatives without modifying navigation code.

TypeScript and JavaScript should be allowed to share one workspace-scoped semantic provider session while retaining distinct canonical language ids.

## 16. Error and fallback policy

Provider failure must not make the text editor unusable.

Rules:

- missing semantic provider -> editor continues as a normal text editor;
- semantic provider crash -> syntax/editor remains usable;
- unsupported semantic feature -> command disabled or returns an unobtrusive unsupported state;
- Tree-sitter unsupported/failed -> existing non-native syntax fallback remains available;
- stale results -> silently discarded, never applied to a different revision;
- user disables extension -> active queries cancelled and server stopped;
- malformed external result -> fail the query, not the editor session.

## 17. Security and trust boundary

Managed acquisition is allowed only for Alera-curated semantic providers. A
workspace/project must never be able to supply an arbitrary package, URL,
version, executable, or post-install command for automatic execution.

Executable resolution order can be:

1. explicit per-device user override;
2. known executable on `PATH`;
3. Alera-managed pinned provider install;
4. unavailable state with setup guidance.

Managed provider acquisition rules:

- the provider id must map to an Alera-owned catalog entry;
- package/module/tool versions are pinned in source control;
- npm installs run with lifecycle scripts disabled and require lockfile SRI
  integrity metadata for every curated package;
- Go installs use the public checksum database, rust-analyzer uses rustup's
  signed/checksummed distribution metadata, and .NET tools use NuGet/dotnet
  package integrity;
- after installation Alera records the executable SHA-256 and verifies it
  before every reuse; a mismatch forces a clean reinstall;
- installation uses an application-support `language-servers/` directory,
  not the project tree;
- concurrent requests for the same provider share one install operation and a
  filesystem lock prevents competing processes from installing over each
  other;
- Alera does not bootstrap ecosystem runtimes as part of semantic-server
  acquisition. Before installing a missing managed provider it preflights the
  required local environment: Node.js + npm, Go, a real .NET SDK (not only the
  runtime), or rustup. Missing/broken prerequisites remain a `Missing` state
  with actionable setup guidance and a `Check Again` path;
- acquisition failures are surfaced as Missing/Failed provider state without
  making the editor unusable.

The initial managed catalog covers C# (`csharp-ls`), Go (`gopls`), Python
(`pyright`), Rust (`rust-analyzer`), PHP (`intelephense`), and TypeScript /
JavaScript (`typescript-language-server` + `typescript`).

Project configuration may identify a language/provider preference, but should not be able to provide an arbitrary command line that executes automatically on project open.

## 18. Suggested code boundaries

New Alera files/modules:

```text
lib/src/features/language_intelligence/domain/language_id.dart
lib/src/features/language_intelligence/domain/language_capability.dart
lib/src/features/language_intelligence/domain/language_extension_descriptor.dart
lib/src/features/language_intelligence/domain/language_provider_descriptor.dart
lib/src/features/language_intelligence/domain/source_location.dart
lib/src/features/language_intelligence/domain/language_intelligence_settings.dart

lib/src/features/language_intelligence/application/language_intelligence_manager.dart
lib/src/features/language_intelligence/application/language_navigation_controller.dart
lib/src/features/language_intelligence/application/language_provider_registry.dart

lib/src/features/language_intelligence/infra/code_forge_lsp_provider.dart
lib/src/features/language_intelligence/infra/code_forge_tree_sitter_provider.dart
lib/src/features/language_intelligence/infra/language_server_runtime.dart

lib/src/features/language_intelligence/presentation/language_intelligence_settings_group.dart
lib/src/features/language_intelligence/presentation/references_panel.dart
```

Native/CodeForge refactor:

```text
third_party/code_forge/rust/src/api/editor_languages.rs
third_party/code_forge/rust/src/api/editor_document.rs
third_party/code_forge/rust/Cargo.toml
third_party/code_forge/lib/LSP/*
third_party/code_forge/lib/code_forge/controller.dart
```

Workbench touch points should remain small:

```text
lib/src/features/workbench/presentation/workspace_editor_surface.dart
lib/src/features/workbench/presentation/workspace_editor_widgets.dart
lib/src/features/keyboard/domain/keyboard_action.dart
lib/src/features/settings/presentation/panes/editor_pane.dart
```

The workbench files should only wire generic actions/state. They must not acquire language-specific launch logic.

## 19. Implementation phases

Implementation status (2026-09-20): Phase A is implemented on
`feature/language-intelligence-phase-a` as pure Dart contracts. The settings
model is intentionally not wired into persistence/UI yet, and no parser or
semantic provider process behavior changes are part of this phase.

### A. Core contracts and registry

Deliverables:

- language/capability/source-location domain types;
- extension/provider descriptors;
- registry;
- per-language settings model;
- fake provider for tests;
- architecture guard proving the editor is not branching on first-wave language ids.

No provider process or UI navigation behavior changes in this phase.

### B. Explicit parser activation boundary

Deliverables:

- separate `languageId` metadata from native parser enablement;
- native structural-provider adapter;
- refactor Tree-sitter grammar registration out of `NativeEditorDocument`;
- preserve existing parser/folding/symbol/highlight behavior under explicit policy;
- add C#/Go/PHP grammar registrations after the registry exists.

### C. Semantic provider lifecycle owner

Deliverables:

- `LanguageServerRuntimePort`;
- workspace-scoped provider sessions;
- lazy start / document attach / detach / idle stop;
- executable resolution/status;
- cancellation/generation handling;
- first CodeForge stdio-LSP adapter;
- default-off semantics;
- local MVP has no `RuntimeStore` or `ServerActor` dependency;
- remote runtime-host support is a later adapter and must use the typed/narrow RPC composition rules from the runtime DI refactor.

### D. Navigation abstraction and editor integration

Deliverables:

- normalized definition/declaration/implementation/reference calls;
- open-target-file + range selection service;
- context menu + keyboard commands;
- multi-definition picker;
- references panel;
- no direct LSP parsing inside `WorkspaceEditorSurface`.

### E. Settings / extension UX

Deliverables:

- Editor -> Language Intelligence group;
- per-language enable toggle;
- provider selector when multiple providers exist;
- Ready / Missing / Failed / Disabled status;
- per-device executable override;
- stop server immediately when disabled.

### F. First-wave language rollout

Run one lane per language after A/C contracts stabilize:

```text
F1 C#
F2 Python
F3 Rust
F4 Go
F5 PHP
F6 TypeScript + JavaScript
F7 Dart
```

Each lane validates:

- file detection/aliases;
- structural parser where supported;
- chosen semantic provider startup;
- document sync;
- definition;
- references;
- cross-file target opening;
- Unicode position conversion;
- missing-provider behavior;
- workspace shutdown/restart.

### G. Extensibility gate

Add a synthetic/mock language extension in tests.

Pass condition: adding the mock language requires only:

- a language descriptor;
- optional parser/provider registration;
- provider-specific tests.

It must **not** require editing:

- `WorkspaceEditorSurface` language branches;
- `LanguageNavigationController` language branches;
- settings UI language switches;
- process lifecycle switches.

This is the main guard against slowly rebuilding a monolithic language switch table.

## 20. Parallel work split

After Phase A establishes the contracts, these can proceed mostly in parallel:

| Lane | Work | Depends on |
| --- | --- | --- |
| P1 | Tree-sitter registry refactor + explicit activation | A |
| P2 | C#/Go/PHP Tree-sitter grammar additions | P1 registry contract |
| P3 | LSP runtime/process/session owner | A |
| P4 | CodeForge LSP adapter + normalized locations | A, P3 runtime contract |
| P5 | Settings/extension UI | A settings/descriptor contract |
| P6 | Navigation/open-target/references UI | A, P4 navigation contract |
| P7 | C# semantic provider lane | A, P3/P4 |
| P8 | Python semantic provider lane | A, P3/P4 |
| P9 | Rust semantic provider lane | A, P3/P4 |
| P10 | Go semantic provider lane | A, P3/P4 |
| P11 | PHP semantic provider lane | A, P3/P4 |
| P12 | TS/JS shared semantic provider lane | A, P3/P4 |
| P13 | Extensibility + architecture guards | A, integration complete |
| P14 | Dart semantic provider lane | A, P3/P4 |

P1, P3, and P5 can start in parallel once A is merged. Language rollout lanes P7-P12 and P14 can then run independently against the stable provider contract.

## 21. Testing strategy

### Domain/application unit tests

- path -> language descriptor resolution;
- alias normalization;
- enabled/disabled provider resolution;
- provider capability selection;
- lazy start exactly once per workspace/provider;
- multiple tabs reuse one process;
- last document idle shutdown;
- disabling cancels/stops;
- crash/restart state transition;
- stale generation result rejection;
- multiple definition result handling;
- large reference result truncation/paging contract.

Use narrow fakes. These tests should instantiate manager/controllers directly without `ProviderContainer`.

### Adapter tests

- UTF-16 <-> scalar conversion with astral Unicode;
- URI -> workspace-safe path normalization;
- LSP response variants: single `Location`, array, location links where supported;
- references result normalization;
- server initialize/shutdown/exit order;
- document open/change/save/close ordering.

### Native parser tests

- descriptor/grammar registration for all enabled grammars;
- existing syntax/symbol/folding tests remain green;
- unsupported parser language does not start/create retained parser state;
- explicit parser disable means `languageId` alone cannot create retained parser state.

### Widget/integration tests

- disabled language shows no semantic process/status Ready;
- enable language -> first relevant document starts provider;
- Go to Definition opens correct file/range;
- Find References list navigates correctly;
- provider missing/failure keeps editor usable;
- changing setting while tabs are open cleanly attaches/detaches provider.

## 22. Performance constraints

- no global LSP startup during application bootstrap;
- one normal semantic server per `(workspace, provider)` rather than per tab;
- no full-document copy on every navigation query;
- document synchronization reuses existing edit/revision flow where possible;
- Tree-sitter viewport queries stay bounded;
- references can later stream/page rather than force one giant Flutter object graph;
- provider status changes should not rebuild the full workbench tree.

Measure separately:

- provider cold start;
- initialize -> ready;
- first definition latency;
- warm definition latency;
- references latency/count;
- memory per active workspace provider;
- idle-shutdown memory recovery.

## 23. Definition of done for the first release

The architecture is ready when all of the following are true:

1. C#, Dart, Python, Rust, Go, PHP, TypeScript and JavaScript are represented only through registry descriptors/provider modules, not editor `switch` statements.
2. Every language can be independently enabled/disabled in Settings.
3. Disabled semantic engines spawn no language-server process.
4. Enabling an engine starts it lazily when needed and shares it across tabs in the workspace.
5. Go to Definition works across files for each supported semantic provider.
6. Find References returns a navigable file/range list for each supported semantic provider.
7. Provider failure does not break editing or basic syntax highlighting.
8. Existing Tree-sitter retained-document performance/correctness paths remain intact.
9. C#/Go/PHP structural grammars can be added without workbench presentation changes.
10. A mock additional language passes the extensibility gate without modifying editor/navigation/process-owner language switches.

## 24. Recommended implementation order

Do not begin by wiring eight language servers directly into `WorkspaceEditorSurface`.

The safe order is:

```text
A contracts/registry
  -> P1 parser activation + P3 lifecycle + P5 settings in parallel
  -> P4 LSP adapter
  -> P6 generic navigation
  -> P7-P12 languages in parallel
  -> P13 extensibility/performance gate
```

This order deliberately spends the first batch on the extension boundary. Once it is stable, every new language becomes much cheaper and no longer requires invasive editor changes.

## 25. Implementation status — 2026-09-21

The generic architecture is now implemented on
`feature/language-intelligence-phase-a`.

Completed:

- **A — contracts / registry:** string-backed language ids, language/provider
  descriptors, capability-based lookup, sparse per-language settings, and
  provider-neutral source locations are in place.
- **B / P1 — structural parser activation:** language metadata no longer has to
  imply parser activation; retained native parser activation is controlled by
  the language policy/descriptor path.
- **P2 — first-wave parser expansion:** C#, Go, and PHP Tree-sitter grammars are
  registered alongside the existing Dart/Rust/JavaScript/TypeScript/TSX/Python/
  JSON grammars. Representative C#/Go/PHP sources and highlight queries are
  covered by native tests.
- **C / P3 — semantic process/session owner:** semantic providers are lazy,
  workspace-scoped, shared across documents, bounded-restart, and stop when
  detached/disabled according to the session policy.
- **P4 — CodeForge semantic adapter:** document sync, UTF-16/scalar conversion,
  definition/reference normalization, URI/path normalization, and provider-
  neutral navigation are implemented.
- **Language-server client-request compatibility:** the standalone runtime now
  answers server-initiated requests before and after initialization instead of
  relying on `CodeForgeController`. This includes
  `workspace/configuration`, dynamic registration, progress/refresh requests,
  workspace folders, safe rejection of workspace edits/show-document, and
  JSON-RPC method-not-found for unsupported requests.
- **D / P6 — generic navigation:** Go to Definition can open/reveal cross-file
  targets, multiple definitions can be selected, and Find References has a
  workspace-scoped grouped result panel with exact-range navigation and stale-
  query suppression. `F12` and `Shift+F12` route the active editor session to
  Go to Definition and Find References through the generic keyboard command
  registry. These paths are capability/provider driven and do not branch on
  concrete language ids.
- **Python semantic servers:** the default is pyrefly 1.3.1, with ty 0.0.84 and pyright 1.1.414 selectable. Measured on a 7,032-file project (hermes): for a widely used function, pyright, ty and pyrefly reached 96%, 95% and 94% of lexical occurrences (the misses are dynamic module access none of them resolves); pyrefly settled in about 6 seconds with 0.14-second warm queries, ty blocked its first query for 37 seconds, pyright answered warm queries in 1.2 seconds. Only pyrefly resolved overrides reached through untyped call sites, and it returned exactly the implementations of an abstract method in 0.1 seconds; pyright has no `textDocument/implementation`. pyrefly and pyright answer from a partial index for the first few seconds after a file opens and neither reports indexing progress, so an early Find References can be incomplete; pyrefly's `--indexing-mode lazy-blocking` and a raised `--workspace-indexing-limit` changed neither the early answer nor the final count, so both keep their defaults. pyrefly and ty install through the `githubRelease` recipe kind: a per-platform archive pinned by URL and SHA-256 in `managed_language_server_catalog.dart`, from which only the pinned executable member is extracted.
- **Go to Implementation:** `Mod+F12`, the editor context menu, and a primary click with the platform navigation modifier (Cmd on macOS, where Ctrl+click is the secondary click; Ctrl elsewhere) route to `textDocument/implementation`. The click falls back to Go to Definition when the server has no implementation support or finds none, so it still navigates from a plain call or a variable. Keyboard and menu entries are offered for providers that declare `LanguageCapability.implementation`. Verified on Windows against fixture projects: csharp-ls 0.28.0, the Dart analysis server, rust-analyzer, gopls and typescript-language-server 6.0.0 each return the two implementations of an interface method. Pyright 1.1.414 rejects the method (`-32601`) and intelephense 1.18.5 (free tier) advertises no `implementationProvider`, so those two keep definition and references only; phpactor is undeclared until it is verified. Navigation now reports a server that is still starting as not ready, a server that never answers within 30 seconds as not responding, and a JSON-RPC error as a failure, instead of an empty result. Result locations are matched to the workspace through `path_identity.dart`, because servers report `e:\...` while legacy roots were stored as `\\?\E:\...`, which previously discarded every result.
- **E / P5 — Settings UX:** each first-wave language has optional semantic
  enablement, provider selection, executable override, and status display.
- **F4 / P10 — Go semantic compatibility gate:** validated locally on Windows
  with `gopls v0.21.1` through the production
  `CodeForgeLanguageServerRuntime` +
  `CodeForgeSemanticProviderAdapter` path. The smoke covered initialize,
  `didOpen`, cross-file definition, references returning both declaration and
  call site, graceful shutdown, and process exit.
- **F1 / P7 — C# semantic compatibility gate:** validated locally on Windows
  with `csharp-ls 0.28.0` and .NET SDK 10.0.401 through the built-in
  `csharp.csharp-ls` descriptor and production runtime/adapter path. The
  smoke covered project restore/build, initialize, `didOpen`, cross-file
  definition, references spanning declaration and call site, graceful shutdown,
  and process exit.
- **F2 / P8 — Python semantic compatibility gate:** validated locally on
  Windows with `pyright 1.1.414` through the built-in `python.pyright`
  descriptor and production runtime/adapter path. The smoke covered
  initialize, `didOpen`, cross-file definition, references, graceful shutdown,
  and a real npm `.cmd` executable shim.
- **F3 / P9 — Rust semantic compatibility gate:** validated locally on Windows
  with `rust-analyzer 1.97.1` through the built-in `rust.rust-analyzer`
  descriptor and production runtime/adapter path. The smoke covered
  initialize, `didOpen`, cross-file definition into `lib.rs`, references
  spanning the declaration and call sites, graceful shutdown, and process exit.
- **F5 / P11 — PHP semantic compatibility gate:** PHP now defaults to the
  cross-platform `php.intelephense` provider while retaining
  `php.phpactor` as an optional provider. `intelephense 1.18.5` was
  validated locally on Windows through the production runtime/adapter path,
  covering initialize, `didOpen`, cross-file definition, references, graceful
  shutdown, and a real npm `.cmd` executable shim. The current Phpactor
  release requires Linux/macOS (Windows users are directed to WSL), and its
  native Windows PHAR fails its required `ext-posix` check, so it is not a
  portable default for Alera.
- **F6 / P12 — TypeScript/JavaScript shared semantic compatibility gate:**
  validated locally on Windows with `typescript-language-server 6.0.0` and a
  disposable TypeScript 5.9.3 workspace. One server session handled both
  `typescript` and `javascript` documents, including cross-file definition
  and references. Standard LSP Definition may first return an import alias, so
  the provider-neutral semantic adapter now follows a bounded single-target
  same-file definition chain with cycle protection before returning the final
  target.
- **F7 / P14 — Dart semantic compatibility gate:** validated locally on Windows
  with Dart SDK `3.13.2` through the built-in `dart.analysis-server`
  descriptor and production runtime/adapter path. The smoke covered
  `dart language-server --protocol=lsp`, initialize, `didOpen`, cross-file
  definition, references spanning declaration and call site, graceful
  shutdown, and the Windows `dart.bat` executable shim.
- **Windows executable compatibility:** PATH discovery now returns the concrete
  executable path, prefers PATHEXT shims over extensionless npm siblings, and
  starts `.cmd` / `.bat` language-server shims through the Windows shell.
  This keeps custom per-session PATH environments consistent between readiness
  probing and real process startup.
- **G / P13 — extensibility gate:** a synthetic `moon` language is registered
  only through a contribution/descriptor/provider in tests and successfully
  runs generic lazy session startup, document sync, definition, references, and
  detach. The architecture guard also covers the Definition/References editor
  integration surfaces.

All first-release semantic validation lanes are now complete for C#, Dart,
Python, Rust, Go, PHP, TypeScript, and JavaScript.

Known status semantics:

- Settings **Ready** currently means that the configured executable resolves
  from the explicit override/PATH probe; it intentionally does **not** start a
  language server just to verify readiness, preserving lazy-start behavior.
- A PATH shim/proxy can therefore still fail when the first real session is
  started. Runtime initialization errors are captured by the workspace session
  state as **Failed** with the underlying error.
