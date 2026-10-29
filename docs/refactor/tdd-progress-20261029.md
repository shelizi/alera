# Alera TDD 重構執行進度（持續更新）

> 起點：2026-10-29；Worktree `.worktrees/refactor-tdd-boundaries-20261029`；Branch `refactor/tdd-boundaries-20261029`；main 基準 `3501b0555`。
> 讀取 `docs/refactor/tdd-roadmap-20261029.md` 了解契約/驗收，然後從下列 pending/in_progress 工作續做。
> 變更只在隔離整合 worktree；最新 macOS hotfix 來源分支目前與 `main` HEAD 指向同一提交，已依使用者要求將此 hotfix 分支合入整合分支，但沒有在原 `main` worktree 做修改、push 或 release。

## 最新：T2-D2 三線功能安全補強（2026-10-30；Capability OFF）

從前一批完成的 `0b1f9f441`，平行切出 Rust Host 已移動 PTY 的終止／重新附加／重新啟動／Tab Remove workspace owner guard **`df69aece0`**、Desktop PTY/Workbench Scoped Tab Close 與同視窗 Xterm handle display owner 更新 **`817fcf7e1`**、跨視窗 watcher 辨識轉移收據並禁止舊 Layout 反寫 **`333afb30e`**。各線獨立 clean 提交後 cherry-pick 整合；具體做法與剩餘競態見 `docs/terminal-tab-cross-workspace-plan-20261029.md`。

**使用者跨專案移動仍未開啟**（`tab.transfer.capabilities.supported:false`）。不宣稱測試通過／可發布：本批無 Flutter/Cargo Tests、Analyzer、Check、Build、Win/Mac 實機；只格式化與 staged Git diff check。尚需 Host 全域 Layout 同步 CAS、跨程序最後一個 Session View/Detach、Runtime crash 後 PTY owner reconciliation、Undo／失聯 UI、operation contracts；原有 dirty `main` 未動、未 push/release。

## 最新：T2-D1 三線已整合（2026-10-29，仍未開啟）

從乾淨基準 `97f21ab02e23cb7eba39791080dedd7f52bd10ae` 平行完成三個獨立工作分支：Rust Host 關閉競態／`tab.transfer.status`／Agent Presence／保護收據 **`43591a4be`**，Dart Host 不明結果的權威確認與 unknown exception **`5a0f01bdd`**，Flutter 依最新 Mutation ID 安全反向 Undo 並恢復原 Pane Group **`efce4bea6`**。已依序 cherry-pick，無衝突。三個新工作樹初始 checkout 曾逾時，但只在本輪新建工作樹內完成修復／解除初始化鎖，提交前各自 clean，既有 dirty `main` 未操作。

**仍需**：Host capability 仍是 `supported:false`，不開啟使用者跨 Workspace Drop；多進程/多視窗 explicit Close/Detach、stale watcher 及 output cursor、原 CWD 消失或重新啟動、Pulse/Automation 拒絕 UX 與 operation contracts 還要整合確認。這輪只跑 `dart format`、`rustfmt` 與 Git whitespace check，沒有 Flutter/Cargo tests、Analyzer、Build、macOS/Windows 實機驗證，不能宣稱完成或正式發布。請以 `docs/terminal-tab-cross-workspace-plan-20261029.md` 的最新 D1 段落接續，不要把歷史 B2 誤認為目前未接線。

## 最新：T2-B2 三線已整合（2026-10-29；Host route 已寫，但產品仍停用）

從乾淨 HEAD `845886d57879b9a5b5801e55ab2d6b549ccb915f` 平行實作並各自提交：Rust Host `tab.transfer` + 原 Hook 身分／安全拒絕條件 **`f3977fa99`**；Workbench controller／兩側 watcher fence／Host capability probe **`8443c2f0e`**；Terminal 原執行路徑提醒及重新啟動路徑保留 **`683b99dc8`**。三條線已合入整合分支。**Host `tab.transfer.capabilities` 仍刻意回報不支援（supported=false），Flutter 的跨 Workspace Drop 保持關閉；請勿描述成正式完成的功能。** 待做 T2-D1：跨視窗/跨程序 Detach/Close、Watcher race、失聯重送、Agent Presence、Undo、源目錄消失與真實測試／編譯。詳 `docs/terminal-tab-cross-workspace-plan-20261029.md` 和 `docs/refactor/feature-progress-20261029.md`。本批依要求不跑測試、分析與編譯；原 `main` 未動，未 push、未 release。

## 最新：T2 三線功能基礎已整合（2026-10-29，未啟用／未驗證）

從乾淨 HEAD `061216941` 平行完成 Flutter sidebar/project Terminal-only DragTarget ＋多 Workspace 選擇器 **`8c119f0a1`**、Dart `WorkbenchTerminalTabTransferCoordinator`／Runtime Port **`97e02eedb`**、Rust Store 原子雙側 Layout+Tab CAS **`0fc27062b`**，三線各自提交並整合。**這不是已完成的跨專案拖曳**：`onTransferTerminalTab` 仍未接線、`tab.transfer` Host route 尚不存在、Rust live PTY Session owner 與 agent hook/restart/termination/pulse 跨 Workspace 的語意尚未建立。下一步唯一阻擋主線為 T2-B2 Actor 轉移；其他非衝突 UI／契約可平行。記錄見 `docs/terminal-tab-cross-workspace-plan-20261029.md` 和 `docs/refactor/feature-progress-20261029.md`。本輪依使用者指示沒有跑 Flutter/Cargo 測試、Analyzer、Build 或實機驗證；未碰 `main` 工作區、未推送／發布。

