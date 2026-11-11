# Antigravity (AGY) multi-account: safety contract and staged implementation

Status: **investigation + safety groundwork; full multi-account support is NOT implemented**.
Date: 2026-11-11. Branch: `feat/agy-multi-account-20261111`.

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
| A2 | Host settings and migration for AGY account references; account-scoped snapshots and independent partial refresh | Blocked on A1 |
| A3 | Classic + GPUI settings, quota cards, pinned rows and selected account for new Terminal | Blocked on A2 |
| A4 | Two-account live smoke on Windows and Linux/macOS; no cross-account leaks, no shared-keyring mutation, no active-PTY interruption | Not started |

No merge, release, auto-deployment or default Host replacement until A1-A4
are validated. A0 is a correctness fix, not a claim of AGY multi-account parity.
