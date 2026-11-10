# Alera GPUI Native — Git Stash / upstream Pull / Push (2026-11-08)

Development-only scope: `integrate/gpui-p0p1p2-20261108`, no production
branch changes, remote operations on real repositories, GUI acceptance or
release. This increments existing `GitActions` and Native Git sidebar.

## User-facing behavior

The Native Git sidebar exposes `Stash tracked`, `Apply stash`,
`Pull FF-only` and `Push`. The first click selects the action and displays
its effect; a **second click on the same action** executes it in a GPUI
background worker. Selecting a different sensitive action disarms the
previous one. Workspace changes cancel pending confirmation.

- **Stash tracked:** save tracked worktree/index modifications only to a
  new Git stash. Untracked files stay untouched; the operation refuses a
  conflicted index or an entirely clean tracked tree.
- **Apply stash:** apply `stash@{0}` only when the full worktree/index
  (including untracked paths) is clean; keep the stash after applying rather
  than popping or deleting it. An apply error preserves the stash; Git may
  retain conflict state for manual resolution.
- **Pull FF-only:** require a checked-out local branch with one configured
  upstream of the form `refs/remotes/<remote>/<branch>`, a clean worktree
  including untracked files, and `git pull --ff-only --no-rebase` for that
  exact configured destination. This never automatically creates a merge
  commit, rebases, switches branches or force-updates local history.
- **Push:** resolve the exact configured upstream and reject a known
  divergent/unfetched tracking ref. Push only the current branch's fully
  qualified `refs/heads` reference to its upstream using ordinary
  `git push --porcelain`, never `--force`. A remote-side race or protection
  rule remains the server's authority and may reject the push.
- **Fetch / Auto Fetch:** use the system Git CLI with noninteractive
  credential settings rather than libgit2's HTTPS/SSH-disabled transport.
  A remote that requires fresh authentication may fail closed; configure
  Git credentials using the normal OS Git tooling.

Network commands reuse `alera_core::git_cli::git_in_dir` (windowless on
Windows, structured argv not shell, no terminal credential prompt). Native
never reads or persists credentials. Raw remote stderr is not rendered in
the Git panel because configured URLs could contain credentials.

## Deliberate limits

- Initial publish and custom tracking-ref mappings are not yet supported
  through this Native UI. Configure upstream with Git first.
- Only the most recent stash can be applied; listing, selecting older
  entries, staged-state restoration, dropping and cross-branch apply
  require separate explicit UX and safeguards.
- Pull does not auto-merge divergent histories; the existing no-conflict
  local Merge is a separate explicit operation.
- The tests use disposable repositories with local bare Git remotes only.
  HTTPS/SSH credentials, protected server branches and GUI confirmation
  have **not** been tested or claimed as accepted.
- Native still has `supported=false`; no merge to `main`, push, tag,
  production Host changes or release.

## Verification

```powershell
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline
cargo check --manifest-path desktop_gpui/Cargo.toml --locked --offline
```

Focused Rust regressions create temporary local repositories to cover stash
tracked/untracked separation, safe apply with retained entry, push to an
upstream local bare remote, FF-only pull, divergence, dirty/untracked guards,
and missing upstream refusal.
