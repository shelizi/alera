# Antigravity (AGY) multi-account: safety contract and staged implementation

Status: **staged implementation; real independent AGY login and multi-account
launch are NOT implemented**.
Date: 2026-11-11. Branch: `feat/agy-multi-account-20261111`.

## Continuation (2026-11-14)

The feature branch now includes the later `main` History Sync implementation
and its shared Classic/GPUI Host policy. No new native history format or
credential-copying mechanism was introduced.

### Host-owned registrations and Native UI (second stage)

The Rust CLI now offers `alera runtime --json agy-accounts get|upsert|remove`
against an **existing** Host only. Every mutation requires a matching
`expectedRevision`, is rejected on an older Host, and confirms the resulting
AGY list and group policy from the Host reply. It does not launch AGY, attempt
OAuth, read credentials or start a Host.

The Host's capability response deliberately reports
`credentialScope=host-os-user`, `accountAuthentication=unverified`,
`supportsIndependentLocalAccounts=false`, and
`supportsAccountSwitching=false`. Registration is metadata only; per-account
credential isolation and Google account identity verification remain a
separate A1 gate. The recommended integration is a **separately authenticated
OS-user Runtime Host** per independent local login, not a rewritten keyring
entry or a modified `USERPROFILE`.

GPUI Shared Agent History now reads both Host revisions and includes Add,
Edit and confirmed Remove for Google registrations. Edits run through the
same versioned Host RPC as Classic; a stale revision never silently overwrites
another client. Existing PTYs and the default AGY login are untouched.
Classic also cleans only removed AGY group metadata, while keeping unrelated
Codex/Claude groups.

### New AGY account selection and safe launch boundaries (third stage)

Classic settings now persist `selectedAgyAccountId` for each Host (default:
`default`) alongside the existing Codex/Claude selected profile. The Google
account registration list includes a selector and a clear warning that
additional registered accounts are **not** signed in. Removing a selected
registration resets the choice to Default. New terminal launch environment
hints carry `ALERA_ACCOUNT_AGY_ID` only for valid registered selections.

No additional Google account is allowed to masquerade as the default login:

- Classic New Agent tab creation checks the selected AGY account **after**
  loading persisted settings and before asking the Host to create a tab.
- Prompt Workspace `agentProfile.launch` requests check the same selected
  account in Classic before contacting even an older, unaware Runtime Host.
- The updated Runtime Host refuses an explicit non-default `agyAccountId`
  at `agentProfile.launch` before creating an idempotency receipt, tab or PTY.
- For managed AGY terminal creation the Host rejects
  `ALERA_ACCOUNT_AGY_ID != default` before PTY startup and strips this
  Alera-only hint for generic shells and other providers.

Only Default is launchable on the current Windows OS-user keyring. This stage
does **not** supply a second Google OAuth login, cross-account native resume,
or a verified separate-user Host router. GPUI account registration works but
GPUI per-account new-Agent selection remains gated on a verified backend.

### Actual Runtime Host OS identity diagnostic (fourth stage)

An authenticated `runtimeSettings.get` from the **running Host process** now
includes `hostOsIdentity.kind` and `hostOsIdentity.subjectHash`. On Windows,
the Host gets its security principal SID from the current process access token
(`OpenProcessToken` / `GetTokenInformation` / `TokenUser`), not from
`USERPROFILE`, `HOME` or a spoofable username. A domain-separated SHA-256
digest is published instead of the raw SID. Unix uses effective UID (only
useful within the corresponding OS instance). Unsupported or failed lookups
are shown as null, never silently considered authenticated.

`alera runtime --json agy-accounts get` passes that Host-derived identity and
`hostOsUserProof=os-token-digest` through to Native GPUI, which validates the
type, digest and proof state, and shows a short read-only fingerprint.

