# Alera Rust / Flutter 效能優化交接

最後更新：2026-09-16

## 0. 一頁摘要

這份文件是目前 Alera Rust / Flutter 效能優化工作的接手入口。先讀這份，再看 `docs/rust-performance-optimization-roadmap.md`。

交接時的重點狀態：

- `main` HEAD：`665794ac77df43f010b6d61528f510d15a65415b` (`perf(explorer): filter hidden entries in Rust`)
- `origin/main`：交接檢查時與 `main` 同步（ahead 0 / behind 0）。
- Agent runtime recursive I/O、Git status projection/reconciliation、Git diff alignment/lazy rows 已經進 `main`，**不要重做**。
- Terminal core migration 是刻意暫停的議題；只有 parser/model benchmark 已落地，除非產品/維護者重新開題，否則不要自行啟動大規模 terminal rewrite。
- 下一個最明確的未完成項目是 Diagnostics ZIP direct-to-file。實作已做到 Rust direct-to-file + Dart production wiring，但目前正在 rebase 到最新 `main`，只剩 FRB generated bindings 衝突，**尚未完成驗證與合併**。
- `main` 本身目前有兩個與本交接不同來源的未提交 tracked changes；不要 restore/覆蓋：
  - `lib/src/features/workbench/application/workspace_file_service.dart`
  - `test/unit/workspace_file_service_test.dart`
- `.worktrees/` 不能整批刪除。至少 diagnostics worktree 正在 rebase；另外 `explorer-file-manager` 也仍是 dirty worktree。

建議接手第一件事：**先保全 `main` 的未提交修改，再完成 `.worktrees/diagnostics-streaming-zip-v2` 的 rebase / codegen / tests / RSS 驗證。**

---

## 1. 效能優化的設計原則

這一輪不是「看到 Dart loop 就搬 Rust」。核心原則是：

1. 減少總工作量，而不是只換語言。
2. 避免大型資料在 `Rust -> FFI -> Dart` 後又被 Dart 重新排序、分組、parse、alignment。
3. recursive filesystem、diff projection、compression 這類會隨資料量放大的工作優先放 native。
4. 小型 top-level syscall / marker file 操作若沒有 profiling 證據，不要為了純 Rust 化而增加 API 複雜度。
5. 保留 pure-Dart / fake backend fallback，讓單元測試與沒有 RustLib 初始化的測試仍可跑。
6. FRB generated files 不手改；修改 Rust API/model 後重新 codegen。
7. 每一批維持 TDD、小批次、驗證後獨立 commit。
8. 先 profile / benchmark，再開大型 migration。

Roadmap：`docs/rust-performance-optimization-roadmap.md`

---

## 2. 交接當下 repository / worktree 狀態

### 2.1 `main`

基準：

```text
branch: main
HEAD:   665794ac77df43f010b6d61528f510d15a65415b
subject: perf(explorer): filter hidden entries in Rust
upstream: origin/main
status at handoff: ahead 0 / behind 0
```

`main` **不是 clean**。Tracked changes：

```text
 M lib/src/features/workbench/application/workspace_file_service.dart
 M test/unit/workspace_file_service_test.dart
```

目前 diff 看起來是在優化 `WorkspaceFileService.applyGitStatusSnapshot()`：

- 沒有 status 變更時直接回傳原 list。
- status 已一致時重用原 entry。
- 只有第一個真正改變的 entry 出現時才配置新 list。
- 測試鎖住 identity reuse 與 metadata preservation。

這兩個檔案**不是本 handoff 文件建立的修改**。接手時不要 restore、不要順手混進 diagnostics commit；先確認修改來源/owner，再決定提交或延續。

另外有：

```text
?? .worktrees/
?? docs/history-session/
```

這兩個路徑也不要直接 `git add -A`。

### 2.2 重要 worktree

#### `.worktrees/diagnostics-streaming-zip-v2`

這是目前最重要的 continuation worktree，**禁止刪除**。

交接時狀態：

```text
interactive rebase in progress; onto 665794ac
Last command done:
  pick 8aaa3f11 perf(diagnostics): stream bundles directly to disk
No commands remaining.
Rebasing branch: perf/diagnostics-streaming-zip-v2
```

目前只有兩個 unmerged paths，而且都是 FRB generated files：

```text
UU lib/src/rust/frb_generated.dart
UU rust/src/frb_generated.rs
```

其餘 diagnostics 變更已 staged，包括：

