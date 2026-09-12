# Alera 架構重構完整交接

日期：2026-09-11  
架構盤點基線：`docs/architecture-review-2026-09-11.md`  
目前工作分支：`refactor/architecture-guard-ci`  
目前工作樹：`E:\Dropbox\work\alera\alera-architecture-guard-ci`  
撰寫前 HEAD：`285b415e7dbd6d44b76689fd741a5b17574d62fe`  
Upstream：無  
Push：尚未 push  
工作樹狀態：撰寫本文件前乾淨

> 本文件是後續實作者的可執行交接，不取代 `docs/architecture-review-2026-09-11.md`。架構盤點文件說明「為什麼」與整體目標；本文件說明「已做到哪裡、現在有哪些限制、下一批要怎麼做」。

---

## 1. 接手時先遵守的工作規則

1. 維持小批次 TDD：先建立 red test，再做最小實作，focused regression 綠後才提交。
2. 每批驗證後可以直接提交並繼續下一批，不必逐批詢問。
3. **不要自行 push。** 只有使用者明確要求才 push。
4. 不要把其他 worktree 的 dirty changes 帶進本分支，不要代為 stash/restore/clean。
5. 不要為了降低行數機械式拆 production 檔案；production 拆分必須對齊 capability、state ownership 或 cross-layer contract。
6. 測試拆分可做純結構搬移，但必須跑原 entrypoint，不能只看編譯成功。
7. Rust actor 重構要保留 actor 的 state ownership；重 I/O 可以移出 mailbox，但 completion 必須回 actor 套用，且要處理 stale completion、client disconnect、ordering、backpressure。
8. 不要把所有 channel 一次改成 bounded，也不要無限制 `tokio::spawn`。控制面 completion/quit 不能因 admission 設計自鎖。
9. 不要重做已完成的 orchestration completion replay；`c1d88258` 已處理 committed completion replay。
10. 不要推倒重寫 Flutter/Riverpod/SQLx，也不要讓 local core workflow 依賴 cloud。

### 1.1 其他 worktree 警告

目前 `git worktree list` 顯示多個 Alera worktree。與本交接最相關的是：

- `alera-architecture-guard-ci`：本工作分支，撰寫前乾淨。
- `alera-runtime-boundary`：`refactor/runtime-boundary`，HEAD `6ab6b9b9`，**目前 dirty**。
- `alera-runtime-boundary` 的 dirty file 已再次確認為：
  `rust/alera-core/src/runtime/orchestration_store_tests/v2_contract.rs`
- 主 worktree `alera` 目前也被工具報告為 dirty；本工作沒有清理或吸收它。

接手者不要在本批次碰上述其他 dirty worktree。

---

## 2. 整體方向

Alera 不需要 rewrite。目標仍是原架構盤點的演進式路線：

```text
Desktop composition root                      Mobile composition root
  Feature UI + per-window state                 Per-host UI + app lifecycle
  Feature use cases / read models               Narrow runtime client surfaces
  Ports + lifecycle owners                      Ports + connection owners
           |                                              |
           +---------- typed, versioned contracts --------+
                                  |
                   Runtime adapters / transport
                                  |
          Rust admission + session/job supervision
                                  |
        Domain use cases + transaction/idempotency contracts
                                  |
          Runtime SQLite / PTY history / filesystem / Git

Desktop local adapters -> native Rust FFI -> local Git/files/search
Optional cloud: account/configuration/grants control plane
Optional edge: authorized encrypted relay data path
```

必須保留的現有設計：

- runtime 是 durable PTY 與多個業務資料的 owner。
- UI detach / resource release 不等同 terminate durable PTY。
- Desktop 與 Mobile 的 UI state 不強制共用。
- Local Git/files/search 可繼續經 native FFI，不需要全部繞 runtime socket。
- Cloud 保持 optional。
- 舊 host / capability downgrade 行為不能在重構中被偷偷移除。
- 既有 race、cleanup、visible error message assertions 不可為了過測試而刪掉。

---

## 3. 已完成的工作

### 3.1 架構盤點基線

`6ab6b9b9 docs: audit architecture and prioritize refactoring`

新增 `docs/architecture-review-2026-09-11.md`，完成 Desktop、Rust runtime、Mobile、Cloud、Edge、CI、release/portable、Workbench、Terminal、Git/editor、agent/settings 的整體盤點。

結論與優先順序：

- Phase 0：architecture guard / CI / smoke fixture。
- Phase 1：lifecycle error taxonomy / adapter conformance / snapshot retry policy。
- Phase 2：Rust ServerActor admission、job ownership、transaction/replay。
- Phase 3：Workbench capability owner 拆分。
- Phase 4：Terminal、Git/editor、Mobile transition。
- Phase 5：agent capability matrix、release/platform contract、observability。

### 3.2 Phase 0：架構護欄已完成核心工作

`fa472626 test(architecture): enforce runtime guard in CI`

已完成：

- `runtime_architecture_guard.dart` 支援解析 relative dependency path。
- 支援追蹤 barrel export / re-export，避免 application 透過 barrel 間接碰 presentation。
- 新增 `tool/quality/runtime_architecture_guard_test.dart`。
- fixture 覆蓋：
  - direct application -> presentation：必須 fail。
  - relative cross-feature Workbench infra：必須 fail。
  - barrel export 間接 presentation：必須 fail。
  - 合法 application -> domain：必須 pass。
  - PR workflow 必須真的執行 guard。
- `.github/workflows/pr.yml` 的 static job 已執行：

```text
dart tool/quality/runtime_architecture_guard_test.dart
dart tool/quality/runtime_architecture_guard.dart
```

目前 `dart tool/quality/runtime_architecture_guard.dart` 已重新驗證通過。

### 3.3 Phase 0：top-level smoke DB fixture 已隔離

`15c2e682 test(app): isolate top-level smoke composition`

已完成：

- Widget smoke 不再直接開正式 DB path。
- 每個 case 使用自有 in-memory DB。
- fixture 與 `main.dart` composition-root overrides 對齊。
- test teardown 先卸載 UI，再 `await db.close()`。
- 原先 Drift multiple `AleraDatabase` warning 已消失。

這只修 test ownership，不代表有 production DB corruption，也沒有改 production DB policy。

### 3.4 Phase 1：shutdown timeout 不再被誤當成功

`91638cb7 fix(runtime-host): preserve shutdown timeout uncertainty`

已完成的契約：

- connection closed 與 request timeout 不再被 adapter 合併成同一 shutdown 成功語意。
- request timeout 代表 **outcome unknown**。
- 顯式 Stop：unknown 不可被吞成已停止。
- Update：unknown 後不可啟動 replacement runtime。
- App Quit：仍保留快速關窗，不要求 slow cleanup 阻塞視窗消失。

這是重要 correctness boundary，後續不要退回「所有 transport error 都等於 shutdown accepted」。

### 3.5 Phase 1：snapshot permanent failure 不再無限 retry

`e68c6e24 fix(runtime): avoid retry loops on permanent snapshots`

已完成：

- request timeout 等暫時錯誤仍可重連/retry。
- `FormatException` 等永久/協定類錯誤不再自己啟動無限 timer retry。
- stream 不會因此永久關閉；後續 reconnect/event 仍可恢復。
- failure signal seam 已建立，但目前沒有為此硬造第二套 global health registry。

後續若做 UI/runtime health，應把 retrying/stale/permanent failure 納入既有 health owner，而不是新增跨 feature 全域 provider。

