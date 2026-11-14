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

Current verification: `flutter test --no-pub
test/unit/agy_account_registration_controller_test.dart` passed (1/1).
`cargo check --offline --bin alera-gpui-desktop` passed (Windows native,
exit 0). `cargo check --offline -p alera-cli --bin alera` also passed
(Windows, exit 0). Two-account live sign-in remains a separate gate.

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
  passed (1/1) before the alias character-count normalization adjustment.
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