```text
lib/src/features/diagnostics/infra/diagnostics_bundle_builder.dart
lib/src/features/diagnostics/infra/diagnostics_service.dart
lib/src/features/settings/presentation/panes/application_diagnostics_section.dart
lib/src/rust/api/diagnostics.dart
lib/src/rust/frb_generated.io.dart
lib/src/rust/frb_generated.web.dart
rust/Cargo.lock
rust/Cargo.toml
rust/src/api/diagnostics.rs
rust/src/api/mod.rs
test/unit/diagnostics_bundle_builder_test.dart
```

原 branch commit：

```text
8aaa3f11 perf(diagnostics): stream bundles directly to disk
```

注意：worktree 顯示 detached HEAD 是 **rebase 進行中的正常現象**，不是 branch 消失。`perf/diagnostics-streaming-zip-v2` branch 仍存在。

#### `.worktrees/explorer-file-manager`

Git worktree list 顯示它是 dirty。雖然其 branch 已被標成 merged，也不要在沒有檢查 diff 前刪除。

其他 merged/clean worktree 可以另行清理，但不屬於這份效能交接的必要工作。

---

## 3. 已完成並進 `main`：Agent runtime resources

### 3.1 已完成內容

原本 Claude / Codex resource preparation 會在 Dart 做 recursive traversal、fingerprint、copy、delete，會放大 UI isolate filesystem work。

已完成：

- 避免 Codex 同一次 terminal launch 重複 resource sync。
- recursive fingerprint 搬 Rust。
- recursive copy 搬 Rust。
- recursive delete 搬 Rust。
- fallback 的 delete -> copy -> fingerprint 收斂成一次 native reconcile，避免多次 FFI round-trip / 重複掃目錄。
- production provider 使用 native path。
- pure Dart tests / fake provider 保留 Dart fallback，不要求 RustLib 初始化。
- Claude / Codex marker hash compatibility 保留。
- source 在掃描後消失等 `NotFound` race 行為與舊 Dart path 對齊。
- Codex 一般 terminal launch 使用 global `CODEX_HOME` in-place；isolated runtimeHome path 才需要 resource sync。

### 3.2 主要 commits

```text
d582d217 perf(agent): avoid duplicate Codex runtime sync
45a55585 perf(agent): fingerprint runtime resources in Rust
ca7f7842 perf(agent): copy runtime resources in Rust
2d1d5181 perf(agent): delete runtime resources in Rust
77c554e7 perf(agent): reconcile runtime copies in Rust
```

### 3.3 已知邊界

現在還留在 Dart 的多半是：

- top-level ownership / link decision
- marker 小檔
- 少量單一路徑 type/link 判斷

這些不會像 recursive traversal 一樣隨目錄大小放大。**沒有新 profiling 證據前，不要繼續為了「全部 Rust 化」而搬。**

### 3.4 合併前已做的驗證

最後一次完整 optimization branch merge gate：

- Rust Agent focused tests：`10/10`
- Flutter Agent focused tests：`66/66`
- touched-files analyzer：0 issue

---

## 4. 已完成並進 `main`：Git status projection / reconciliation

### 4.1 Native projection

已把原本 Dart 會重做的 status projection 往 Rust 收斂：

```text
f29e076c perf(git): project status groups in Rust
4aaa18a8 perf(git): project status tree rows in Rust
```

設計重點：

- group 回傳 `entryIndices`，不重複 serialize 同一個 `GitChangeEntry`。
- tree file row 使用 `entryIndex`。
- directory/file ordering、depth、fileCount 與原 UI 語意保持一致。

### 4.2 Dart reconciliation allocation 優化

後續又把 refresh common path 的短命 allocation 降低：

```text
5c92a308 perf(git): avoid entry equality key allocations
948f048b perf(git): defer reconciliation allocations
f1e10e0b perf(git): rebind status groups by native indices
2308bcf4 perf(git): bucket reconciliation by path
b5630823 perf(git): merge unified status groups linearly
```

主要改善：

- unchanged entry 不再建立兩個 13-field record key 只為 equality。
- 全部 unchanged 時直接回傳 previous，不先配置 merged list / bool arrays。
- production native groups 直接用 entry index rebind，不建 `source object -> rebound object` HashMap。
- reorder/insert fallback 改成 path bucket + full-value compare。
- same-path staged/unstaged 與 exact duplicate LIFO 語意都有 regression coverage。
- Unified Changes 已經收到各 area 的排序結果，改用最多 3 組的 linear k-way merge，不再整份 O(n log n) 重排。
- legacy / mock / groups-empty path 保留原 fallback。