---

## 4. Phase 2 已完成：ServerActor mailbox 與 background I/O

### 4.1 Sidebar snapshot 移出 actor mailbox

`123c332f refactor(runtime): defer sidebar snapshot reads`

`workspaceSidebar.snapshot` 不再在 actor 主 loop 裡等待整組 SQLite snapshot read。改為 worker -> `ServerCommand` completion -> actor 套用結果。

驗證包含：

- snapshot 工作被 barrier 卡住時，`status.get` 仍可前進。
- client disconnect 後晚到 completion 不回寫。

### 4.2 Blocking directory reads deferred

`1b796203 refactor(runtime): defer blocking directory reads`

`hostDirectory.list` 改走 generic deferred blocking request，不再在 mailbox 內同步 filesystem list。

### 4.3 Git metadata reads deferred，並共用 loader

`9d5e5e68 refactor(runtime): defer git metadata reads`  
`df0022ae refactor(runtime): share git metadata loaders`

已處理：

- `project.branches.list`
- `workspace.repositoryWebUrl`

DB lookup 在 deferred future 裡；libgit2 / blocking Git work 放 blocking pool。direct fallback 與 deferred path 共用 domain loader，避免兩套行為漂移。

### 4.4 Effective project config deferred + async config file I/O

`4b7f17bb refactor(runtime): defer effective project config`

`projectConfig.effective` 已離開 actor mailbox；`alera.toml` 讀取也改 async I/O。

### 4.5 Automation policy read-only show deferred

`fe482c23 refactor(runtime): defer automation policy reads`

只有純讀 `kind=show` 且沒有 `run` identity 的 request 會 deferred。

刻意保留在 actor 的：

- `kind=agent`
- `kind=project`
- 帶 `run` 的 show

原因是 mutation ordering / live-run identity 仍由 actor 擁有，不能為了去阻塞把 mutation ownership 一起丟出去。

### 4.6 Background I/O active concurrency 已有 8-slot budget

`4073e2a2 refactor(runtime): bound deferred request work`

ServerActor 現在有：

```text
DEFERRED_REQUEST_CONCURRENCY = 8
Arc<Semaphore> deferred_request_slots
```

重要語意：

- 最多 8 個 active background read/I/O work。
- actor 本身不等待 permit。
- worker task 在 actor 外等待 permit，所以 control mailbox 不被 semaphore 卡住。

**尚未完成的部分：** 等待 permit 的 task 數仍可累積。除了 sidebar single-flight 外，現在沒有完整的 bounded pending queue / typed overload rejection。這是後續 Phase 2 的待辦，不能把「8 active」誤稱成「總佇列有界」。

### 4.7 Sidebar snapshot single-flight

`ee970099 refactor(runtime): coalesce sidebar snapshot reads`

同一 actor 同時收到多個 sidebar refresh 時：

- 只啟動一個 DB load。
- 每個 request-id 都保留 waiter。
- completion fan-out 給所有仍 authenticated 的 waiter。
- completion 後 state 清空，下一次 refresh 可重新啟動。

不能把 coalesce 寫成「直接丟掉舊 request」，每個 RPC 必須有回覆。

### 4.8 `project.register` 改成 prepare/commit ownership

`147d73de refactor(runtime): defer project registration preparation`

已拆成：

- background preparation：path validation / canonicalize / Git branch detection。
- actor completion：DB commit、duplicate handling、broadcast、RPC response。

preparation 未完成前 runtime store 不會被 mutation；`status.get` 可先完成。

這個模式是後續 mutation 重構的範本：**prepare outside actor, commit inside owner**。

### 4.9 Prompt image/file uploads 已 deferred 並接 client ownership

相關 commits：

- `7ae7e6c4 refactor(runtime): defer prompt image uploads`
- `577f589a refactor(runtime): bound prompt file upload work`
- `bd5cfb7b refactor(runtime): bound disconnected upload cleanup`
- `9b0862c4 fix(runtime): clean prompt image uploads on disconnect`
- `56dd0291 refactor(runtime): defer orphaned upload cleanup`

目前契約：

- `mobile.promptImage.start/chunk/complete/cancel` 全部 deferred。
- prompt image/file 都使用 shared 8-slot I/O admission。
- 同一 upload 有 per-upload gate，保留 start/chunk/complete/cancel 序列語意。
- start 成功後才記錄 per-client upload ownership。
- complete/cancel 解除 ownership。
- client disconnect 立即排入 bounded cleanup。
- start worker 已成功、但 client 在 completion delivery 前斷線的 orphan，也會排 background cleanup。
- cleanup 不在 actor completion handler 同步刪檔。

曾檢查 id-less upload frame；現有 `extract_request()` 已要求 request id，因此無 id frame 會在 routing gate 被拒絕，不存在另一條同步 upload 旁路。不要再重做這一題。

### 4.10 Dispatch context cleanup 已 background + generation safe

`848b2c3c refactor(runtime): defer dispatch context cleanup`

`remove_dispatch_context` 原本同步 `remove_file`，現在：

- 使用 shared background I/O budget。
- per-context path generation gate。
- 舊 cleanup 若晚於新 install，不可把新 context 刪掉。

**只完成 cleanup；install 尚未完成。** `install_dispatch_context()` 現在仍會同步：

```text
create_dir_all
write / OpenOptions + write_all
```

而且 install 必須先成功，worker preamble 才能安全注入。因此不能直接 fire-and-forget；詳見後續步驟。

### 4.11 Agent hook reconcile 已 latest-wins background 化

`1442859f refactor(runtime): defer agent hook reconciliation`

新增 `host_service_agent_integrations.rs`，目前行為：

- `agentStatusHooks` 先由 actor persist setting / 更新 presence / broadcast。
- filesystem hook reconcile 不再讓 `apply_mobile_runtime_settings()` 等 blocking I/O。
- 同 runtime 有 serial gate。
- generation latest-wins：尚未開始的 stale work 跳過。
- 已開始的舊 work 結束後，最新 generation 最後執行，最終磁碟狀態收斂到最新 setting。
- 先取得 per-runtime serial gate，再拿 shared I/O permit，避免 stale jobs 先占滿全域 slots。
- runtime startup 原本就會依 persisted settings reconcile，因此程序剛好退出時不會永久失去設定意圖。

actor-level test 已證明 shared I/O permit = 0 時，settings persistence / response 仍可完成。

---

## 5. 已完成的 max-lines/test debt 清理

這一段是為了讓新的 PR static gate 最終能真的綠。這些 commit 都是純測試結構拆分，沒有 production 行為變更。

- `b3fec239 refactor(runtime): split oversized request tests`
- `fa6c0b53 refactor(runtime): split deferred request tests`
- `1561063d refactor(runtime): split oversized resource tests`
- `76f1b8ab refactor(runtime): split small oversized tests`
- `0d133c4c refactor(tests): split small Dart test fixtures`
- `5e703d12 refactor(tests): split app window prompt fixtures`
- `285b415e refactor(tests): split oversized widget cases`

本輪開始清 debt 時 max-lines offender 約 28 個；目前重新執行後剩 **14 個**。

### 5.1 目前剩餘 14 個 offender