Two Host runtime directories under one Windows user will report the **same
fingerprint**, correctly showing that they cannot isolate AGY's single
`gemini:antigravity` keyring entry. A different fingerprint is only evidence
of a different local OS user, **not** proof of an independently authenticated
Google account or secure cross-user Host pairing. No token/keyring operations
or OS-user creation are performed.

Remaining: provision a second real OS user, establish least-privileged
cross-user transport and ownership verification, and route AGY quota plus
new Agent PTYs to that verified Host. The existing Classic client has one
local Runtime Host socket, so the routing layer is genuinely not present
yet. Do not loosen the safety gate until it is.

### Loopback SSH independent-user account backend (fifth stage, feature branch)

The original missing remote-user routing is now implemented in the isolated
worktree, reusing the existing Runtime SSH Target catalog instead of inventing
an unscoped cross-user Host control token. `runtime agy-accounts bind` pins a
registered AGY account ID to an existing **loopback, public-key-only SSH
Target**, after checking its SSH host key via OpenSSH's strict known-hosts
policy and comparing the remote Windows user's token-derived identity to the
real local Host OS user. `unbind` clears only the binding. Both use the
revisioned Host settings update and reject old Hosts.

The Host re-probes the remote OS identity before each AGY quota fetch and
each new AGY Terminal PTY. Bound-account quota fetches read the separately
logged-in user's existing Host's `agy:default` quota and remap only the
resulting display account ID; they never fetch the local default quota for
the bound account. A new AGY Tab stores its account ID and uses interactive
SSH in the second Windows user's shell, first selecting the original workspace
directory using an encoded PowerShell command. The remote user must have
access to that directory. Failed SSH login/identity/working-directory checks
never silently fall back to the local Google account.

Classic New Agent and Prompt Workspace check `agySshRoutingVersion=1` and
an actual paired profile before requesting the new Agent. Native GPUI and
Classic show the paired target; pairing commands, host-key authorization and
real Google login are documented in
`docs/agy-ssh-isolated-windows-host-20261114.md`.

**Real deployment is still unverified and disabled on this machine**: the
inspected Windows host has `ssh.exe`, but no installed `sshd` service or
configured second Windows user. Neither Google credentials nor OS account
permissions were changed, and no SSH listener was installed. The backend
source must pass the current build and unit tests before any claim of
release readiness or merge to `main`.

Fifth-stage verification on Windows:

- `cargo check --offline -p alera-cli --bin alera -j 2`: **PASS**, exit 0.
- `cargo check --offline --bin alera-gpui-desktop -j 2`: **PASS**, exit 0.
- `flutter test --no-pub test/unit/prompt_workspace_runtime_client_test.dart`:
  **7/7 PASS** after fixing legacy Host no-routing graceful rejection.
- `flutter test --no-pub test/unit/agent_account_launch_environment_test.dart`:
  **6/6 PASS**, including persistent SSH-target / OS-user-pin round trip.
- The initial combined Flutter test attempt (13/14 pass) exposed and led to
  that legacy Host fix; the two focused suites above were subsequently
  rerun and passed. The initial tool-congested builds timed out; no live
  second Windows user, SSH daemon or separate AGY login was provisioned.
- Additional Rust and Native unit tests for SSH routing, identity binding and
  partial metadata remain pending, and no actual two-identity E2E acceptance
  has been performed. Native GPUI only displays paired state; initial SSH
  binding requires the documented CLI command.

Fourth-stage verification on Windows:

- `cargo check --offline -p alera-cli --bin alera`: PASS (exit 0).
- `cargo check --offline --bin alera-gpui-desktop`: PASS (exit 0).
- Rust format check and staged Git whitespace check: PASS.
- Targeted Rust OS-identity tests were started but **not executed**: compiling
  the large `alera-cli` test executable consumed excessive time, so the
  local test build was stopped. The tests remain unverified; compilation
  success must not be reported as a test pass.