### 4.3 驗證

最後 merge gate：

- Git focused tests：`23/23`
- touched-files analyzer：0 issue
- `git diff --check`：passed

---

## 5. 已完成並進 `main`：Git diff projection / lazy materialization

主要 commits：

```text
528cb41a perf(git): project side-by-side diff rows in Rust
eb4cafbf perf(git): project full-file diff alignment in Rust
3ef1f5d2 perf(git): project single-column full-file rows in Rust
95bf78f7 perf(git): lazily materialize full-file context rows
45d6b259 perf(git): lazily materialize side-by-side context rows
69796a90 perf(git): lazily materialize full-file replacement rows
e3792ca6 perf(git): lazily materialize unified diff rows
7b6d7220 perf(git): lazily materialize side-by-side diff rows
```

### 5.1 重要架構決策

不要把 full file 文字重新送回 Rust 只為 alignment。

現有方向是：

```text
Rust diff generation
  -> compact alignment/index plan
  -> Dart 使用已 decode 的 old/new full-file text 展開 UI rows
```

因此可以同時避免：

- Dart 再 parse hunk / 做大量 alignment work。
- FFI 再傳一次完整 old/new file strings。

### 5.2 Editable diff 不可破壞

Full File Side-by-Side 的 editable working-tree path 有自己的 editor buffer / save / conflict semantics。

效能 projection 只處理 read-only row projection；**不要把 editable right-side editor 改成 native-owned text snapshot**，除非另外設計完整 editing protocol。

### 5.3 Row-level native paging 暫時不是優先項

先前檢查時，單檔 native diff 已有約 `5000 lines / 512 KiB` 的 bounds，UI 又已有 file-page / hydration + lazy spans。

因此沒有實際 profiling 證據前，不建議為 roadmap 上的「paging」字面目標導入 retained native diff snapshot / lifetime management。那會增加狀態管理、失效與 FFI API 複雜度。

如果之後大型 diff 仍有可量化瓶頸，再以真實 trace / allocation / frame data 決定是否做 row paging。

---

## 6. Terminal：刻意暫停，不是下一個接手項目

目前已有：

```text
0cbe7b80 perf(terminal): add parser model benchmark
```

`integration_test/terminal_parser_benchmark.dart` 可當 parser/model baseline。

先前評估結論：

- Alera 的 xterm2 fork 已經相當完整。
- 真要做 Rust terminal core PoC，feature-complete 首選是 official WezTerm `wezterm-term`，以 exact Git SHA pin 住，外面包 Alera-owned adapter。
- `alacritty_terminal` 適合當 fallback / benchmark candidate。
- parser-only crate 不足以替代 xterm2；Alera 需要 grid、cursor、alt screen、scrollback、resize/reflow、Unicode width、modes、hyperlinks 等完整 state model。

但這個議題已被明確暫停。**不要在 diagnostics 還沒收斂、也沒有新 profiling 證據時啟動 terminal rewrite。**

---

## 7. 下一個明確工作：Diagnostics ZIP direct-to-file

這是接手者建議優先完成的項目。

### 7.1 已經做到哪裡

`perf/diagnostics-streaming-zip-v2` 的 `8aaa3f11` 已經不是早期 `Cursor<Vec<u8>>` prototype。

Rust API 現在概念上是：

```text
write_diagnostics_bundle(
  output_path,
  metadata_json,
  app_log_directory,
  runtime_log_directory,
)
```

`rust/src/api/diagnostics.rs` 已做：

1. 在 destination parent 建 `.alera-diagnostics-*.tmp` tempfile。
2. `ZipWriter` 直接包 `File`。
3. 找出 app/runtime `.log`，排序後逐檔 `File::open` + `io::copy` 進 ZIP writer。
4. `meta.json` 直接寫入 archive。
5. finish + flush。
6. 若 destination 已存在就替換。
7. tempfile persist 到最終 output path。
8. missing log directory 直接略過。

這條路徑不需要在 Dart 建整個 ZIP `List<int>`，output archive 也不需先存在 Rust `Vec<u8>`。

### 7.2 Dart production wiring 也已經存在於 rebase patch

`DiagnosticsBundleBuilder`：

- 注入 `DiagnosticsBundleNativeWriter` seam。
- `writeToFile(...)` 只把 output path、metadata JSON、log directory paths 傳 native。
- 不在 Dart scan / 讀取整份 log。

`DiagnosticsService`：