> **最初列為 T2 規劃（歷史紀錄，已由上方部分實作進度更新）**：跨 Project/Workspace 拖曳 Terminal Tab、維持同一 PTY/Session/CWD/scrollback，原子搬移 Tab 與兩側 Layout、支援反向復原。已收編 `ec150a493` 舊設計稿的文件至 `docs/terminal-tab-cross-workspace-plan-20261029.md`。當時僅規劃；最新進度請以本節上方 T2 三線部分基礎為準。

> **最新 macOS 貼上修正合入（2026-10-29）**：`fix/macos-claude-kitty-paste-v-20261029` 已透過 no-ff merge **`f49ee344944f85036c2798f33ba3b5e01e290e35`** 合入 `refactor/tdd-integrate-20261029`；xterm2 子模組 pin `6b365fc3a` 並在隔離整合 worktree 完成 `git submodule update --init`，版本 `0.91.15+308`。本輪無測試、靜態分析或 Mac 實機驗證；詳 `docs/refactor/feature-progress-20261029.md`。本次沒有操作原本有未提交資料的 `main` worktree。

## 最新：第八批改為功能優先（2026-10-29，依使用者指示暫停測試／分析）

四個互不衝突的隔離工作樹從第七批乾淨 HEAD `54d6280c1815f5311b2907e86deded483648bd01` 接續，**只做實際功能修正、格式化、程式碼差異檢視及最小 Git diff check**，不執行 Cargo/Flutter Tests、Analyzer、架構守門、效能量測或純拆檔。詳細的每條功能、原始／整合提交、適用範圍和剩餘風險記錄在 **`docs/refactor/feature-progress-20261029.md`**。

已整合四條來源碼提交：Rust localhost 未 admission 64 MiB byte quota **`48a45b5f3`**、Mobile view prefs timeout 後讀回權威狀態 **`4437ee71e`**、Terminal 保留共用 PTY 檢視 **`45941055a`**、Workbench 丟棄舊背景 layout **`a7ee48fbd`**。**本批刻意未測試／未分析／未跨平台編譯，不能視為 green 或可發布。**原始 `main` dirty 工作資料不動，未 merge main／push／release。

## 最新：第七批四線隔離 TDD（2026-10-29）

**此節為當前續做起點。以乾淨整合 HEAD `e38e8191c8bd3d2622ac9fb6171f656df1df8c8c` 建立第七批四個獨立 worktree，四線完成 Red→Green、獨立提交、驗證後依序 cherry-pick 至 `refactor/tdd-integrate-20261029`。未碰原本 dirty 的 `main`、未 push／release；focused regressions 不代表完整跨平台 CI。**

| 線／工作樹 | 已寫入實作 | 本批確實完成的紅綠證據 | 邊界／仍待 |
| --- | --- | --- | --- |
| W1 `refactor/tdd-b7-workbench-20261029` | `WorkbenchSelectionOwner` 增加 Source Control Focus owner-scoped generation；新的根目錄選取、清除、切專案或工作區都使未完成的同工作區 Git Probe 失效。 | 真實 Controller 新測同工作區慢舊 Repo 後選新 Repo **Red：Expected false, Actual true** → race + clear **Green**；完整 Controller+focus service **140 tests PASS exit0**、scoped Flutter analyze **No issues exit0**。隔離 `40f55f0c0` → 整合 **`262b18526`** | 全量跨平台 UI/E2E 未跑 |
| G1 `refactor/tdd-b7-git-20261029` | 一方法 `GitFileRevisionQueryPort` 和注入式 `WorkspaceGitFileRevisionReader`，工作區根／巢狀 repo 相對路徑先 normalize、拒絕跨 repository 與 `../`，Diff UI 實際呼叫該 service，保留單一 Rust Git backend。 | Red 缺 service/port 編譯失敗 → 小 fake **3 PASS exit0**；Git Diff Surface Widget **54 PASS／2 skipped**；scoped Flutter analyze **No issues exit0**。隔離 `0d6be7882` → 整合 **`116dce5cd`** | Git 完整 E2E／跨平台 CI 未跑 |
| Q0 `refactor/tdd-b7-q0-20261029` | 將 Branch Picker 純樹投影／row model／normalized branch 抽到獨立 `workspace_branch_tree_projection.dart` part；抽出 `filterWorkspaceLocalBranches` 保留遠端 prefix 排除、去重排序、當前分支、detached HEAD；原 `workspace_branch_status_bar_control.dart` **538→456 行**。 | Red 缺 pure filter 無法編譯 → **3 unit PASS exit0**，Shell + Branch Favorites 全量相關 Widget **109 PASS exit0**；scoped analyzer 初輪一個冗餘 foundation import info，修正後 **No issues exit0**。官方整合 `check_max_lines.dart --list-oversized` **160 oversized／127 offenders**（前 161／128），無 baseline 重設。隔離 `152347521` → 整合 **`29b455da2`** | 全域 max-lines guard 仍紅：127 個違規檔 |
| R1 `refactor/tdd-b7-rust-20261029` | Rust localhost accept loop 限制同時 **32 socket 連線**；超額 socket 立即關閉、不建立 Actor `ClientConnected` 或 Writer 佇列；每個連線 `OwnedSemaphorePermit` 跟隨 socket lifecycle、釋放後可再接納。保留已有 64 pending command＋32 MiB 已 admitted request budget。 | 新 Tokio 測試缺 `spawn_accept_loop_with_limit` **Red compile fail** → **Green 1 PASS**；正式 Cargo `--bin alera` Client **11** + Socket **1** + Wire Fixture **48** + Priority **2** + Deferred **9** = **71 tests PASS exit0**；rustfmt --check、Git staged diff check exit0。隔離 `8763ca289` → 整合 **`da33328ea`** | **socket 數上限不等於所有 ingress bytes 有界**；Mobile/Relay/Hook、Actor unbounded、最多 32 socket × 未 admitted 16 MiB 仍不受已 admission 配額限制；status/cancel/quit p95/p99 尚未實測 |

