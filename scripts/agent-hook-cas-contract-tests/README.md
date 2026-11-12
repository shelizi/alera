# Alera Agent Hook CAS contract regression

This tiny, independent Cargo package compiles **the exact**
`rust/alera-cli/src/agent_hook_cas_contract.rs` source imported by the real Alera
CLI. It tests the secret-free Host readback projection, the two-field
`runtimeSettings.update` payload, stale revisions, old provider values,
no-ops, changed unrelated providers, input limits and revision exhaustion.

Run from the repository root:

```sh
cargo test --offline --locked --manifest-path scripts/agent-hook-cas-contract-tests/Cargo.toml
```

The GPUI experimental CI runs it on Windows, macOS and Linux. This test
**does not** start a Host, mutate RuntimeStore, exercise Host's SQLite
`BEGIN IMMEDIATE` transaction or validate a real Windows GUI/Toast. Those
remain separate H6/H7 acceptance gates. The full CLI is type-checked by its
own Cargo build; this package exists to make the security-critical pure
contract executable independently of the very large CLI test binary.