```text
1625  lib/src/app/localization/alera_localizations.dart
 501  lib/src/features/settings/domain/alera_settings.dart
 513  lib/src/features/settings/presentation/settings_dialog.dart
 537  lib/src/features/workbench/application/workspace_service.dart
 580  lib/src/features/workbench/presentation/workbench_dialog_launchers.dart
1067  rust/src/api/git_diff_impl.rs
1405  rust/alera-cli/src/managed_workspace.rs
2172  rust/alera-cli/src/terminal_host/server/orchestration_requests.rs
 582  rust/alera-cli/src/terminal_host/server/project_requests.rs
1734  rust/alera-cli/tests/terminal_host_conformance.rs
 711  test/unit/workbench_controller_test_harness.dart
 865  test/unit/workbench_controller_view_prefs_test_cases.dart
 954  test/widget/settings_dialog_core_test_cases.dart
1372  test/widget/workspace_explorer_test.dart
```

`dart tool/quality/check_max_lines.dart` **目前仍 exit 1**。這是已知狀態，不可在交接中宣稱 CI 全綠，也不要直接 `--write-baseline` 把 debt 接受掉。

---

## 6. 最近完整 commit 序列

以下是從架構盤點開始到目前 HEAD 的 relevant history，下一位可用來定位每批意圖：

| Commit | 說明 |
| --- | --- |
| `6ab6b9b9` | docs: audit architecture and prioritize refactoring |
| `fa472626` | test(architecture): enforce runtime guard in CI |
| `91638cb7` | fix(runtime-host): preserve shutdown timeout uncertainty |
| `e68c6e24` | fix(runtime): avoid retry loops on permanent snapshots |
| `15c2e682` | test(app): isolate top-level smoke composition |
| `123c332f` | refactor(runtime): defer sidebar snapshot reads |
| `1b796203` | refactor(runtime): defer blocking directory reads |
| `9d5e5e68` | refactor(runtime): defer git metadata reads |
| `df0022ae` | refactor(runtime): share git metadata loaders |
| `4b7f17bb` | refactor(runtime): defer effective project config |
| `fe482c23` | refactor(runtime): defer automation policy reads |
| `4073e2a2` | refactor(runtime): bound deferred request work |
| `ee970099` | refactor(runtime): coalesce sidebar snapshot reads |
| `147d73de` | refactor(runtime): defer project registration preparation |
| `7ae7e6c4` | refactor(runtime): defer prompt image uploads |
| `577f589a` | refactor(runtime): bound prompt file upload work |
| `848b2c3c` | refactor(runtime): defer dispatch context cleanup |
| `bd5cfb7b` | refactor(runtime): bound disconnected upload cleanup |
| `9b0862c4` | fix(runtime): clean prompt image uploads on disconnect |
| `56dd0291` | refactor(runtime): defer orphaned upload cleanup |
| `1442859f` | refactor(runtime): defer agent hook reconciliation |
| `b3fec239` | refactor(runtime): split oversized request tests |
| `fa6c0b53` | refactor(runtime): split deferred request tests |
| `1561063d` | refactor(runtime): split oversized resource tests |
| `76f1b8ab` | refactor(runtime): split small oversized tests |
| `0d133c4c` | refactor(tests): split small Dart test fixtures |
| `5e703d12` | refactor(tests): split app window prompt fixtures |
| `285b415e` | refactor(tests): split oversized widget cases |

前一階段另外有：

- `90360f96 test(runtime-host): align status panel lifecycle contract`
- `c1d88258 fix(orchestration): replay committed completion safely`

---

## 7. 驗證狀態與測試注意事項

### 7.1 已重新確認

撰寫交接時重新執行：

```text
dart tool/quality/runtime_architecture_guard.dart
```

結果：PASS。

重新執行：

```text
dart run tool/quality/check_max_lines.dart
```

結果：FAIL，原因只有目前列出的 14 個 max-lines offender；不是新的 architecture guard failure。

### 7.2 過去各批已跑過的 affected regressions

依變更不同，曾跑過並通過：

- runtime host lifecycle service / quit / status panel focused tests。
- runtime snapshot stream focused tests。
- top-level widget smoke。
- Rust deferred request tests。
- workspace sidebar request tests。
- request routing tests。
- client delivery/disconnect tests。
- project management tests。
- prompt image/file tests。
- orchestration conformance/review regressions。
- host service request tests。
- 對應 Flutter widget/unit entrypoints。
- changed-file format / `git diff --check`。

**沒有宣稱全 repo suite 通過。** 尚未在這個交接點重新跑完整 Rust workspace、所有 Flutter、Mobile、Cloud、Edge、native E2E、soak、release build 或 security certification。

### 7.3 Windows / toolchain 注意事項

曾遇到兩類環境問題，不要誤判成程式 regression：

1. Flutter native hook 曾因本機沒有 `zig` 失敗。當時使用既有 Windows `ghostty-vt.dll` prebuilt，透過 `GHOSTTY_VTE_PREBUILT` 跳過 Zig build。若再次出現相同問題，先沿用既有 prebuilt 流程，不要為測試任意改 production source。
2. 平行 Cargo 測試曾出現 Windows linker `LNK1104`，原因是另一個 test exe 被同 target graph 占用。等鎖釋放後單獨重跑 focused test 即可；不要看到 `exit 101` 就先改 Rust code。

另外，早期 `cargo fmt --all --check` 曾看到 repo-wide 與本批無關的格式 debt。重構小批優先對 changed Rust files 做 rustfmt，再搭配 `git diff --check`；若要修 repo-wide format，應另開獨立批次，不要混進 correctness commit。

---

## 8. 下一批：已分析、尚未修改的 Workbench test debt

目前 HEAD 乾淨，下面工作只有分析，**尚未落任何未提交檔案**。

### 8.1 `workbench_controller_test_harness.dart` 711 行

已定位：

- `_FakeWorkbenchRepository` 從約 line 323 到檔尾，約 389 行。

下一步：

1. 新增例如：
   `test/unit/workbench_controller_fake_workbench_repository.dart`
2. 檔頭：
   `part of 'workbench_controller_test.dart';`
3. 原樣搬移 `_FakeWorkbenchRepository`，不要改行為。
4. 在 `workbench_controller_test.dart` 的 parts 列表加入新 part。
5. 原 `workbench_controller_test_harness.dart` 保留 harness、graph repo、worktree runner、project repo 等其餘 fixture。
6. 跑完整：
   `flutter test test/unit/workbench_controller_test.dart`

預期 root harness 可降到約 320 行。

### 8.2 `workbench_controller_view_prefs_test_cases.dart` 865 行

已定位兩個自然 capability 區塊：

- 約 line 306-602：folder workspace / source-control focus / stale focus completion。
- 約 line 710-865：project/workspace/tab/layout failure surfaces。

建議拆成：

```text
test/unit/workbench_controller_source_control_focus_test_cases.dart
test/unit/workbench_controller_failure_surface_test_cases.dart
```

各自：

- `part of 'workbench_controller_test.dart';`
- 提供註冊函式，例如：
  - `_registerWorkbenchControllerSourceControlFocusTests()`
  - `_registerWorkbenchControllerFailureSurfaceTests()`
- 在 `workbench_controller_test.dart` 的 group 中維持原測試註冊順序。
- 不改 fixture、不改 assertion、不改 production。

這批完成後預期 max-lines offender **14 -> 12**。

### 8.3 此批驗證

```text
flutter test test/unit/workbench_controller_test.dart
dart format --set-exit-if-changed <changed dart files>
dart tool/quality/check_max_lines.dart
git diff --check
```

注意 `check_max_lines` 在 offender 尚未清完前仍會 exit 1；驗收是 offender 數從 14 降至 12，而且本批目標檔離開清單。