Current verification: `flutter test --no-pub
test/unit/agy_account_registration_controller_test.dart` passed (1/1).
`flutter test --no-pub` for account environment, account controller,
prompt-workspace runtime client, AGY registration widget, and Workbench
provider-boundary test passed **14/14**. The full Workbench controller
compiled and its focused bootstrap regression passed **1/1**.
`dart run build_runner build --build-filter=lib/src/features/settings/domain/alera_settings.mapper.dart`
passed, generating the new `selectedAgyAccountId` mapping without deleting
unrelated tracked output.
`cargo check --offline --bin alera-gpui-desktop` passed (Windows native,
exit 0). `cargo check --offline -p alera-cli --bin alera` also passed
(Windows, exit 0). Two-account live sign-in remains a separate gate.

The new Rust unit cases covering the terminal identity rejection and
`agentProfile.launch` rejection have not been individually executed.
No WSL distribution is configured in the inspected Windows environment; no
new Windows OS user or Google credentials were created or modified.

Additional regression tests were added for the versioned account CLI
(reject legacy Host responses, invalid and duplicate labels, metadata-only
removal), Native GPUI capability validation/offline behavior, and two-client
Host revision conflict. These Rust test cases are not yet individually
executed; the Rust CLI binary and Native GPUI pass compilation/type checking.

The next incremental step registers **pending** AGY account labels and stable
non-default account IDs in `RuntimeAgentQuotaSettings.agyProfiles`. IDs are
restricted to 1-64 ASCII letters/digits/`_`/`-`, cannot be `default`,
and are deduplicated; aliases are trimmed and bounded. The AGY profile schema
rejects unknown secret/credential fields.

- Classic Settings > Provider Quotas now has an Add Google Account registration
  dialog (add/edit/remove), clearly marked **registration only**. This is not
  Google's OAuth flow and never reads or rewrites Windows Credential Manager.
- The quota Host passes these labels to its proxy. Their dedicated quota rows
  are `unavailable`, with an explicit login-isolation reason; the proxy
  NEVER runs `agy /usage` for those IDs, nor labels the default login as a
  different account. The `identity-unverified` marker also forbids reviving
  a previous reading as stale for a pending identity. Existing `default`
  behavior remains unchanged.
- Registered IDs appear in History Sync account catalogs as
  `supported=false`, including GPUI's shared History Sync UI, but the
  native AGY mirroring worker still does **not** include any AGY directories.
- No selected AGY account can be launched while identity binding is
  unverified. Existing PTYs and runtime credentials are unchanged.

This step covers **account registration UX and safe metadata plumbing only**.
It must not be described as multi-login support, multi-account quota access,
cross-account native Resume, or a completed A1 credential-isolation backend.

### Verification ledger for this feature worktree

- `cargo test --offline -p alera-core --features runtime agy_accounts_are_optional_non_secret_metadata_and_normalize_unique_ids`:
  passed (1/1), including the final account ID/name deduplication rules.
- `dart run build_runner build --build-filter=lib/src/features/settings/domain/alera_settings.mapper.dart`:
  passed, produced the AGY profile and Host settings mapping. The generated
  EOF normalizer and targeted Dart formatting succeeded. Unrelated output
  deletions caused by the build filter were individually restored from HEAD.
- `rustfmt --edition 2021 --check`: passed for modified Rust files.
- `flutter test test/unit/agent_history_sync_settings_test.dart`:
  passed (6/6), including pending AGY account model roundtrip.
- `flutter test --no-pub test/widget/agy_quota_profiles_control_test.dart`:
  passed (1/1), including invalid IDs and explicit registration-only warning.
- `cargo test --offline -p alera-cli agy_registered_account_is_unavailable_without_touching_default_keyring`:
  targeted unit test passed (1/1), but the overall Cargo command reached its
  process deadline while traversing unrelated integration-test executables.
- The Host/CLI test suite, Classic/GPUI UI compilation and two-account live
  sign-in are separate validation gates; never interpret the metadata-only
  registration as an authenticated account.

## Current Alera behavior

- The host's `agentQuota.snapshot` fetcher reads one AGY login with `agy /usage`.
  That snapshot has `provider=agy` and `accountId=default`.
