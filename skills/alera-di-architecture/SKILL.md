---
name: alera-di-architecture
description: Apply Alera's dependency-injection and modularity rules when creating or refactoring application services, controllers, owners, repositories, Git/runtime/file-system integrations, Riverpod providers, Rust request handlers, or tests. Use when a change should reduce coupling, make modules easier to unit test, split broad interfaces into capability ports, move dependency wiring to composition roots, or review whether a module depends on too much infrastructure.
---

# Alera DI Architecture

Use dependency injection to make ownership explicit and tests cheap without introducing a new DI framework.

## Core rules

1. Treat Riverpod as a composition root, not as a business-logic dependency.
   - Provider functions may `read`/`watch` other providers and construct services.
   - Ordinary application services, owners, coordinators, and domain types should not depend on `Ref`, `WidgetRef`, `ProviderContainer`, or provider globals.
   - A Riverpod `Notifier` may use Riverpod as part of its state-owner role, but delegate substantial logic to constructor-injected collaborators when that logic can be tested independently.

2. Prefer constructor injection.
   - Inject long-lived collaborators through constructors.
   - Make required dependencies explicit and immutable.
   - Avoid hidden lookups from service locators, globals, static singletons, and broad context objects.

3. Inject narrow capability ports, not giant backends.
   - Apply interface segregation before creating a fake for a broad interface.
   - Example: inject `GitDiffQueryPort` into a diff controller rather than the full `GitBackend` when only diff/blob operations are needed.
   - Production implementations may implement several ports on one concrete object.

4. Inject external effects and ownership boundaries.
   Good DI candidates include database/store access, filesystem I/O, Git, network/RPC clients, process execution, clocks, notifications, clipboard/window/OS integration, repositories, and mutable shared-state owners.

5. Do not DI pure computation only to satisfy a pattern.
   Keep parsers, mappers, formatters, DTOs, value objects, and pure algorithms as direct functions/types unless substitutability is actually required.

6. Do not create a giant `AppServices`, `Dependencies`, or service-locator bag.
   - Define feature-scoped dependency sets only when several collaborators always travel together.
   - Name them after the concrete owner, for example `WorkbenchCatalogDependencies`, not `CommonServices`.

## Dart/Flutter pattern

Prefer this direction:

```text
Presentation / Notifier
        -> application service / owner
        -> narrow port
        -> infra implementation
        -> Rust / RPC / OS / filesystem
```

Wire it in Riverpod:

```dart
@Riverpod(keepAlive: true)
WorkspaceGitHistoryController workspaceGitHistoryController(Ref ref) {
  return WorkspaceGitHistoryController(
    history: ref.watch(gitHistoryQueryPortProvider),
    mutations: ref.watch(gitHistoryMutationPortProvider),
  );
}
```

Keep the service Riverpod-free:

```dart
final class WorkspaceGitHistoryController {
  WorkspaceGitHistoryController({
    required GitHistoryQueryPort history,
    required GitHistoryMutationPort mutations,
  }) : _history = history,
       _mutations = mutations;

  final GitHistoryQueryPort _history;
  final GitHistoryMutationPort _mutations;
}
```

## Rust pattern

Do not add a DI framework. Pass the smallest required capabilities to domain handlers.

Prefer:

```rust
struct ProjectRequestHandler<'a> {
    store: &'a RuntimeStore,
    changes: &'a dyn RuntimeChangeBroadcaster,
}
```

or a narrow request context over making every handler depend on the complete `ServerActor`.

Keep transaction and ownership boundaries intact. Splitting an interface or `impl` block does not mean splitting the underlying SQLite pool, runtime host, or process owner.

## Refactor workflow

1. Identify the unit that owns the behavior.
2. List every dependency it actually uses.
3. Separate pure logic from effectful dependencies.
4. Replace broad dependencies with narrow ports where this materially reduces coupling.
5. Move provider/global lookups to the composition root.
6. Constructor-inject the resulting collaborators.
7. Move async/cache/cancellation state out of presentation widgets when it is application state.
8. Add direct unit tests using small fakes/stubs of the ports.
9. Add only a small number of provider wiring tests for composition correctness.
10. Run the relevant architecture guard, focused tests, and `git diff --check`.

## Testing standard

Prefer most application tests to instantiate the unit directly:

```dart
final controller = WorkspaceGitHistoryController(
  history: FakeGitHistoryQueryPort(),
  mutations: FakeGitHistoryMutationPort(),
);
```

Use `ProviderContainer(overrides: ...)` only when testing provider wiring or a Riverpod state owner itself. Use `ProviderScope` primarily for widget/integration tests.

A refactor is incomplete if tests still need to fake dozens of unrelated methods from a giant interface for a unit that uses only a few operations.

## Alera-specific priorities

- Git: split the broad backend into query/mutation capability ports and let one production backend implement multiple ports.
- Runtime RPC: route metadata and domain handlers should receive explicit capabilities; avoid turning `ServerActor` into a service locator.
- RuntimeStore: split domain `impl RuntimeStore` modules while preserving the shared pool and transaction boundaries.
- Workspace files: separate Explorer/quick-open/watch, editor I/O/conflict, mutation, and path-safety capabilities behind stable facades.
- Workbench: inject owner-specific dependencies instead of growing forwarding/service-locator layers.
- Agent hooks: preserve the existing per-agent adapters; do not re-centralize agent-specific policy through DI.

## Review checklist

Read `references/review-checklist.md` when doing an architecture review or before declaring a DI refactor complete.
