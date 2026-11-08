# Alera GPUI Native — guarded text merge-conflict resolution (2026-11-08)

Development-only branch: `integrate/gpui-p0p1p2-20261108`.
The original nonconflicting Merge action is unchanged. This introduces a
separate **Inspect merge conflicts** / explicit file-side choice flow.

## Native UI flow

1. Type/select an existing **local source branch** in the Native Git sidebar.
2. Choose **Inspect merge conflicts**. This is read-only: it uses in-memory
   `git2::Repository::merge_trees` and never checks out a branch, writes an
   index or changes a Git ref. An error leaves worktree untouched.
3. Review the conflict file list. Select a file to see base/current/source
   excerpts. For every file choose **Ours** (currently checked-out branch) or
   **Theirs** (source branch). **A choice replaces the entire conflicted
   file, not just a hunk.** Each per-file choice disarms final confirmation.
   The UI displays only the first 1,800 characters per side and explicitly
   warns about truncation; inspect full files with the editor/Git tools before
   choosing.
4. Once every file is selected, click **Merge selected sides** and then
   **Confirm Resolved Merge**. The background Git operation regenerates the
   proposed three-way index, checks exact current/source OIDs, branch name,
   paths and choice cardinality, and performs safe checkout preflight.
5. If preflight succeeds and worktree/index are still clean, apply the
   merged tree and create a **two-parent merge commit** on the current branch.
   All automatically merged nonconflicting files are retained.
6. **Cancel review** clears local selection with no Git operation.

## Safety and limits

- Does not use `git merge` CLI, `reset --hard`, force push, shell
  interpolation, unstaged index markers, or external network operations.
- Requires checked-out local branch, a clean index/worktree **including
  untracked files**, a common ancestor and genuinely divergent history.
  Native Editor dirty-buffer checks also run before starting an action.
- Supports at most **48 conflict files**, UTF-8 regular files only, and at
  most **64 KiB per conflict stage**. Delete/modify, rename/path, symlink,
  submodule, binary/non-UTF-8 and oversized conflicts are refused; use a
  dedicated merge tool or the full Classic workflow. No per-hunk edits or AI
  suggestions are claimed here.
- Exact source/current refs and preview are rechecked before a write. A
  different branch tip, mismatched filename, missing or repeated choice,
  or dirty worktree fails closed. Another independent Git process changing
  files concurrently still requires standard external coordination.
- On a late checkout or commit I/O error, manually inspect Git index and
  worktree before retrying; the operation does not use a destructive
  automatic rollback. Existing `merge_without_conflicts` has the same
  conservative behavior.
- The UI is experimental. No production `main` merge, tag, push or release,
  and Native `supported=false` remains in force.

## Verification

Disposable `git2` repositories cover:

- Conflicting side preview returns full bounded UTF-8 base/ours/theirs
  without changing the branch/index/worktree.
- Explicit Ours and Theirs choices both form a correct two-parent merge
  commit and leave the worktree clean.
- Incomplete, duplicate, unknown paths, untracked changes, and a source
  branch advanced after preview are rejected without modifying files.
- Unrelated, automatically merged source-only additions are preserved.
- Binary conflict stages are not offered for text resolution.

### Isolated Windows GUI smoke

Built the Native Windows binary and launched it with separate Native app
settings and an intentionally nonexistent CLI/Host against a disposable Git
repository in `%TEMP%/alera-gpui-merge-ui-20261108-1759`.
The temporary repository had a `master` / `feature` conflict on
`conflict.txt`; no real remote was configured.

- Typed `feature` in the source branch field, clicked **Inspect merge
  conflicts**. The UI showed one conflict, base/ours/theirs text excerpts and
  an initial 0/1 selection state.
- Selected **Theirs**: the UI showed 1/1, disarmed prior confirmation, and
  required **Merge selected sides** then **Confirm Resolved Merge**.
- After the final click, disposable `master` advanced to merge commit
  `d1bf4f1` with two parents (`dd08de6`, `f65f64d`).
  `conflict.txt` contained `theirs`, and `git status --short --branch`
  printed just `## master` (clean).
- The UI cleared its conflict plan after success and reported the commit.
  The experimental Native window was closed. No production Git branch,
  remote, server or Host session was touched.

The first observed UI was vertically cramped in the right Git panel. A
subsequent change added scrolling to the entire right sidebar so large
conflict lists and confirmation controls remain reachable. That additional
scrolling change has only been compilation-checked, not individually
re-smoked in Windows yet.

Run without connecting a real remote:

```powershell
$env:CARGO_TARGET_DIR = 'D:\alera-build\alera-gpui-target'
cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --locked --offline
cargo check --manifest-path desktop_gpui/Cargo.toml --locked --offline
```