**第七批四線均已整合**：Rust `da33328ea`、W1 `262b18526`、G1 `116dce5cd`、Q0 `29b455da2`。整合 `flutter test --no-pub test/unit/workbench_controller_test.dart test/unit/workspace_branch_local_filter_test.dart test/unit/workspace_git_file_revision_reader_test.dart test/widget_test.dart` **142 PASS exit0**；兩個架構 guards **exit0**（Agent manual touchpoints 仍 5）；最終官方 Q0 **160 oversized／127 offenders**，未變更 baseline。所有獨立工作樹應於提交後保持乾淨；`main` 的 dirty 資料不動。**未做完整跨平台 CI、Release、Push、Merge main**。

**第八批可繼續平行**：R1 mobile/relay/hook backlog、Socket 全域未 admitted bytes、真實 status/cancel/quit queue wait p95/p99；Q0 Explorer/Terminal 大檔下一個 500+ 拆分；G1 Git Commit/File history query 狹義 port＋子模組保護；W1 Focus 切換 Generation 與 layout apply 的 owner 競態。保留同一 Runtime/GitBackend/transaction ownership；Flutter UI/Focus 不強行 Rust 化。

## 最新：第六批四線隔離 TDD（2026-10-29）

**新續做起點：上一批乾淨整合 HEAD `7063e72df93a6eece197912fb8e78e78bcc2cb67`；四個工作樹均從此 SHA 獨立切出、完成四線獨立提交並 cherry-pick 到 `refactor/tdd-integrate-20261029`。`main` 仍有使用者既存未提交資料，不碰它。以下分別記實際 Red / Green，不把 focused regression 說成完整 CI。**

| 工作線 | 目前改動 | 測試證據 | 未完成項 |
| --- | --- | --- | --- |
| W1 `refactor/tdd-b6-workbench-20261029` | `WorkbenchSelectionOwner.activateProject` 明確使 workspace epoch 與 tab selection generation 失效，避免 project-level 手動切換期間，先前的非同步 Go Back 向 history cursor 提交舊選取 | Controller 紅測 `project activation while back navigation hydrates does not move history cursor` **實際 fail**：Expected false Actual true → 修正後 pass；既有 selection coordinator + navigation history **16 pass**，完整 WorkbenchController **133 pass**；scoped Flutter analyze **No issues found**，全 exit0。隔離 `3ac5331e3` → 整合 **`5794f93b1`** | 完整跨平台 UI/E2E 尚未執行 |
| G1 `refactor/tdd-b6-git-20261029` | 新增只含 `fileRevisionBlobBytes/diffBlobBytes` 的 `GitDiffBlobQueryPort`，`WorkspaceGitDiffBlobReader` 封裝 renamed oldPath、commit/worktree area、immutable branch compare cache、失敗容錯；真實 Diff Surface UI 改用小介面，單一 `RustGitBackend` 不變 | 紅測缺 port/service 無法編譯 → 小 fake **4 unit pass**；Diff Surface UI **54 pass／2 skipped**；scoped Flutter analyze **No issues found**，全 exit0。隔離 `394ea8e4c` → 整合 **`6aea669b4`** | Git 完整 E2E／跨平台 CI 未執行 |
| Q0 `refactor/tdd-b6-q0-20261029` | Explorer watcher lifecycle / batch / quick-open invalidation 移至 `workspace_explorer_watcher.dart`；`workspace_explorer_refresh.dart` **536→415** 行；新增純函式 `workspaceExplorerWatcherTouchesSearch` 保留 VCS/設定排除、大小寫與空事件語意 | 紅測缺函式無法編譯 → **4 unit pass**；完整 Explorer Widget **78 pass**；scoped Flutter analyze **No issues found**，全 exit0；正式整合品質工具 **161 oversized／128 offenders**（前 162／129），未修改 baseline。隔離 `99fbda5d3` → 整合 **`5a67f87c9`** | 全域 max-lines guard 仍有 128 項違規 |
| R1 `refactor/tdd-b6-rust-20261029` | localhost 64-command 限制再加總計 **32 MiB 已 admission 請求**預算，以每 4 KiB `OwnedSemaphorePermit` 計費，Actor `handle_line` 結束才同時釋放，避免 command 數有限仍允許多筆 16 MiB 巨大請求；控制/PTY 完成訊息仍不受限制；`client.rs` **490 行** | 新 UTF8/整數邊界與 2-client byte-pressure／shutdown／socket-control 紅測缺型別／演算法 **compile red**；首輪修正測試將單筆 16 MiB 與合計 32 MiB 混淆的 assertion 後，正式 Cargo `--bin alera` **client 11 + wire fixture 48 + priority 2 + deferred admission 9 = 70 passed、exit0**；rustfmt --check / Git diff --cached --check exit0。隔離 `cbf78ff11` → 整合 **`996962ef4`** | 不涵蓋 socket 暫存未 admission 行、mobile/relay/hook、actor `unbounded_channel`；全 Socket memory、status/cancel/quit 真實 p95/p99 未量測 |