- export 前先 `AppLogger.flush()`。
- 取得 package/runtime metadata。
- 呼叫 builder 直接寫 output path。

Settings UI：

- 先取 runtime facts。
- 再開 save picker。
- 使用者選好 destination 後才 flush / stream logs。
- 完成後顯示 success toast。

### 7.3 已有測試

Rust `diagnostics.rs` 目前有 regression：

- sorted app/runtime logs + `meta.json` contents。
- missing directories 被略過。
- existing destination 可以被替換。

Dart `diagnostics_bundle_builder_test.dart` 目前有：

- output path / app/runtime directories / metadata JSON 正確 forward 給 native writer。
- Dart 不先 scan / reject missing directories。
- suggested filename filesystem-safe。

### 7.4 現在真正卡住的地方

正在把 `8aaa3f11` rebase 到 `665794ac`。

**只剩兩個 generated file conflicts：**

```text
UU lib/src/rust/frb_generated.dart
UU rust/src/frb_generated.rs
```

Conflict 內容主要是：

- `rustContentHash`
- FRB function IDs
- current `main` 的 workspace-files APIs vs diagnostics 新 API

這類衝突**不要手算 funcId，也不要手合 content hash**。

### 7.5 建議安全解法

在 diagnostics worktree：

```powershell
cd .worktrees/diagnostics-streaming-zip-v2
git status
```

先確認仍是同一個 rebase。若是，而且 source/API conflicts 沒有新增：

1. 讓 conflicted generated files 回到目前 rebased base 的版本，或至少移除 conflict 狀態；不要嘗試人工保留舊 function IDs。
2. 確認這些 source-of-truth 仍包含 diagnostics 變更：
   - `rust/src/api/diagnostics.rs`
   - `rust/src/api/mod.rs`
   - `rust/Cargo.toml`
   - Dart diagnostics builder/service/UI
3. 從 worktree root 重新執行：

```powershell
flutter_rust_bridge_codegen generate
```

4. 檢查 codegen diff，確認：
   - diagnostics API 存在。
   - current main 的其他 Rust APIs 沒消失。
   - generated files 沒 conflict marker。
   - 沒有人工 funcId / hash patch。
5. `git add` regenerated files。
6. `git rebase --continue`。

如果 rebase 狀態和本文件描述不同，**先停下來重新 `git status` / `git diff`，不要直接套固定 ours/theirs 指令。**

### 7.6 Security invariant：ZIP 本身不做第二次 redaction

Direct-to-file implementation 是 byte-copy 已寫入磁碟的 `.log`。

目前兩端 logging sink 都已有「寫入檔案前 redaction」：

- Flutter app log：`lib/src/shared/infra/logging/log_redaction.dart` + `log_record_formatter.dart` / `AppLogger`。
- Rust terminal/runtime host：`rust/alera-cli/src/terminal_host/diagnostics/redaction.rs` + `jsonl_layer.rs`；runtime control token 啟動時也會註冊到 redaction layer。

所以現在的安全模型是：

```text
raw event
  -> sink-level redaction
  -> redacted .log on disk
  -> diagnostics ZIP byte-copy
```

不要把它改成「raw log 先落盤，export 時再遮罩」。那會讓平常磁碟 log 本身變成風險。

在 diagnostics merge 前仍建議補一個 end-to-end safety regression：寫入三類測試機密（已註冊 literal、HTTP Authorization credential、key/value credential），從實際 log -> ZIP 解開後確認原始值不存在。這能鎖住 UI 文案「Secrets such as tokens are masked before anything is written」的承諾。

### 7.7 還不能宣稱完成的項目

即使 direct-to-file code 看起來正確，merge 前仍缺：

- 完成 current rebase。
- FRB bindings 以 latest main 重產。
- Rust + Dart diagnostics tests 全綠。
- targeted analyzer 全綠。
- `git diff --check`。
- 實際大檔 / 多小檔 peak RSS benchmark。
- export UI smoke test。
- redaction end-to-end safety check。

尤其是 RSS：**架構上已消除整包 archive buffer，不等於可以跳過量測。**

---

## 8. Diagnostics 建議驗證順序

完成 rebase/codegen 後，建議按照這個順序，避免一次看到太多噪音：

### 8.1 Rust focused tests

```powershell
cargo test --manifest-path rust/Cargo.toml diagnostics_bundle --lib
```

如果 Windows 出現 `os error 32` / target artifact 被其他 Cargo process 鎖住，這次工作以前遇過共享 `rust/target` file lock。不要為了 lock 去改 dependency 或 source；優先改用獨立 target：

