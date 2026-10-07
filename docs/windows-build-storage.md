# Local Windows Build Storage

Local Windows builds store compilation artifacts and caches at the root of a local fixed drive outside Dropbox. By default the preparer selects the ready fixed drive with the most available space, requiring at least 20 GB free. Prepare each checkout before running raw Flutter, Dart, Cargo, tests, code generation, or native-asset commands:

```powershell
& .\tool\development\prepare_windows_build_storage.ps1
pwsh -File tool/development/prepare_windows_build_storage.ps1 -CheckOnly
```

The standard Windows setup, `make` debug flows, `make rust-test`, Release build helper, and Final-Link exporter prepare storage automatically. Existing generated directories are moved without overwriting an existing destination. The preparation refuses a directory referenced by a running compiler or app and preserves an existing Cargo configuration instead of replacing it. Wait for the owning process to exit before retrying a migration.

| Contents | Physical storage |
| --- | --- |
| Native Cargokit | `<drive>:\c\n` |
| Bundled Rust CLI | `<drive>:\c\cli` |
| Direct Cargo, Rust tests, debug CLI | `<drive>:\c\t` |
| Flutter `build`, `.dart_tool`, Windows ephemeral files, preview/mobile/package caches, prebuilt libraries, local `.tools`, landing/edge dependencies and outputs | `<drive>:\alera-build\checkouts\<checkout-id>` |
| Pub, sccache, Zig/Gradle/npm/Bun caches, temporary files | `<drive>:\alera-build` |
| Isolated local release worktrees | `<drive>:\alera-build\worktrees` |

Flutter expects several generated paths inside its source checkout. Windows junctions keep those paths working while their contents reside on the selected drive. Each checkout has its own Flutter/CMake directories; Rust uses Cargo's shared target locking. The local `.cargo/config.toml` selects `<drive>:\c\t` and disables incremental compilation. The selected roots are also written to the git-ignored `.cargo/alera-build-storage.json` so Dart debug tooling uses the exact same drive. The config is machine-local and git-ignored. Debug staging reads the configured Cargo target rather than assuming `rust/target`.

Generated paths beneath Dropbox also carry the Windows `com.dropbox.ignored` stream with value `1`. Preparation repairs missing attributes on existing junctions, and `-CheckOnly` rejects missing attributes. This is Dropbox's ignore mechanism; Git `.gitignore` does not control Dropbox syncing. If Flutter requires a physical generated directory inside the checkout, mark that directory before generating its contents with `Set-Content -LiteralPath '<generated-directory>' -Stream com.dropbox.ignored -Value 1`. Only mark generated output/cache directories, never the source checkout. See [Dropbox's ignored-file documentation](https://help.dropbox.com/sync/ignored-files).

The script exports build environment variables to the calling PowerShell process. When running raw commands, use `& .\tool\development\prepare_windows_build_storage.ps1` in the same PowerShell session so pub, sccache, Zig, and temporary files use the selected drive. To force a location, set `ALERA_BUILD_STORAGE_ROOT=X:\alera-build` and `ALERA_CARGOKIT_TEMP_DIR=X:\c`, or pass `-StorageRoot` / `-CargoScratchDirectory`. Starting preparation in a separate `pwsh` process creates the junctions/config but does not export environment variables back to its parent. Standard entry points prepare their own environment.

`flutter clean` can remove generated directories and their junctions. Run preparation again before another build or test. Use Cargo's `clean --target-dir <exact-target>` to clear unused Rust artifacts, only after verifying no build uses that target. Do not delete a source worktree or its uncommitted files to clear compilation caches. The next build after cleaning must recompile the removed outputs.

The short `<drive>:\c\n` path protects Vulkan's nested CMake object paths from MSVC path limits. Keep Ninja and `GGML_CCACHE=OFF` for Windows native builds. Linux/macOS and GitHub Actions retain their existing paths; Windows CI continues to use its explicit `R:\c` configuration. This policy relocates Alera build outputs and per-build caches, not system-installed Visual Studio/Rustup or other projects' package stores.