**第六批四條線均完成獨立提交與整合：** W1 `5794f93b1`、Q0 `5a67f87c9`、G1 `6aea669b4`、Rust R1 `996962ef4`。三條 Dart 線 scoped Flutter analyze **No issues found、exit0**；W1 完整 WorkbenchController **133 tests pass**，Q0 Explorer **78 widget pass**，G1 Diff Surface **54 pass／2 skipped**；Rust 4 組專項 **70 pass**。整合工作樹 `flutter test --no-pub ...` （navigation + watcher policy + blob reader + app shell）**26 tests passed、exit0**；`runtime_architecture_guard.dart`、`agent_extension_guard.dart` **exit0**，Agent 仍 5 touchpoints；官方 `check_max_lines.dart --list-oversized` **161 oversized／128 offenders**（第五批 162／129），無 baseline 修改。

**下一批可平行**：R1 mobile/relay/backlog（含 socket 未 admission 行、總 byte budget）與真實 status/cancel/quit p95/p99；Q0 Explorer/Terminal/長測試檔下一個 >500 拆分；G1 Git file history/commit query 狹義能力；W1 project-level/tab stale continuation 與 layout await 的更深競態。無需為了 Rust 比例把 Dart UI/Focus 綁到原生。**不能宣稱完整 CI、跨平台 build、main merge、push 或 release 完成。**

## 最新：第五批四線隔離 TDD（2026-10-29）

**本節為最新續做起點。從乾淨整合 HEAD `56f0658f9` 建立四個互不衝突的工作樹，四線獨立提交並逐一 cherry-pick 至 `refactor/tdd-integrate-20261029`；原本已有未提交項目的 `main` 完全不動。未實際 exit0 的測試不得宣稱 green。**

| 專線 | 實作與紅測證據 | 已知驗證結果 | 尚待完成／風險 |
| --- | --- | --- | --- |
| W1 `refactor/tdd-b5-workbench-20261029` | `WorkbenchNavigationHistoryService.navigateBack/Forward` 新增 `canCommit` async gate，`WorkbenchSelectionOwner` 用共享 epoch 檢查舊導航不能事後提交歷史游標；TDD red 因缺 canCommit parameter 無法編譯，修復後保留正常前進／後退 | 兩個 unit suites **16 passed、exit0**，scoped Flutter analyze **No issues found、exit0**；隔離 `38fa2a766` → 整合 **`35c8d1a03`** | Workbench 全量／跨平台 UI CI 未跑 |
| G1 `refactor/tdd-b5-git-20261029` | 新增只含 `status/diff/diffAll/diffAllPage` 的 `GitWorkingTreeDiffQueryPort` 與小型 Fake；抽 `WorkspaceGitDiffPageReader` 供實際 Diff UI 使用，status path 去重、nested submodule 回退 Rust combined diff、失敗分頁保留畫面；後續抽 `gitDiffPreviewBytes` 純函式（保留 UTF-16 `String.length+1` 規則） | 初輪 Red 缺 port/service → 4 unit pass；後續 Red 缺 preview helper → **5 unit tests passed、exit0**，Diff Surface widget **54 passed、2 skipped、exit0**；scoped Flutter analyze **No issues found、exit0**。隔離 `bb8f13579` + `2100e908e` → 整合 **`c35ce883f` + `721b82b9a`** | 仍保留單一 RustGitBackend，不代表 Git 完整系統 CI 綠 |
| Q0 `refactor/tdd-b5-q0-20261029` | `workspace_git_history_ref_actions.dart` 將 branch delete / remote checkout / mutation feedback 私有 helpers 移至 Dart part；主檔 517 → **334 行**；另外 G1 Diff loader 504→**494 行**、R1 Rust Client 594→**447 行**（測試移至 `client_tests.rs`），均為真正責任拆分 | Git Diff+History 兩個 Widget suites **60 passed、exit0**；scoped Flutter analyze **No issues found、exit0**；最終整合 `check_max_lines.dart --list-oversized` **162 oversized、129 offenders**（前 164／131），無 baseline 重設。隔離 `36ddb45bc` → 整合 **`0299ba00e`** | 全域 max-lines CI 仍紅（129 offenders），不得重設 baseline |
| R1 `refactor/tdd-b5-rust-20261029` | localhost 多 client 共用 64 pending command permit；新 `AdmittedClientLine` 直到 Actor `handle_line` 結束才釋放；單 client 至多暫存一筆未 admission 的 16 MiB 請求，配額滿仍可收發 control/terminal reply。新多 client backlog/控制寫入/獨立 shutdown 紅測編譯失敗（缺新參數、command variant）；後續將 cfg(test) 從 Client 主檔移至獨立測試模組 | 正式 Cargo **client 9 + priority 2 + deferred admission 9 = 20 tests passed、exit0**；測試抽檔後 client **9 passed、exit0**；rustfmt / diff check green。初輪 Rust Wire Fixture 47/48，查明固定 JSON 漏 12 個現有 Mobile 能力，修正**僅測資**後直接執行編譯好的 Rust test binary **48/48 passed、exit0**。隔離 `e29c66920` + `9dc76ddd7` + `4e01bd587` → 整合 **`e369c2232` + `00cc090c9` + `262a8d16d`** | 主 Actor 其它生產者/mobile/relay/byte budget 仍無界；尚未測真實 status/cancel/quit p95/p99，無全量 Rust / 跨平台 CI 綠結論 |

