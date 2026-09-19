# DI Refactor Review Checklist

- Does the module expose its required collaborators in its constructor?
- Are `Ref`, `WidgetRef`, `ProviderContainer`, provider globals, or static service lookups absent from ordinary application/domain services?
- Does every injected interface represent a capability the module actually uses?
- Can a broad dependency be split without duplicating the production implementation?
- Are I/O, time, process, RPC, Git, filesystem, notification, clipboard/window, and persistent-state boundaries injectable where substitution is useful?
- Are pure algorithms left as ordinary code rather than wrapped in one-method interfaces?
- Is there any giant `AppServices`/`Dependencies` bag hiding coupling?
- Is presentation holding cache, generation, cancellation, pagination, or request-lifecycle state that belongs in an application owner/controller?
- Can the main behavior be unit-tested without Riverpod or Flutter bindings?
- Do fakes implement only the capabilities needed by the test?
- Are provider tests limited to composition/wiring behavior?
- For Rust, does the handler depend on a narrow context/capability set instead of the complete actor/server when possible?
- Are shared transaction, process, and lifecycle ownership boundaries preserved?
- Did the refactor avoid creating circular feature dependencies?
- Did architecture guards, focused tests, and `git diff --check` pass?