完成後小批提交，不 push。

---

## 9. 接著清理剩餘 test-only max-lines debt

WorkBench 兩檔完成後，優先處理不影響 production 的測試檔：

### 9.1 `test/widget/settings_dialog_core_test_cases.dart` 954

按設定 capability 拆 part，不要按固定 500 行硬切。建議先依 existing test names 分：

- General / appearance / editor 類。
- Runtime / agent / integration 類。
- update / validation / error state 類。

保留同一 `settings_dialog_*_test.dart` root library，讓 private fixture 不必改 public。

### 9.2 `test/widget/workspace_explorer_test.dart` 1372

先找既有 helper 與測試主題，優先拆：

- file tree / navigation。
- selection / reveal / rename。
- error / stale async / repository switch。

若它本來沒有 part 架構，先把共同 fixture/support 抽成 part，再把 cases 分組；不要為拆檔改 production interface。

### 9.3 `rust/alera-cli/tests/terminal_host_conformance.rs` 1734

這是 integration/conformance test，不是 production owner。按協定 capability 分成多個 integration test files 或 shared support + cases，例如：

- lifecycle / hello / auth。
- terminal/session/output。
- workspace/project。
- orchestration。
- mobile/relay/capability downgrade。

重點是每個新 entrypoint 都仍用同一 conformance harness，不能因拆檔減少 coverage。

完成 test-only debt 後，再進 production max-lines。不要在這之前為了讓 gate 變綠直接寫 baseline。

---

## 10. Production max-lines：不能只為數字拆

目前 production offenders：

```text
lib/src/app/localization/alera_localizations.dart          1625
lib/src/features/settings/domain/alera_settings.dart        501
lib/src/features/settings/presentation/settings_dialog.dart 513
lib/src/features/workbench/application/workspace_service.dart 537
lib/src/features/workbench/presentation/workbench_dialog_launchers.dart 580
rust/src/api/git_diff_impl.rs                              1067
rust/alera-cli/src/managed_workspace.rs                    1405
rust/alera-cli/src/terminal_host/server/orchestration_requests.rs 2172
rust/alera-cli/src/terminal_host/server/project_requests.rs 582
```

建議順序不是從最短開始，而是對齊架構 ownership：

1. `project_requests.rs`
2. `orchestration_requests.rs`
3. `managed_workspace.rs`
4. `workspace_service.dart`
5. `workbench_dialog_launchers.dart`
6. `git_diff_impl.rs`
7. settings domain/presentation
8. localization catalog

### 10.1 `project_requests.rs` 582

目前同檔混有：

- project register prepare/commit。
- rename/remove preview。
- effective config / branch / directory compatibility entrypoint。
- clone job lifecycle。

優先按 capability 拆：

- `project_registration_requests.rs`
- `project_clone_requests.rs`
- `project_query_requests.rs`

不要只是把 functions 搬到任意檔；`project.register` 的 prepare-background / commit-actor ownership 要保留。

### 10.2 `orchestration_requests.rs` 2172

這是最高價值 production 拆分，但不能一次大搬。

建議按 domain capability 小批移：

1. message send/check/reply/inbox。
2. waiters / ask / wait response。
3. task lifecycle。
4. gate lifecycle。
5. run lifecycle。
6. dispatch lifecycle。
7. dispatch context/token/schema。
8. coordinator transfer/status。

每搬一個 capability：

- 先跑對應 conformance/regression。
- 保留 `ServerActor` owner。
- 不把 transaction 拆成 caller 自己拼低階 store calls。
- 如果搬移後只是同一 actor impl 在不同檔，至少要讓 dependency 與 capability 邊界清楚；後續才能再抽真正 owner。

### 10.3 `managed_workspace.rs` 1405

先分清：

- validation / path plan。
- Git/worktree execution。
- cleanup/recovery。
- automation-owner safety check。
- request/result models。

不要先拆所有 helper；優先把 pure plan/validation 與 side-effect executor 分開，才能做 deterministic tests。

### 10.4 `git_diff_impl.rs` 1067

建議切：

- path/repository scope normalization。
- diff/patch parse。
- blob/content loading。
- native backend execution。

既有 Git diff lazy-load / stale scope protections必須保留。後續 application loader/cache 重構會依賴這些 seam。

### 10.5 `alera_localizations.dart` 1625

這類檔案大不代表 owner 問題，但 max-lines gate 已要求處理。建議把「catalog data」按 feature 拆檔，保留單一 `AleraLocalizations` facade：

```text
localization/catalog_core.dart
localization/catalog_workbench.dart
localization/catalog_settings.dart
localization/catalog_runtime.dart
...
```

不要把品牌/模型/branch/workspace/project/profile 名稱翻譯規則改掉，也不要因拆 catalog 改 lookup fallback semantics。

---

## 11. Phase 2 下一個 substantive correctness：Dispatch Context install

這是目前 runtime actor 重構最值得接續、但也最不能草率的一批。

### 11.1 現況

`remove_dispatch_context()` 已 background + generation safe；`install_dispatch_context()` 仍同步執行：

- `create_dir_all`
- serialize token JSON
- `OpenOptions` / `write_all` 或 `std::fs::write`

它通常發生在 orchestration dispatch/run 路徑。context 必須在 worker preamble/contract 使用前完成，因此不能改成單純 fire-and-forget。

另外 `orchestration.dispatch` 還有 internal callers，例如 agent-spawn / pending dispatch replay 類流程；不能只改 RPC routing 而讓內部 caller 維持另一套同步 path。

### 11.2 先寫的 red tests

至少固定四個行為：

1. context install 被 barrier 卡住時，actor control request（例如 status）仍能先完成。
2. install 尚未成功時，不得啟動需要該 context 的 worker/preamble injection。
3. 舊 generation cleanup / completion 不得覆蓋或刪除新 context。
4. client/run/dispatch owner 已失效時，晚到 preparation completion 不得 commit 新 dispatch state。

如果要額外驗證 retry：

5. context write 成功但 RPC reply/後續 step lost 時，重入必須遵守 dispatch instance / idempotency contract，不可產生第二個不同 worker owner。

### 11.3 建議實作方向

不要讓 actor `await spawn_blocking(write).await`，那仍會卡 mailbox。

建議做兩階段 state machine：

```text
Actor validates dispatch mutation + reserves generation/owner
    -> background install context (shared I/O admission)
    -> ServerCommand::DispatchContextPrepared/Installed
    -> actor re-validates generation / dispatch owner
    -> commit dispatch transition
    -> start worker / inject preamble
```

需要一個明確的 in-flight dispatch ownership/token。不要直接把這件事硬塞進 generic read-side `DeferredRequestFinished`，因為這是 mutation continuation，不只是 RPC read result。

如果既有 runtime mutation queue 可以自然承接 dispatch mutation token，優先共用；若不能，建立 orchestration-specific in-flight guard，但不要創造第二套任意 mutation queue。

---

## 12. Phase 2 其他仍待 audit / 改善的 actor stall 與 admission

### 12.1 `runtimeSettings.update` 的 automation autostart reconcile

`host_service_requests.rs` 目前對 `automation` setting：

1. 先 persist `RuntimeAutomationSettings`。
2. 接著同步呼叫 `reconcile_autostart(...)`。

這是 filesystem side effect，而且 actor 仍等待它。

但不能直接照 agent hooks fire-and-forget，因為現有 RPC 語意在 reconcile error 時會回 error，而 persisted setting 已先寫入。先決定產品契約：