**第五批來源與品質重構均已整合**；整合 Flutter `workbench navigation/coordinator + Git diff reader + App Shell` **23 passed、exit0**；另 `test/unit/terminal_host_wire_lifecycle_fixtures_test.dart` Flutter 共用 Wire Fixture **7 passed、exit0**；`runtime_architecture_guard.dart`、`agent_extension_guard.dart` 均 **exit0**（Agent 仍 5 touchpoints）；最終整合正式 max-lines 為 **162／129**。Mobile Hello 固定測資修正後 Rust Wire Fixture **48 passed、exit0**（不改 Runtime 宣告）。完整跨平台 CI／macOS／mobile／release／push 皆未執行，`main` 保持原本 dirty，不動它。下一批可平行：R1 多 client 入站 byte-weighted budget、mobile/relay limit、status/cancel p95/p99；Q0 Explorer/Terminal 大檔；G1 file revision/blob narrow ports；W1 concurrent navigation/activation 和 layout-await owner race。UI/Focus 不盲目 Rust 化。

## 最新：第四批四線隔離 TDD（2026-10-29）

**本節為續做起點。起始整合 HEAD `569a77abe`；各線獨立 worktree，不修改原本有未提交工作之 `main`。**

| 專線 | 變更 | 紅測 → 綠測與驗證 | 待完成／風險 |
| --- | --- | --- | --- |
| W1 工作區選取競態 | `refactor/tdd-b4-workbench-20261029`：同一 workspace 重複非同步 hydrate 使用 owner-scoped epoch 避免舊結果覆蓋新結果；Tab 切換續行也驗證 generation | 新測一開始因缺 epoch compile red；補實作後選取 coordinator + hydrator **12 tests passed**；隔離 `6c31c9ac7`、整合 `c845fbe2e` | scoped analyze 7 項 `prefer_initializing_formals` info（既存公開 named API 初始化形式，非編譯錯誤）；完整 CI 未跑 |
| G1 Git mutation | `refactor/tdd-b4-git-20261029`：抽六方法 `GitIndexMutationPort` 與 `WorkspaceSourceControlIndexMutations`，rename 先 oldPath 再 newPath、submodule 不可從父 scope mutate；保留唯一 RustGitBackend | 新小型 fake compile red，實作後新 4 + controller 原 11 = **15 tests passed**；隔離 `73bf5a394`、整合 `6e4f0d48e`；5 檔分析 **No issues found** | 與 W1 的整合 Flutter App Smoke 共 **29 tests passed，exit 0**；完整 Git CI 未跑 |
| Q0 Git Diff UI | `refactor/tdd-b4-q0-20261029`：Git History 的 loading/collapse/refresh/invalidation、commit compare/copy/open 生命週期移至獨立 part extension，主面板**452 行**（原約 551） | 最終抽取後雙 Widget suites **60 passed，exit 0**；3 檔 `flutter analyze --no-pub` **No issues found，exit 0**；隔離 `9701b97aa`、整合 `cb7ad9d97`；原 baseline fixture 漂移已查證修正。正式 max-lines oversized **164**、offenders **131**（前 165／132），baseline 未放寬 | 全域 max-lines CI 仍紅（131），完整跨平台驗收未跑 |
| R1 本機 ingress | `refactor/tdd-b4-rust-20261029`：Tokio 本機 Socket 單一請求行新增 16 MiB 上限；`fill_buf` 追加 bytes 前先檢查；因 `tokio::select!` 會中止 pending read，保留跨呼叫 partial-buffer 以免截斷訊息；CRLF/UTF8/EOF parity | Rust/Tokio 首輪 red **4 failed / 1 passed**，實作後 reader 6 passed；最終 Cargo 覆蓋 parser + 原 Socket EOF/控制回覆順序 **8 passed、exit 0**，且移除原 unused import warning；rustfmt/diff check exit0。隔離 `d4dabecc0` → 整合 **`89ac5306e`** | 未改主 Actor 無界 inbox、不涵蓋 mobile/relay，真正 queued bytes、multiclient p95/p99/backpressure 仍未解 |

**第四批四條線都已獨立 commit 並 cherry-pick**，Git/Agent 架構 guards 都 exit0（Agent 保持 5 touchpoints），Flutter 整合 focused tests + App Shell smoke 共 **29 passed，exit0**，Git Diff/History Widget **60 passed，exit0**，Rust client parser/transport **8 passed，exit0**，Q0/G1 scoped Flutter analyzer **No issues found**。W1 scoped analyzer 的 7 個 info 皆為現有 public constructor named parameter 與 private field 形式引起的 `prefer_initializing_formals`，不能宣稱整體 analyzer 無警告。全域 max-lines guard **131 offenders 仍紅**、完整跨平台與 release CI 未跑，不宣稱全綠。

