# Alera Settings parity phase 5 — Quick Open exclusions and project manager

## Source of truth

The existing Classic Editor preference `editor.quickOpenExcludedDirectories`
has moved into the reviewed Rust Core desktop settings contract as field
**29**. Alera Classic and GPUI continue using the same
`configuration.portable.v1` document, accessed via authenticated Runtime
Host `desktopSettings.get`, `compareAndSet` and
`compareAndSetBatch`. No new Native JSON or Flutter Drift writer was added.

Rust Core owns the first-run exclusion list and validates all CAS values:
- A bounded list of at most 128 unique (case-insensitive) directory names;
- individual nonempty, trimmed basenames of at most 128 bytes;
- no separators, parent traversal, or control characters;
- an empty list is allowed intentionally; `.git`, `.hg`, and `.svn`
  remain hard-excluded from Ctrl+P search.

The default list exactly mirrors Classic's
`EditorSettings.defaultQuickOpenExcludedDirectories` (24 entries).
Classic's settings repository now applies the Host snapshot to this field,
uses structural JSON list equality to detect real changes, and sends the
entire edited list with expected old value through field-scoped Rust CAS.
An unrelated Classic edit cannot replay an old exclusion list into
`configuration.settings.update`.

## GPUI Settings behavior

**Editor → Quick Open → Excluded Directories** now renders the actual
Host-owned list with individual Remove buttons, an IME-aware freeform
directory-name input, Add, and Restore Defaults. The input is cleared only
after successful Rust Host readback; rejected edits keep the draft. The UI
never writes the old `native-settings-v1.json` file for these fields.

**Ctrl+P** consumes the same authenticated snapshot and matches ancestor
directory names without case sensitivity, even with Git-ignored files shown.
It also protects control directories `.git`, `.hg`, `.svn`. The Explorer
file/directory fuzzy filter is **not** affected: the existing bounded
index is shared and the exclusion list is applied to Ctrl+P results only.
The scanner may still walk excluded directories when Explorer search
needs them. This deliberately preserves Explorer visibility; a separate
scan/cache would be required if skipping traversal were prioritized.

**Settings → Projects → Projects** opens the existing Native saved
projects/worktree management panel and dismisses the Settings modal.
Per-project editor settings (the other row) are still not implemented.

## Tests and remaining gaps

- Rust Core CAS test verifies a first-run 29-field snapshot, two
  independent store clients, Classic portable readback, stale-revision
  rejection, malformed duplicate rejection and re-open persistence.
- Classic Flutter repository test verifies Host-provided list, deep
  equality of separately allocated list objects, CAS with expected list
  and no stale `configuration.settings.update` replay.
- GPUI Native snapshot test rejects duplicate exclusions atomically.
- GPUI file search test verifies case-folded nested exclusions,
  protected VCS directories, live settings changes without re-scan and
  unaffected Explorer search, including Git-ignored-visible mode.
- GPUI compilation, Rust CLI check, catalog tests and Dart analysis
  remain part of the verification gate.
- This is source validation; a real simultaneous Classic + GPUI Windows
  GUI session with the rebuilt Host/CLI and restart is still outstanding.
- Syntax-theme colors, Terminal cursor style/blink/TUI wheel speed,
  full Settings panels for AI Assist, Dictation, Quotas, Accounts,
  Mobile Devices and Remote Hosts remain incomplete and should not be
  claimed as functional.

Nothing in this change implies deployment, push or merge into main.