```powershell
$env:CARGO_TARGET_DIR = "$PWD/.cargo-target-diagnostics"
cargo test --manifest-path rust/Cargo.toml diagnostics_bundle --lib
```

### 8.2 Dart focused tests

```powershell
flutter test test/unit/diagnostics_bundle_builder_test.dart
```

如果同一批有 diagnostics service / settings UI tests，全部一起跑。

### 8.3 Analyzer

至少分析本批 touched Dart files；不要只看 generated files。

歷史上 optimization branch merge 時，**本批 touched files analyzer 是 0 issue**，但當時 full-repo `flutter analyze` 有 163 個其他既有 integration/test harness 問題。那個 163 只是當時的歷史觀察，不是永久基準，也不能拿來忽略新的 error。

### 8.4 Structural checks

```powershell
git diff --check
git status
```

再檢查 generated code 只反映目前 Rust API schema。

### 8.5 RSS / wall-time benchmark

至少做：

1. 多個小 log。
2. 單個或數個大型 log。
3. 記錄 export wall time。
4. 記錄 process peak RSS / working set。
5. 若能取得舊版 baseline，用相同資料比較。

驗收重點不是 ZIP 壓縮速度一定要大幅提升，而是輸出大小增加時，不應再出現「完整 archive 大小等級」的額外 Dart/Rust memory spike。

---

## 9. Release tooling / benchmark 已存在，不要重造

Windows release helper / release skill：

```text
36fdedfd tool(release): add verified Alera release workflow
```

該批已修正不能依賴 `$env:OS` 判斷 Windows 的問題，`-CheckOnly` 當時驗證過 Rust 1.98 / Ninja / scratch env。

Terminal parser/model benchmark：

```text
0cbe7b80 perf(terminal): add parser model benchmark
```

效能 roadmap merged-state 更新：

```text
40eb4aeb docs: mark Rust optimizations merged
```

---

## 10. Known traps / 不要踩的坑

### 10.1 不要手改 FRB generated function IDs

Rust API 一變就重新 codegen。Generated merge conflict 應由 source-of-truth + codegen 解，不是人工拼 ID。

### 10.2 不要 `git add -A`

目前主 worktree 有其他未提交修改、`.worktrees/`、`docs/history-session/`。每次 commit 明確列 paths。

### 10.3 不要把 `.worktrees/` 當垃圾目錄

至少 diagnostics 正在 rebase；`explorer-file-manager` 也 dirty。清 worktree 前逐個 `git status`。

### 10.4 不要把 full repo analyzer 的既有問題當成本批問題，也不要反過來忽略本批 error

做 touched-files analyzer + focused tests；full analyzer 另外追技術債。

### 10.5 不要重做已 native 化的 Git projection

Groups、tree rows、full-file alignment、side-by-side alignment、single-column full-file projection都已做。

### 10.6 不要無證據導入 retained diff paging state

先 profile 現有 file bounds + lazy materialization。

### 10.7 不要破壞 pure-Dart/fake test path

Agent / Git 多處刻意保留 fallback，讓 unit test 不需初始化 native library。

### 10.8 不要破壞 editable diff buffer ownership

Read-only native projection 和 editable workspace-side editor 是兩個不同問題。

### 10.9 不要自行重新開 Terminal core migration

這是刻意暫停的高風險大項。

---

## 11. Key commit archaeology

### Agent

```text
d582d217 perf(agent): avoid duplicate Codex runtime sync
45a55585 perf(agent): fingerprint runtime resources in Rust
ca7f7842 perf(agent): copy runtime resources in Rust
2d1d5181 perf(agent): delete runtime resources in Rust
77c554e7 perf(agent): reconcile runtime copies in Rust
```

### Git status

```text
f29e076c perf(git): project status groups in Rust
4aaa18a8 perf(git): project status tree rows in Rust
5c92a308 perf(git): avoid entry equality key allocations
948f048b perf(git): defer reconciliation allocations
f1e10e0b perf(git): rebind status groups by native indices
2308bcf4 perf(git): bucket reconciliation by path
b5630823 perf(git): merge unified status groups linearly
```

### Git diff

```text
528cb41a perf(git): project side-by-side diff rows in Rust
eb4cafbf perf(git): project full-file diff alignment in Rust
3ef1f5d2 perf(git): project single-column full-file rows in Rust
95bf78f7 perf(git): lazily materialize full-file context rows
45d6b259 perf(git): lazily materialize side-by-side context rows
69796a90 perf(git): lazily materialize full-file replacement rows
e3792ca6 perf(git): lazily materialize unified diff rows
7b6d7220 perf(git): lazily materialize side-by-side diff rows
```