下批可繼續平行：Q0 剩餘大檔拆 Explorer/Terminal；G1 diff query/mutation；W1 同 workspace 載入／layout await race；R1 多 client admission／cancel/quit/status latency。不可未經 benchmark 將簡單 Dart UI formatter 強制移至 Rust。

## 最新：第三批四線平行重構（2026-10-29）

**以整合分支 `refactor/tdd-integrate-20261029` 為續做起點，舊儀表板的 134/133 與 R1 pending 欄位是歷史快照；請先讀本節。**

| 線 | 已獨立提交的範圍／整合 SHA | 紅綠與實際回歸 | 狀態／仍缺 |
| --- | --- | --- | --- |
| A0 Agent | `a6f79e569` → `62590dbec`；Quota 改由 canonical ID 配對 AgentType、Persona 查詢與預設值交回 discovery、Profile 預設 adapter 單一來源 | guard 預期因 3 條 stale baseline **Red** → ratchet 後 **Green：8→5 touchpoints**；14 Flutter tests passed | 已達 <=5 target，不取消未能移除的 legit Codex transcript/圖示特殊實作 |
| Q0 Git History | `76489a839` → `427f3151c`；時間格式化移出 Git History StatefulWidget，純函式注入時鐘 | 11 Flutter tests passed；`--list-oversized` **165**、未符合 max-lines **132**（前次 166／133） | Guard 仍紅，沒有重設 baseline；原版格式保持一致 |
| G1 Git read | `c5e7dfb31` → `9ac3ae928`；3-method `GitSourceControlQueryPort`、獨立並行 Snapshot Loader，原 status controller 保留 generation/reconciliation owner | 8 Flutter tests passed；Runtime Architecture Guard exit 0 | mutation port 與 race/backpressure 待下批，仍使用唯一 RustGitBackend |
| R1 Rust inbox | `4be5d7f5a` → `dc5067e11` 已整合；64 件 bounded lookahead + shutdown/restart 優先、其他命令 FIFO | 純 Rust 2 fail→3 pass；正式 `cargo test priority_receiver_tests` **2 passed**，首次 module path 編譯失敗已修復；原 `deferred_admission_tests` **9 passed，exit 0** | Actor inbox 仍 unbounded、未跑完整 Rust CI |

四條工作線都已 cherry-pick 到整合分支。整合 `dart tool/quality/runtime_architecture_guard.dart` 與 `dart tool/quality/agent_extension_guard.dart` 均 exit 0（5 touchpoints），整合 `git diff --check` exit 0。整合 Flutter test（Agent policy／Git History age／Git read port／App Shell smoke）**9 passed、exit 0**。整合 Flutter 14 檔初輪 analyze 只發現 `agent_profiles_pane.dart` 一處在重構後未使用的 import，移除後於涵蓋主要 composition consumer 的 5 檔 scoped analyze 複驗 **No issues found、exit 0**。原 `main` 尚有原本的 dirty history／release 工作，本輪未接觸，也未 push/release。

下一輪平行所有權：A0 目前達 5/5，持續做 per-agent declarative contract；Q0 從 132 violations 依測試拆 Explorer/Terminal/Diff；G1 依小型 Fake 擴 mutation/status/diff；R1 主 inbox **仍為 unbounded**，本次只有固定前瞻的 shutdown/restart 優先，尚須真實多 client 超載、status/cancel/quit p95/p99、pending bytes 與無界入站防護。Rust 不搬簡單 UI formatter 跨 FFI。

## 最新整合狀態 — 第二批三線平行（2026-10-29）

**接手以此節與下方新驗收紀錄為準；以下原有 134 項等數字為第一批歷史快照。**

- 整合工作樹 `.worktrees/refactor-tdd-integrate-20261029`，分支 `refactor/tdd-integrate-20261029`，包含首批 `552c6152e` 和第二批三項已驗證重構：
  - Q0 原分支 `refactor/tdd-quality-20261029`：`b56dbffd6`；整合 `94f900812`。將 Codex/Copilot/Cursor 能力規格移至各自 adapter，保留 registry 登記順序、完整 read-only/diff-only 安全 CLI 參數。
  - G1 原分支 `refactor/tdd-git-20261029`：`722750370`；整合 `3f3eff27b`。抽 `GitRepositoryProbePort`（一方法）與 `GitHostedReviewRetentionPort`（兩方法），不分裂 RustGitBackend。
  - R1 原分支 `refactor/tdd-rust-runtime-20261029`：`b3b0757e4`；整合 `669f93275`。Rust `DeferredAdmission` 依 request type 新增 **常數記憶體** queue-wait histogram，向 `status.get.deferredAdmission.requestTypes[]` 加入 `queueWaitSamples`／`queueWaitP95Ms`／`queueWaitP99Ms`，不改排程/PTY/DB/FRB，保留既有 sum/max。