- Claude and Codex already have configured profile lists, account-scoped quota
  queries, pin keys and launch-environment selection. AGY has none of these.
- The CLI proxy previously ignored a requested `accountId` for AGY and
  fetched the default account anyway. The first safe change is to **fail
  closed** for non-default AGY IDs rather than mislabel another login's quota.

## Verified provider constraints

- The official AGY CLI stores settings under `~/.gemini/antigravity-cli/`
  and uses a native OS keyring for subscription authentication:
  https://antigravity.google/docs/install
- On Windows, the `gemini:antigravity` Credential Manager item is scoped to
  the Windows user, **not** to `HOME`, `USERPROFILE` or a distinct CLI data
  directory. Changing these directories alone cannot safely isolate
  simultaneous Google accounts. A CLI collaborator confirmed this:
  https://github.com/google-antigravity/antigravity-cli/issues/1261
- The local `agy --help` output does not expose a built-in named
  account/profile switch parameter. Third-party switchers may overwrite the
  shared credential; they must not be silently depended upon.
- macOS and desktop Linux keyrings require the same per-login identity proof;
  only a demonstrated isolated secret-store backend qualifies for parallel use.

## Required product contract

1. **Real account identity:** a configured alias is not proof of an independent
   login. Every snapshot and new terminal must be bound to an identity verified
   by its credential backend. Unknown/ambiguous => unavailable, never default.
2. **Credential ownership:** Alera stores only aliases and references to
   external or OS-protected identity contexts. No plaintext OAuth secrets,
   credential copies, session-token logs or login-state rewrites.
3. **Isolation:** independent accounts require independent authentication
   namespaces. Windows must use separately authenticated OS-user contexts or
   another independently verified backend; simply setting
   `USERPROFILE`, `HOME`, or a `--gemini_dir`-style data flag is insufficient.
4. **Quota correctness:** `(hostId, provider, accountId)` uniquely identifies
   snapshots, refresh requests, stale cache entries and status-bar pins.
   Never refresh a non-default row by reading the machine's default credential.
5. **Launch safety:** selection affects **new** terminals only; existing
   sessions, PTYs, hooks, conversation IDs and authentication remain intact.
   A backend that cannot prove isolation must refuse the launch rather than
   silently use a different login.
6. **No implicit rotations:** no automatic account switching to bypass limits,
   no unattended Windows Credential Manager swapping during quota polling.
   Explicit manual user action and clear current-account status are required
   for any future single-login account-switch adapter.

## Milestones / TDD

| Stage | TDD / acceptance gate | Status |
| --- | --- | --- |
| A0 | AGY non-default `accountId` does not launch `agy` or return a Default snapshot | Implemented; AGY-focused Rust tests pass (18/18) |
| A1 | Per-platform identity capabilities; authenticated context proves unique account; reject alias-only identities | Not started |
| A1a | Host-owned credential-scope capability report (explicitly no local multi-login); no keyring mutation | Implemented in feature worktree; independent-identity proof still pending |
| A2a | Host-safe AGY account label registration; unavailable per-account quota placeholders; shared history catalog | Implemented in feature worktree; validation pending |
| A2b | Per-account verified credential binding, independent quota fetches and partial refresh | Blocked on A1 |
| A3a | Classic Add Google Account registration dialog and group labels | Implemented in feature worktree; validation pending |
| A3a-GPUI | GPUI Add/Edit/Remove metadata-only account registration with CAS Host readback | Implemented; Windows native check passed, live GUI not yet tested |
| A3b | Real quota cards/pins and selected AGY account for a new Terminal | Blocked on A2b |
| A4 | Two-account live smoke on Windows and Linux/macOS; no cross-account leaks, no shared-keyring mutation, no active-PTY interruption | Not started |

No merge, release, auto-deployment or default Host replacement until A1-A4
are validated. A0 is a correctness fix, not a claim of AGY multi-account parity.