### Supporting

```text
0cbe7b80 perf(terminal): add parser model benchmark
36fdedfd tool(release): add verified Alera release workflow
8c399f98 docs: update Rust performance roadmap status
40eb4aeb docs: mark Rust optimizations merged
665794ac perf(explorer): filter hidden entries in Rust
```

### Diagnostics continuation branch

```text
8aaa3f11 perf(diagnostics): stream bundles directly to disk
```

這個 commit **目前不在 main**；正在 `.worktrees/diagnostics-streaming-zip-v2` rebase 到 `665794ac`。

---

## 12. 建議接手順序

1. `git status` 確認 `main` 仍有那兩個 `workspace_file_service` 未提交修改；先保留，不要混進 diagnostics。
2. `git worktree list`，確認 diagnostics worktree 與 rebase 還在。
3. 進 `.worktrees/diagnostics-streaming-zip-v2`，讀 `git status`。
4. 確認只有 generated FRB conflicts；若 source 也衝突，先依最新 main 行為解 source。
5. 以 source-of-truth 重新 `flutter_rust_bridge_codegen generate`，不要手改 function IDs。
6. 完成 `git rebase --continue`。
7. 跑 Rust diagnostics focused tests。
8. 跑 Dart diagnostics focused tests。
9. 補/確認 end-to-end secret redaction regression。
10. 跑 targeted analyzer + `git diff --check`。
11. 做大檔 / 多小檔 RSS + wall-time benchmark。
12. 確認真的比舊 archive-in-memory path 降低 peak memory 後，才 merge `perf/diagnostics-streaming-zip-v2` 回 latest `main`。
13. 更新 `docs/rust-performance-optimization-roadmap.md` 的 diagnostics snapshot；目前 roadmap line 60 仍寫「等 direct output / Dart path 完成」，已比 branch 實際進度落後一步。
14. Diagnostics 完成後再重新 profile，決定下一個工作，不要直接從 roadmap 挑最大項目。

---

## 13. Diagnostics Definition of Done

只有以下全部成立才算完成：

- [ ] `perf/diagnostics-streaming-zip-v2` rebase 完成且 branch/worktree 無 conflict。
- [ ] FRB bindings 從最新 source 重新生成，沒有人工 funcId/content hash patch。
- [ ] Rust ZIP writer 直接寫 destination/temp file，不建立完整 archive `Vec<u8>`。
- [ ] Dart export path 不建立完整 ZIP `List<int>`。
- [ ] Existing destination replace、missing directories、sorted log entries、metadata tests 全綠。
- [ ] App log 與 runtime host log 的 sink-level redaction invariant 有 regression / 明確驗證。
- [ ] Focused Rust tests 全綠。
- [ ] Focused Flutter tests 全綠。
- [ ] Touched-files analyzer 0 issue。
- [ ] `git diff --check` 通過。
- [ ] Large-log RSS / working-set benchmark 有紀錄，證明不再隨 archive size 產生整包 memory buffering。
- [ ] Export UI smoke test 通過。
- [ ] Rebase latest main 後再做最後一次 focused gate。
- [ ] 只提交本批 paths，不混入主 worktree 其他未提交修改。
- [ ] Roadmap 更新到完成狀態。

---

## 14. 接手完成 diagnostics 之後怎麼選下一題

依這個順序，不要靠直覺：

1. 重新跑/建立 profiling baseline。
2. 找 UI isolate CPU、allocation、FFI payload 或 recursive filesystem 中會隨資料量成長的實際 hotspot。
3. 先確認是不是能「少做工作」；如果只是「同樣工作換 Rust」，先估 FFI / state ownership 成本。
4. 開小 batch + RED test。
5. production/native path 與 fake/test fallback 分開設計。
6. before/after benchmark 同機器、同 build、同資料集比較。

目前已知不應優先的項目：

- Agent top-level marker / ownership 的微型 syscall。
- Git status grouping/tree projection（已完成）。
- Git diff alignment（已完成）。
- 無 profiling 證據的 row-level retained paging。
- Terminal core rewrite（暫停）。
- ICO decode / terminal buffer accounting 這類低優先項，除非 profile 指向它們。

交接原則很簡單：**先把 diagnostics 收乾淨，再用量測決定下一個 native migration。**