- Q0 當下超過 500 行檔案 **166**，**133** 項違規（歷史 167/134 → 166/133）；`ai_assist_registry.dart` 約 616 → 466 行；未重設 baseline。`Agent` guard 仍有 8 manual touchpoints（target <=5）。
- **驗證**：Q0 AI Assist 63 tests passed、characterization 15 passed（有重疊）；5 個檔案 targeted Flutter analyze **No issues found**。G1 hosted-review/probe/workbench lifecycle **12 tests passed**；architecture guard passed；擴大 Dart analyze 曾有 4 項 info 已補註記，第二次 analyzer 因高負載 timeout，**不能宣稱第二輪 analyze 通過**。R1 Rust histogram pure test 2 個先紅後綠；Cargo integration metrics 測試先紅後綠（`queue_wait` 篩選 3 passed，另 1 ignored benchmark）；原生 admission/control 9 tests passed；額外 `unstarted_queued_work` Rust test 1 passed（單獨篩選，exit 0）。
- 三批各自 source/test commit 已保存，不合併 dirty `main`，未 push／release。完整 Flutter/Rust/mobile CI 未跑；`check_max_lines.dart` **仍紅（133）**。下次先在整合工作樹 `git status`／`git log -4 --oneline` 並跑 scoped checks，再開下批 worktree。

### 下批互不衝突的工作所有權

| 議題 | 狀態 | 下一個確定性紅測／執行 |
| --- | --- | --- |
| Q0 | 部分完成（133 violations） | 針對 Explorer/Terminal/Diff 的具體責任邊界寫測試再減少；禁止重設 baseline |
| A0 | guard 綠，但尚有 8 manual files | 以 per-agent adapter 鑑別特殊政策，把 touchpoint 逐一消除，驗收 <=5 |
| G1 | 四個 narrow ports 已驗證 | 針對 Git status/diff/mutation 的 consumer/fake 縮減做 TDD |
| R1 | 排隊指標實作、排程不變 | 真正未受控 inbox 的多 client/backlog/quit latency TDD；量 status/quit/cancel p95/p99；避免替換全 bounded inbox 導致死鎖 |
| W1/T1/D1/F1/C1 | 未實作 | 依 roadmap 的所有權與 race/相容測試分 worktree 同步進行 |

## 已完成的 scoped commits

- `57fdaf2349b80d604226fdc1d0aa3b101f44ca37` — `refactor: isolate terminal metadata and agent launch capabilities`（G0/A0；包含必要的 Riverpod 產碼與測試）。
- `378d62d7e3360b79bf6dd090f6ac898d73f23761` — `refactor(git): separate history and commit-compare query ports`（G1 history/compare；RustGitBackend concrete 保持不變）。
- 本文與 roadmap、Q0 inventory、R1 Rust audit 作為第三個 docs scoped commit；查詢最新 SHA 請用 `git log -3 --oneline`。
- `build_runner` 的三個 unrelated generated hash／註解修改已回復至此分支原狀，避免夾帶無關程式碼。必要的 G0/A0 產碼則隨第一個 commit 保留。

## 儀表板

| ID | 狀態 | 最新證據／阻塞 | 下一步 |
| --- | --- | --- | --- |
| G0: Resource Manager DI | verified / committed | architecture guard：2 項 → 0；Riverpod generation exit 0；Resource title / runtime binding 等在 79 passed；app smoke + workbench boundary 10 passed | 依本文件追蹤後續更多高壓回歸 |
| A0: Agent extension | verified / committed | agent guard 5 additions+1 stale → passed（仍有 8 處，target <=5）；guard fixture 4/4 passed；AI Assist/Grok/FX/AGY 相關包含在 79 passed | 後續 8 → 5 的 Agent touchpoints |
| Q0: max-lines | inventoried / gate_red | 167 大於500行（33 baseline, 19 grew, 115 new），其中 134 offenders；新增分類文件 | 依所有權 TDD 分批降低，不一鍵放寬 baseline |
| G1: Git ports | history/compare verified / committed | 新增 History/CommitCompare ports，狹義 Fake + 原既有測試 9/9 passed；保持 RustGitBackend concrete | 擴更多 Git query/mutation ports 及跨 scope/race tests |
| R1: Rust runtime admission | pending | main inbox unbounded，但 deferred admission 存在 | 先量 coverage，再寫 Rust race/admission test |
| W1/T1/D1/F1/C1 | pending | 見 roadmap | 待前置護欄穩定 |

## Red → Green 稽核紀錄