- A：setting persistence 成功即成功，autostart reconcile failure 變 warning/event，startup 下次再收斂。
- B：autostart failure 必須 rollback persisted setting。
- C：維持 current error response，但 reconcile background 後需要兩階段 completion。

先用 TDD 固定期望，再改。不要無意間改錯誤語意。

### 12.2 Skill install 後 orchestration hook reconcile

`start_skill_install_request()` 本身已在 spawned task，但 orchestration skill 成功後會 `spawn_blocking(reconcile_agent_integrations)`，目前沒有明確使用 shared deferred I/O budget。

這不會卡 actor mailbox，但可能形成 unbounded blocking work。應 audit：

- 是否可接 shared 8-slot budget。
- 或 host-tool 類工作是否應有獨立 budget，避免大型 install/reconcile 把 read-side slots 全占滿。

先量測/測試，不要假設共用同一 semaphore 一定最佳。

### 12.3 `workspace.removeManaged` preflight

`try_start_deferred_request()` 的 `workspace.removeManaged` 分支在啟動 runtime mutation 前，仍可能 `await workspace_has_active_automation_owner(...)`。

因為 `try_start_deferred_request()` 本身是 actor request routing 的 await path，這類 preflight DB work 仍可能讓 mailbox 停住。

建議 red test：

- block active-automation-owner preflight。
- 同時送 control status/quit。
- control 必須可先前進。

若 red，將 preflight 納入 runtime mutation preparation，而不是在 router await。

### 12.4 Deferred pending task 數仍可能無界

目前只有 active I/O = 8 有界。大量不同 request 都可 `tokio::spawn` 後等 semaphore，因此 pending task 仍可能累積。

建議先加可觀測資料：

- queued count
- active count
- queue wait ms
- request type
- cancellation/disconnect count

再做 deterministic overload test。可能方案：

- 總 pending capacity，例如 active + queued 有上限。
- 可 coalesce 的 request（sidebar/search refresh）優先 single-flight/coalesce。
- 不可 coalesce 的 user operation 超限時回 typed backpressure/busy error。

不要用 blocking semaphore acquisition 放回 actor。

---

## 13. Phase 2：資料權威、transaction、idempotency/replay

這部分仍未完成，不要被 mailbox refactor 取代。

針對以下 domain 建立 operation contract 表：

- Project
- Workspace
- Tab/Layout
- Orchestration
- Shared Workbench Preferences

每個 operation 至少回答：

1. authority 在哪裡。
2. instance/revision 如何驗證。
3. operation ID 是否可 replay。
4. 同 operation ID + 不同 payload 怎麼處理。
5. DB commit 與 event publish 順序。
6. reply lost 後 client retry 的結果。
7. restart 後如何恢復。
8. stale owner / wrong assignee / disconnected client 如何處理。

### 13.1 高價值測試

- DB commit 成功、reply 丟失，retry 只能有一次業務效果。
- stale instance / wrong assignee / altered replay payload 必須有明確 typed result。
- persistence 成功、event publish 前注入 failure，restart/resync 後資料仍一致。
- tab/layout 更新中間失敗不得留下 activeTab/layout 不變條件破壞。
- migration 中途 failure 後 restart 可繼續。

`c1d88258` 已處理的 orchestration completion replay 不要重寫；只補缺少的矩陣情境。

#### Orchestration domain 已完成

- `orchestration_requests.rs`（原 2151 行最大 offender）按 capability 拆成 message/task/dispatch/completion/run/gate 六個 request 檔；message waiter 機械併入 `orchestration_wait_requests.rs`，result schema 驗證併入 `orchestration_validation.rs`，主檔只留 router + agent presence 入口。拆後最大 478 行，全部低於 500。
- Contract matrix 落在 `docs/orchestration-operation-contract.md`：每個 operation 的 authority、instance 驗證、replay 語意、commit/publish 順序、lost-reply retry、restart 恢復、stale owner 處理都已列明；`send`/`reply`/`ask`/`escalate` 無 client idempotency key（retry 會寫重複列）是目前接受的缺口，已在文件標註。
- 新測試 `orchestration_contract_tests.rs` 補上缺口情境：dispatch retry 失敗收斂、dispatchAccept 重放保留首次 accepted_at、taskCancel/gateResolve/heartbeat/taskRecover/workerDone 的 typed 拒絕。

#### Project domain 已完成

- Contract matrix 落在 `docs/project-operation-contract.md`：涵蓋 `project.register`/`rename`/`upsert`/`remove(.preview)`/`list`/`branches.list`、`projectConfig.*`、`hostDirectory.*`、`project.clone.*` 每個 operation 的 authority、replay 語意、commit/publish 順序、lost-reply retry、restart 恢復。已記錄的缺口：`clone.start` 無 `(parent_path, directory_name)` dedupe key，重複 start 可能讓兩個 runner 競爭同一 destination。
- 新測試 `project_contract_tests.rs` pin 住三個關鍵格：register retry 回同一 project 不複製、`project.remove` 二次呼叫為零刪除的冪等成功、terminal 狀態 clone job 的 cancel 回存 job 列而非錯誤。deferred read 面（branches/effective config/host directory/disconnect drop）已由 `deferred_project_requests_tests.rs` 覆蓋。
- 此 domain 只補文件與測試，未改 production code：`project_requests.rs`/`project_clone_requests.rs`/`runtime_mutations.rs` 的 ownership 拆分在 Batch E/P2 已就位，無需再拆。

#### Workspace domain 已完成

- Contract matrix 落在 `docs/workspace-operation-contract.md`：涵蓋 `workspace.*` CRUD/pin/rename、managed lifecycle（`createManaged`/`runSetup`/`storageImpact`/`switchBranch`/`sleep`/`remove*`）、`workspaceTag.*`、`workspaceRelation.*`、`workspaceSection.*`、`workspaceActivity.*`、`linkedReview.*`、`layout.*`、`workspaceCascade.preview`。三個執行面已列明：mailbox CRUD、spawn 型 deferred job（`managed_workspace_jobs` gate shutdown timer）、runtime-mutation worker。
- 已記錄缺口：`workspaceTag.create` 無 name 唯一檢查（retry 產生同名不同 id 的 tag）；`createManaged` retry 是 fail-closed 而非回傳首次建立的 workspace。
- 新測試 `workspace_contract_tests.rs` pin 住：`workspace.remove`/`workspace.sleep` 二次呼叫為零刪除冪等、`layout.upsert` 同 workspaceId last-write-wins。其餘格子由 `managed_workspace_cleanup_tests`/`preflight_tests`/`workspace_section_requests_tests`/`runtime_mutations/tests.rs` 既有覆蓋。

#### Tab/Layout domain 已完成

- Contract matrix 落在 `docs/tab-layout-operation-contract.md`：涵蓋 `tab.list`/`find`/`upsert`/`rename`/`remove`/`removeForWorkspace` 與 `layout.find`/`upsert`/`remove`。關鍵不變量已列明：tab payload 的 host-owned 欄位（`agentTitle*`）client 不可覆寫、`spawnOnCreate` spawn 失敗時 tab 列 rollback（先刪列再終止 session）、`tab.remove` 在 enqueue 前先於 actor 取消 title job、startup `reconcile_spawn_on_create_tabs` 對失敗恢復採同樣的刪列語意。
- 新測試 `tab_layout_contract_tests.rs` pin 住：`tab.remove`/`tab.removeForWorkspace` retry 冪等、spawn-on-create 失敗不留 tab 列不留 session。

