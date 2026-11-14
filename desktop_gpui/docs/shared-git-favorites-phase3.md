# Alera Classic + GPUI: Rust-owned Git favorites (phase 3)

## Shared owner and identity

Only `rust/alera-core/src/runtime/git_favorites_store.rs` persists live
Git favorites, in the existing Rust RuntimeStore's
`runtimeMetadata[git.favorites.scopes.v1]`. Classic/GPUI are views and do not
mutate separate settings copies.

Each key uses Classic's existing `branchFavoritesScopeKey` representation:
compact JSON `[projectId, relativeGitRootOrNull]`.

- `projectId` is an actual Host-registered Project ID, not an arbitrary
  directory, remote URL, branch or a hash of the currently selected worktree.
- `relativeGitRoot` is normalized, relative, POSIX path without traversal.
  Nested repositories do **not** borrow root-repository favorites. Existing
  folder-type projects may own a nested Git root but not the plain folder root.
- Native GPUI supplies only the canonical active Workspace root. The Host
  resolves that path against registered Git projects/linked worktrees, checks
  for ambiguous matches and converts it into the project-level root key.
  Therefore worktrees of the same project share its root Git favorite branches;
  an unrelated or unregistered root is rejected.
- All favorites are bounded and validated: at most 512 scopes, 80 branches
  per scope, no duplicates, invalid names or unsafe project identities.
- The Rust catalog is non-secret, does not execute Git or change the Git
  repository, index, refs, worktrees or credentials.

## Migration

1. Classic first load sends its old Workbench Drift
   `favoriteBranchesByRepository` map through authenticated
   `gitFavorites.seedClassic`. Rust inserts only absent valid registered
   scopes. Invalid/missing-project entries stay preserved in Drift; they are
   *not silently reinterpreted or reassigned*.
2. Native checks whether a legacy `git-favorites-*.json` file genuinely
   exists for its canonical Workspace root. **No file** means it only reads
   the shared Host catalog. **An old file, even with []**, means a one-time
   insert-only `gitFavorites.seedNative` may be attempted.
3. Once a Host scope exists, neither legacy importer can overwrite it.
   An explicitly cleared favorites list is stored as an empty array
   tombstone so a later old frontend restart cannot resurrect old pins.
4. Original Classic Drift and Native legacy files remain unchanged as
   rollback evidence. The Native JSON writer is only compiled in tests,
   not available to production GPUI.

## Cross-window synchronization

- Classic watches `gitFavoritesChanged` on its existing
  `RuntimeWorkbenchViewPrefsRepository.changes` channel, re-reads Rust
  favorites and refreshes Workbench views. Favorite star/rename operations
  update only changed repository keys, using Rust Host CAS; unrelated sidebar
  geometry edits never re-send or overwrite Host favorites.
- Native uses no-start authenticated Rust CLI on GPUI's background executor.
  Its Git Graph favorite star invokes `gitFavorites.casForWorkspace`, and
  only after Host confirms does it update the UI; a stale CAS is rejected
  and prompts a fresh Host read. Results for an old workspace generation
  are ignored. While Git Graph is open, polling is every five seconds;
  otherwise it is every fifteen seconds.
- Host broadcasts only when the catalog actually changes. Repeat migration
  reads do not generate event loops. Both editions use exact previous-list
  compare-and-set, so one cannot silently undo the other's edit.
- Errors fail closed: missing Host, old CLI, unregistered workspace, an
  ambiguous project path or corrupt old Native JSON do **not** create another
  local settings owner. The previous file is never deleted.

## Existing boundaries and follow-up acceptance

Native's Git Graph history currently uses the active worktree root; its
`git_diff_root` option is a *history path filter*, not a registered nested
Git repository selection. The nested-repository favorite identity is fully
preserved by Rust and Classic but should be hooked into Native only when
Native adds actual nested-repository Git history roots.

Automated regression tests:
- Two independent Rust RuntimeStore handles with project + linked-worktree
  path mapping, CAS conflict, restart, per-repository isolation and
  migration tombstones;
- Classic Workbench Host projection, orphan Drift key preservation,
  changed-scope-only CAS, rejected stale write;
- GPUI Native legacy-file existence discrimination and successful Rust
  Bridge compilation; CLI command parser and Host request type checking.

Manual Windows acceptance remains important: rebuild Host and both desktop
executables, open Classic and GPUI *simultaneously* against the same Runtime
profile; star/unstar the same branch in each direction, switch to a linked
worktree and a second project, then restart both editions. Verify Nested Git
root pins remain isolated, concurrent toggles conflict safely and no native
JSON favorite file is rewritten. A source-only commit does not deploy the
Host, rebuild Classic or complete interactive acceptance.