| 日期／批次 | Red（先失敗） | Green（實作後） | 檔案／commit |
| --- | --- | --- | --- |
| 2026-10-29 baseline | `dart tool/quality/runtime_architecture_guard.dart`: 2 違規；`dart tool/quality/check_max_lines.dart`: 134 offenders；`dart tool/quality/agent_extension_guard.dart`: 5 additions + 1 stale；`dart tool/quality/agent_extension_guard_test.dart`: synthetic positive fixture 失敗 | `dart tool/quality/runtime_architecture_guard_test.dart`：5 passed；其他尚未實作 | baseline `3501b0555` |
| 2026-10-29 G0 | 架構 guard：presentation import 與 full TerminalRuntime provider（2 個 fail） | architecture guard exit 0 | terminal session title port、app composition、Resource Manager、main overrides、title tests |
| 2026-10-29 A0 | synthetic fixture 正例因缺 adapter/shared policies 失敗；目前 5 additions、1 stale | agent extension guard tests 4/4 PASS；agent extension guard exit 0（8 manual touchpoints） | Agent adapter detection、AI Assist capability policies、guard fixture/baseline |
| 2026-10-29 G0 + A0 focused | G0 guard 2 violations、Agent synthetic fixture fail | `flutter test --no-pub`（resource title、runtime binding、resource providers、AI Assist settings/service、local Agent detection）：79 passed；generator exit 0；`git diff --check` exit 0 | G0 + A0 來源與 .g.dart；部分 .g.dart 為 one-shot generator 的 hash/註解更新 |
| 2026-10-29 G1 | 新單測需 `GitHistoryQueryPort` / `GitCommitCompareQueryPort` 才能編譯；此前大 Backend Fake 是唯一測試入口。嘗試的單檔 `dart analyze` 在編譯暖機時逾時，未取得可靠 red 執行輸出 | `flutter test --no-pub test/unit/workspace_git_query_ports_test.dart test/unit/workspace_git_history_loader_test.dart test/unit/workspace_git_commit_compare_cache_test.dart`：9 passed | GitQueryPorts + backend interface + history loader/commit cache + narrow fake tests |
| 2026-10-29 Final verification | 第一輪 `flutter analyze` 出現既存/新 lints；在嘗試公開不可得 `Override` 型別後 smoke compile red | 已改用 opaque Riverpod return type 推導；`flutter analyze --no-pub`（11 個影響範圍）**No issues found** exit 0；Widget app smoke + workbench terminal boundaries：10 passed；AI Assist/Git ports/widget 重跑：65 passed；guards 測試 runtime 5/5、agent 4/4 passed | G0/A0 `57fdaf234`；G1 `378d62d7` |

## 並行策略／檔案所有權

- G0：`features/resource_manager/application`、`app/terminal_runtime_composition.dart`、resource title test；需 Riverpod 一次性 codegen。
- A0：`tool/quality/agent_extension_guard*.dart`、`agent_extension_touchpoints.txt` 與 agent-specific app 檔。不得同時修改 G0 檔。
- G1：`shared/infra/git`、`features/workbench/application/workspace_git_{history_loader,commit_compare_cache}.dart` 與專用單測。
- R1：`rust/alera-cli/src/terminal_host`；Cargo target 與其他 Rust build 互斥。
- Shared generated output、main Git index、整包 Flutter build runner 在不同 worktree 保持單寫者。

## 需持續追蹤

- baseline max-lines 134 項：先把分類清單/原則紀錄，不應直接修改整份 baseline 掩蓋產品檔案。
- 分類明細與處理政策：`docs/refactor/q0-max-lines-inventory-20261029.md`；167 = Desktop 45、Rust 50、Tests 57、Localization 7、Other 8。舊 baseline 33，134 違規仍存在，不宣稱整體 PR CI 通過。
- 已以 robocopy 從原 main 複製缺少的 `third_party/xterm`、`third_party/dart_terminal` 到新 worktree 供離線 Flutter 測試；移除複製所帶入的失效 .git pointer。主分支原先的 dirty source 沒有改動。
- Rust runtime 先驗證既有 admission：DeferredAdmission 上限 8 active / 64 total / 8 critical reserve；runtime mutation queue 上限 256，且已有 queue wait / rejected / disconnect metrics。下一階段 R1 應先測未經 admission 的主 inbox 路由，不要重做上述現成功能。
- Rust R1 實際排程 code / route policy / admission tests 盤點與第一組 red/green 驗收契約：`docs/refactor/r1-runtime-admission-audit-20261029.md`（Rust source 尚未修改）。
- Window/Flutter tests 可能受 cold-start native build、missing submodules、Flutter generated dependencies 影響；失敗要記錄分類，不得宣稱通過。
- 已有其他進行中 release task 的 main scope；所有本次修改留在新的 refactor worktree。

## 後續接手起點（精確）

1. `git -C .worktrees/refactor-tdd-boundaries-20261029 status -sb`、`git -C .worktrees/refactor-tdd-boundaries-20261029 log -3 --oneline`；禁止直接對原 main 的 dirty docs/history 進行清理。
2. 優先 `Q0`：既存 134 個 max-lines offenders、`A0` Agent touchpoint 8 → 5；不得以一次性 baseline 重設偽裝成 CI 通過。
3. 可與 Q0 平行的 Rust `R1`：見 `docs/refactor/r1-runtime-admission-audit-20261029.md`，第一個 red test 是 slow worker 同時讓 status/quit/cancel 前進，保留現有 admission semantics。
4. `G1` 下批候選是 Git diff/status mutation 能力 ports 與相對應小型 fake。Rust heavy operations 已位於 native Git backend，未經 benchmark 不搬 UI 事件跨 FFI。
5. `T1/D1/F1` 有各自的 benchmark/docs；維持小批 TDD，只有明確的 CPU/IO 熱點才優先遷移 Rust。

## 本輪不宣稱

- **全套 PR CI 綠燈**：`check_max_lines.dart` 仍有 134 violations。
- **Rust actor 改造已完成**：本輪完成 R1 調查與後續紅綠規格，尚未寫入 Rust source。
- **跨平台 Release 驗證**：未跑完整 Rust/mobile/Windows/macOS build，也未推送。

## 最後接手確認

- [ ] 工作樹 diff/status
- [ ] G0/A0 focused red-green
- [ ] 品質檢查重跑與結果
- [ ] 重要測試及生成檔比對
- [ ] commit SHA 與下一批目標