#### Shared Workbench Preferences domain 已完成

- Contract matrix 落在 `docs/shared-prefs-operation-contract.md`：涵蓋 `workbenchViewPrefs.get`/`update` 與 `workspaceActivity.*`。此 domain 的核心規則是非對稱樂觀鎖：desktop 是權威 writer 恆 last-write-wins，mobile 必須帶符合的 `expectedRevision` 否則收 typed conflict；`sectionSort`/`collapsedSectionIds`/`othersSectionCollapsed` 三個 legacy key 在寫入前從現值 backfill。
- `workbench_shared_state_store_tests.rs` 新增 `desktop_writer_bypasses_the_revision_check`，與既有 mobile stale-revision 拒絕、activity max-wins、tab remove 冪等、sleep cascade 範圍測試互補。

#### Agent/Settings domain 已完成

- Contract matrix 落在 `docs/agent-settings-operation-contract.md`：涵蓋 `agentProfile.*`（含 `launchIdempotent` 的 `clientMutationId` receipt）、`agentPresence.list`、`agentQuota.*`/`agentUsage.snapshot`、`runtimeSettings.*` 與 `mobile.*` 變體、`aiText.*`、`aiDictation.*`、`host.*`、`configure`、`status.get`，並列出三個執行面（mailbox op、spawned job、Maintenance deferred）與 settings update 的 deferred side effects。
- 新測試 `agent_settings_contract_tests.rs` pin 住：`agentProfile.upsert` stale `expectedRevision` 回 typed conflict 且不覆寫、`agentProfile.remove` 區分 format error / typed conflict / 冪等 retry、`runtimeSettings.update` 重放冪等、`mobile.runtimeSettings.update` 拒絕 desktop-only key、`status.get` 形狀穩定。
- 記錄的缺口：`runtimeSettings.update` 逐 key 寫入（mixed payload 可能部分提交）且無 revision/OCC、`agentQuota.consumeCodexResetCredit` 無 idempotency key、name 衝突與 stale-title 失敗是 plain `state` error。

#### Terminal domain 已完成

- Contract matrix 落在 `docs/terminal-operation-contract.md`：涵蓋 `createOrAttach`/`write`/`resize`/`terminal.read`/`setOutputPaused`/`detach`/`terminate`、mobile `terminal.create`/`attach`/`restart`/`reclaim`、`terminal.driver.*`/`runningProcesses`/`terminal.pulse.*`，並補一張「Internal machinery」表（output 雙軌 batch、resync tick、durable barrier、checkpoint、PTY exit、client disconnect、idle shutdown、spawnOnCreate 恢復、instance-id fencing、`restore_exited`）。
- 新測試 `terminal_contract_tests.rs` pin 住：live session attach 不 re-validate metadata、`terminal.restart` metadata 不符 fail-closed、`write` 三種錯誤面、`detach` 保留 PTY + checkpoint 而 `terminate` 刪除 tab+session+history、`dispose_client` 不殺 PTY且過期 shutdown tick 被忽略。
- 記錄的缺口：`write` 無 op id（lost reply retry 會重複寫入）、`terminate` 非冪等（retry 回 `not attached`，與從未 attach 無法區分）、`terminal_input_backpressure` 是字串前綴而非 typed error。

---

## 14. Phase 2/3：跨語言 wire contract

Desktop、Mobile、Rust 各自仍有 protocol parsing/DTO 宣告。已有 old-host compatibility 與 relay cross-language fixture，但核心 workspace/tab/lifecycle contract 還不夠集中。

下一步不是立刻換 RPC framework，而是先建立 versioned golden fixtures：

1. workspace lifecycle。
2. tab/layout。
3. shutdown/lifecycle errors。
4. revision/conflict errors。
5. terminal binary frame/output resync。
6. unknown fields / missing optional fields。
7. old capability downgrade。

同一 fixture 由 Rust producer 與 Desktop/Mobile consumer round-trip。等 fixture 穩定後才評估 shared schema/codegen。

#### P5 首批已完成（`ef703467`）

- Golden fixtures 落在 `test/fixtures/wire/`（repo root，Rust 與 Dart 讀同一份檔）：response envelope（ok/unauthenticated/format/conflict）、`tab.list`/`tab.find`/`tab.upsert` 的 desktop vs mobile 投影差異、`workbenchViewPrefs.update` mobile conflict、agentProfile remove typed conflict、四個 broadcast event 形狀。
- 三側驗證：Rust `wire_fixture_tests.rs`（10 tests，真實 `ServerActor` 路徑逐欄位比對）、Desktop `terminal_host_wire_fixtures_test.dart`（6 tests，loopback socket）、Mobile `mobile_runtime_wire_fixtures_test.dart`（3 tests，WebSocket upgrade）。
- 過程中 pin 住的非直覺契約：mobile prefs conflict 是 untyped error（只有 `agentProfile.remove` 走 typed `errorCode`）；`projectCloneJobsChanged` 不在 desktop `runtimeHostEventNames` allowlist（desktop 丟棄、mobile 全收）；`agentTitleStatus: generating` 投影正規化為 `failed`。
- 未覆蓋的 fixture 面向（下一批接著做）：workspace lifecycle、shutdown/lifecycle errors、terminal binary frame/output resync、unknown/missing fields、old capability downgrade。

---

## 15. Phase 3：Workbench 真正 owner 拆分

目前 test file 拆分只是在清 CI debt，**不是 Workbench architecture refactor 本身**。

目標 owner：

### 15.1 Catalog / read model owner

負責：

- projects/workspaces catalog。
- external workspace sync/watch。
- project/workspace read model refresh。

第一個 TDD：外部 workspace 刪除與 bootstrap/refresh 交錯，stale result 不可復活已刪 workspace。

### 15.2 Tab/Layout owner

負責：

- tabs。
- preview tab。
- pane groups/layout。
- active tab invariant。
- persistence ordering。

第一個 TDD：workspace switch 同時收到 external tab removal；active tab/layout 不可指向不存在的 tab。

### 15.3 Selection/Navigation owner

負責：

- active project/workspace/tab selection。
- window-local navigation history。
- source-control focused root selection。

不要把 server-side shared view prefs 與每個 window 的 selection 混成同一 authority。

### 15.4 Resource lifecycle owner

負責：

- workspace subscriptions。
- editor sessions。
- terminal leases/attachments。
- retirement/cleanup。

第一個 TDD：切 workspace / 關 tab / 外部移除交錯時 resource 只 release 一次，durable PTY 不因 UI detach 被 terminate。

### 15.5 演進策略

- 保留 `WorkbenchController` facade，UI 先不用全面改 call site。
- 每批移「state + lifecycle + tests」，不要只抽 callback-heavy coordinator。
- owner 要可以獨立 fake/test。
- 只有當 old Internals 實際減少 ownership 時才算完成。
- 不要一個 state field 一個 Riverpod provider；先守住跨欄位 invariants。

### 15.6 P6 盤點結論（2026-09-12）

`workbenchControllerProvider.notifier` 共 54 個呼叫點，全部維持 facade 簽名。`WorkbenchState` 欄位分桶：`projects`/`workspacesByProject`/`sections` 屬 Catalog；`tabsByWorkspace`/`layoutByWorkspace` 屬 Tab/Layout；`activeProjectId`/`activeWorkspaceId`/`activeTabIdByWorkspace`/`searchQuery`/`collapsed` 屬 Selection；`viewPrefs` 為 server-shared 暫留 facade；`bootstrapped`/`error` 留 facade。

執行順序（依風險由低到高）：

1. Step 0：facade 內抽出 owner 共用的 `state` read/emit + `_disposed` 介面（`WorkbenchStateStore`），不動行為。
2. Selection owner：`_navigationHistory`、`selectWorkspace`/`activateProject`/`selectWorkspaceTab`/`setActiveTab*`/`focusWorkbenchGroup`/`goBack`/`goForward`/`setSearchQuery`/`setCollapsed`。invariant：`isSelectionCurrent` 雙檢查、history record 在 applyLayout 之後。
3. Tab/Layout owner：tab_opening/file_tabs/pull_request_diff_tabs/tabs + `_onTabsChanged` + layout load/persist 鏈 + `_tabClosingScope`/`_layoutLoading`/`_clearedLayouts`/`_fileTabMutations`。invariant：`layout.sanitize(tabs)` 在 apply 前、activeTabId 不指向不存在的 tab、watcher event 與 close 交錯時 snapshot 先行。
4. Catalog owner：projects/sections/workspace creation/三段 `_on*Changed`/`bootstrap`。invariant：removed workspace 的 retired cleanup 在 apply 前、`ensureMainWorkspace` 只跑一次、viewPrefs 修剪跟隨 catalog 事件。
5. Resource lifecycle owner：subscription registries + cleaners + dispose；`deleteWorkspace`/`sleepWorkspace`/`removeProject` 的清理段改走 owner port。invariant：resource 只 release 一次。

已知阻礙：owner 是 plain class 不能 extends notifier（`part of` + `_$WorkbenchController` 限制）；`workbench_controller_provider_boundary_test.dart` 只掃 `workbench_controller*.dart` 前綴，新 owner 檔名需擴大掃描範圍；`focusSourceControlRoot` 實作寫在 shared `WorkbenchViewPrefs`，歸 selection 需先經 facade 寫入（可能是刻意的 contract 變更，後續再定）；跨欄位 plan（`planWorkbench*SetSync`/`applyWorkbenchSleepWorkspaceState`）留在共享 sync 層由各 owner 呼叫。

---

## 16. Phase 4：Terminal

`TerminalRuntime` 實際 logical library 很大，下一步按 ownership 拆：

1. Session I/O / recovery owner。
2. Renderer adapter。
3. Shell launch/input delivery。
4. View resource / buffer budget。

必測：

- 兩個 view/tab 指向同一 durable session。
- detach / reattach。
- background tab output 持續更新。
- restore cancel / stale restore completion。
- output ordering。
- buffer release 不殺 durable PTY。
- app/window close responsiveness 不退化。

不要把 Flutter widget lifecycle 變成 PTY owner。

---

## 17. Phase 4：Git/editor

`WorkspaceGitDiffPanel` logical library 仍很大。已有 generation/cancellation 防護，下一步是把 loader/cache 從 presentation 移到 application owner。

先做 TDD：

1. repo A history future 尚未完成，切到 repo B；A 結果晚到不可顯示。
2. compare cache 不可跨 repo scope 污染。
3. duplicate refresh 要 coalesce。
4. nested git root scope 必須保留。
5. 整檔/差異範圍與 side-by-side/單欄仍是兩個獨立 preference。

再抽：

- repository-scoped history loader。
- compare loader/cache。
- refresh/cancel generation owner。

Presentation 保留 render/input/transient UI state。

---

## 18. Phase 4：Mobile runtime transition matrix

`HostConnectionController` 已有 retry、epoch、opening attempt、dispose。先補狀態轉移測試再拆 concrete client：

- direct -> relay。
- relay -> direct。
- foreground -> background -> foreground。
- host restart during reconnect。
- dispose during opening attempt。
- auth renewal 與 connection replacement 交錯。
- runtime restart 後 stale client completion 不回寫。

不要把 desktop window/keepAlive semantics 搬進 mobile。

---

## 19. Phase 5：Agent capability matrix

Rust 已有 agent registry；不要建第二套 registry。

建立表：

```text
agent x launch / hook / status / usage / restart / model override
```

先選差異大的兩個 agent 做 fixture，再找真正需要的 adapter seam。目標是新增 Devin/Rust/其他 agent 時，不必在 Dart/Rust/UI 多處新增散落 switch。

Agent hook reconcile 的 latest-wins scheduler 已完成，可作為 filesystem integration 更新的參考模式。

---

## 20. Phase 5：Settings ownership

Settings 要區分三類：

- local-only UI prefs。
- runtime operational settings。
- portable/cloud configuration。

`SettingsController` 可作 facade，但不要讓 application domain 反向依賴 settings presentation/app barrel。

`alera_settings.dart` / `settings_dialog.dart` 的 max-lines 拆分應順便對齊這三類，而不是單純切行數。

---

## 21. Phase 5：Release / portable / observability

### Release / portable

建立平台矩陣：

- Windows desktop portable/installable。
- macOS。
- Linux。
- standalone runtime/node。
- mobile。

將 signed manifest、hash、package-manager/landing metadata 的來源集中到 release plan；不要在 runtime correctness 重構同時大改 release workflow。

### Observability

優先新增不含敏感 payload 的：

- operation/request ID。
- deferred queue wait ms。
- execution duration。
- active/pending background jobs。
- outcome taxonomy。
- reconnect reason。
- stale completion count。
- resource owner / cleanup completion。

不要記錄 credentials、完整 prompt 或完整 terminal payload 來換除錯方便。

效能 gate 先有可重複 benchmark，再設定 threshold，不要猜 p95/p99。

---

## 22. 每批標準流程

### Dart / Flutter 批次

```text
1. 寫 red test
2. flutter test <focused entrypoint>
3. 最小實作
4. flutter test <focused + affected regression>
5. dart format --set-exit-if-changed <changed files>
6. dart tool/quality/runtime_architecture_guard_test.dart
7. dart tool/quality/runtime_architecture_guard.dart
8. dart tool/quality/check_max_lines.dart
9. git diff --check
10. review diff
11. commit
12. 不 push
```

max-lines 尚未全清前，第 8 步會是 repo-wide exit 1；必須確認 offender 數沒有增加且本批目標有下降。

### Rust 批次

```text
1. 寫 deterministic red test（barrier/channel/generation，不用 sleep 模擬 I/O）
2. cargo test -p alera-cli <focused module/filter>
3. 最小實作
4. affected request-routing/client-delivery/conformance tests
5. rustfmt changed files
6. git diff --check
7. review diff
8. commit
9. 不 push
```

涉及 actor/background job 時額外檢查：

- client disconnect。
- stale generation。
- worker panic/join error。
- admission full。
- completion delivery failure。
- owner disposed/restarted。

---

## 23. 目前不該做的事情

- 不要 push。
- 不要 merge/rebase 到 main，除非使用者明確要求。
- 不要碰 `alera-runtime-boundary` 的 dirty `v2_contract.rs`。
- 不要用 `check_max_lines --write-baseline` 掩蓋 14 個 offender。
- 不要為了 max-lines 一次重寫 `orchestration_requests.rs`。
- 不要把所有 deferred job 都改成同一個 universal queue。
- 不要把所有 settings/view prefs 集中到 server。
- 不要建立另一套 agent registry。
- 不要移除 old-host compatibility。
- 不要把 UI close 等待所有 runtime cleanup。
- 不要因 test-only split 順便整理 production。

---

## 24. 接手後最推薦的實際順序

### Batch A：先完成已分析的 Workbench test split

- `_FakeWorkbenchRepository` 抽 part。
- ViewPrefs source-control/failure cases 抽 part。
- `flutter test test/unit/workbench_controller_test.dart`
- max-lines 14 -> 12。
- commit，不 push。

### Batch B：清 `settings_dialog_core_test_cases.dart`

- 按 capability 分 part。
- 完整 settings dialog widget tests。
- max-lines 12 -> 11。
- commit。

### Batch C：清 `workspace_explorer_test.dart`

- 按 file tree / actions / stale async 分 cases。
- 完整 explorer tests。
- max-lines 11 -> 10。
- commit。

### Batch D：拆 `terminal_host_conformance.rs`

- 按 wire capability 分 integration tests/shared harness。
- 確認 coverage 沒少。
- max-lines 10 -> 9。
- commit。

### Batch E：`project_requests.rs` owner split

- registration / clone / queries。
- 保留 prepare outside actor + commit inside actor。
- focused Rust tests + request routing。
- max-lines 9 -> 8。
- commit。

### Batch F：Dispatch Context install state machine

- 先 red：blocked install 不阻塞 status。
- 加 generation/in-flight owner。
- background I/O -> completion command -> actor revalidate/commit -> worker start。
- orchestration conformance/review regressions。
- commit。

### Batch G：Deferred pending backpressure / metrics

- 先量 pending/queue wait。
- deterministic overload red test。
- coalesce 可合併 request；不可合併超限回 typed error。
- 不阻塞 actor 等 permit。
- commit。

### Batch H：Transaction/replay contract matrix

- Project / Workspace / TabLayout / Orchestration / SharedPrefs 逐 domain 做。
- 一個 domain 一批，不大包。

### Batch I：Workbench owner refactor

依序：Catalog -> Tab/Layout -> Selection/Navigation -> Resource lifecycle，保留 controller facade。

### Batch J：Terminal / Git / Mobile

先測 owner/race，再拆 loader/session/transition。

### Batch K：Agent/settings/release/observability

最後整併擴充性與發布/量測，不要提前擴大本輪 blast radius。

### Batch L：檔案與異動風暴下的 UI refresh backpressure

使用者回報：檔案過多或異動檔案過多時桌面端卡住。已確認的機制：

- Explorer watcher（`workspace_explorer_refresh.dart`）：Dart 端 `_scheduleWatchedRefresh` 把每個 batch 串進**無界** `_watchRefreshQueue`；每個 batch 都跑一次 `_refreshGitStatusSnapshot`（完整 `git status` FRB）加逐目錄 `projectExplorerTree`，風暴期間 queue 只增不減，停止後仍持續卡頓。需改成 in-flight + pending-merge（對齊 `workspace_source_control_controller.dart` 的 `_watcherReloadInFlight/Queued` 模式），合併待處理 batch 的目錄集合，且 git snapshot 每次 drain 最多跑一次。
- Source control watcher：native 與 Dart 端已有 debounce/coalescing，但持續異動時每輪仍是完整 `status()+repositoryState()+listStashes()` 全掃描。需加 min-interval/adaptive throttle。
- 大量異動檔案：`GitStatusResult` 完整清單過 FRB 後整表重建，無上限/虛擬化。需界定 payload 上限或渲染上限。
- `_syncWatchedDirectories` 在目錄異動時重送完整 watched 清單，可改增量。
- Red test 方向：fake watcher batch 風暴下，status 呼叫次數有界、queue 不增長、風暴結束後 UI 立即收斂。

歸屬：Explorer 半邊屬 Workbench（Batch I 前可先獨立做），source control 半邊屬 Git（Batch J）。共用同一套「有界合併 refresh」模式，建議作為獨立批次插在 Batch G/H 之後或併入 Batch J 前半。

#### Batch L 已完成（`d3d2d6db` + `10b809a2`）

- Explorer `_watchRefreshQueue` 無界 Future 鏈 → in-flight + pending 目錄集合合併；每輪 drain 最多一次 `explorerStatusSnapshot`。
- Source control watcher 新增 `_watcherReloadNotBefore` cooldown floor：reload 結束仍有訊號 queued 時，下一次 watcher reload 間隔 250ms→500ms→1000ms→2000ms 遞增；floor 同時限制新訊號 debounce 才不被頂替繞過；無 queued 即歸零。
- `_syncWatchedDirectories` 集合無變化時不再重送 `updateExplorerWatcher`。
- Source control 異動清單改 lazy：groups 走 `CustomScrollView` + 每 group `SliverList.builder`，tree mode 走 `SliverList.builder`；2500 筆清單只建 viewport 內的 row。
- `0766fb13`：`git_status*` wire 改單次傳輸——原本每筆變更走橋三次（`entries` + `groups[].entries` + `tree_rows[].entry`），現在只傳 flat `entries`，groups/tree rows 由 Dart `GitChangeGroup.fromEntries` 重建，decode 量約降 66%；wire schema 未變、不需 FRB regen。已知可見差異：submodule 同時有 range+worktree 變更時同 area 顯示序改為 path 排序。
- `88ca68f3`：刪掉 Dart 側已不可達的 group/tree-row pass-through mapper。
- `fccf6a37`：panel 派生資料 memoize——`_filteredStatus`/`_groupsFor`/`_visibleCollapsibleKeys` 依 `identical(status)` + query + groupMode 快取，collapse toggle 與一般 rebuild 不再重算 sort+tree。

#### Batch L 殘留（實測仍卡頓時接著做）

- `gitStatus()` 仍一次回傳完整 entry list：單次 decode 已減半以上，但幾萬筆時 Dart 端 `fromEntries` 的 sort+tree build 仍是 UI isolate 上的 O(n log n) 單次工作。方向：payload 分頁（先傳前 N + 計數）或 status 摘要化；會動 wire contract，P5 fixture 已落地可在此基礎上加。
- `WorkspaceSourceControlState` 每次 reload 全表重建；可考慮結構共享或 entry-level diffing。
- 驗證方式：以 `E:\Dropbox\work\coding-tools-mcp` 之類的大異動 repo 實測，觀察 decode/分組是否仍是卡點。

---

## 25. 交接完成條件與目前真實狀態

截至本文件建立前：

- Branch：`refactor/architecture-guard-ci`
- HEAD：`285b415e7dbd6d44b76689fd741a5b17574d62fe`
- Worktree：乾淨
- Upstream：無
- Push：無
- Architecture guard：PASS
- Max-lines：FAIL，剩 14 offender，已明列
- Phase 0：核心完成
- Phase 1：shutdown uncertainty + snapshot retry policy + smoke DB ownership 完成
- Phase 2：已完成大量 read-side/background I/O mailbox 去阻塞、8-slot active budget、sidebar single-flight、project registration prepare/commit、upload lifecycle cleanup、agent hook latest-wins reconcile
- Phase 2 尚未完成：dispatch context install、pending task total backpressure、部分 settings/autostart/managed-workspace preflight、transaction/replay matrix、wire fixtures
- Phase 3+：尚未正式開始 owner migration
- 其他 worktree：有 dirty 狀態，不能清理或吸收
- Merge/rebase/deploy/build release：本工作未做

下一位可以直接從第 24 節 Batch A 開始，不需要重新盤點一次整個專案。
