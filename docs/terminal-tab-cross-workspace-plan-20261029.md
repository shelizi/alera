# T2 — 跨專案／跨 Workspace Terminal Tab 移動：功能規劃與並行重構邊界

**Priority：P2；Status：D1 Host/Recovery/Undo implementation integrated, feature-gated OFF（2026-10-29）。** 已分三批合入九條來源碼工作線，最近新增 Runtime 關閉競態護欄、失聯回覆權威確認及可逆的 Undo。**尚未開放跨專案拖放，也沒有執行本批的 Flutter/Cargo 測試、靜態分析或編譯。**

本文件原是 `feat/cross-project-terminal-tabs-20261029` 分支上的設計稿（commit `ec150a493`），現**只收編此規劃檔**至 `refactor/tdd-integrate-20261029`；未合併該舊分支的產品程式。原設計所記載的並行分工為 2026-10-29 第三批的歷史狀態；最新整合已包含第八批功能修正及 macOS Claude Code 貼上 hotfix。

## 最新：T2-D1 再平行三線合入（2026-10-29，仍停用）

從乾淨整合 HEAD `97f21ab02e23cb7eba39791080dedd7f52bd10ae` 分出三個獨立 worktree，各自完成 scoped 修改、`rustfmt`／`dart format` 與 `git diff --cached --check`，獨立 commit 後依 Host→Recovery→Undo 順序 cherry-pick 回整合分支（無合併衝突）：

| 線路 | 本批完成的實際功能／保護 | 隔離 → 整合提交 |
| --- | --- | --- |
| T2-D1 Host / race | `tab.transfer` 遇到背景 Runtime Mutation、來源或目的 Workspace 尚有未完成 shutdown 時拒絕移轉；來源 Workspace 切換後，Host 即時發送 Agent Presence refresh；杜絕舊視窗的 Terminal `tab.upsert` 將已移轉 Tab 靜默搬回原 Workspace。Host-owned `lastTerminalTabTransfer`、原 Execution Workspace、launch directory 在後續 Tab 更新時不可被覆蓋。轉移收據記錄先前 `sourceGroupId`。新增只讀 `tab.transfer.status`，以**精確 Mutation ID＋資料庫收據＋仍在執行且歸屬目的 Workspace 的 PTY**共同確認不明結果。 | `c1f8aa185` → **`43591a4be`** |
| T2-D1 Recovery / lost reply | Dart 轉移 RPC 收到 Timeout／Connection Closed，先用同一 `clientMutationId` 呼叫 Host `tab.transfer.status`，只有收據、Tab、Session、目的地全部一致才能視為已提交。若 Host 無法確認，回報 `TerminalTransferOutcomeUnknownException`，**不重送新 Mutation、不將未知視為失敗成功**。 | `c340bc6fb` → **`5a0f01bdd`** |
| T2-D1 Undo / UX | Controller 正向轉移成功後傳回原 Host mutation receipt id；在側欄顯示 10 秒的 Undo Snackbar。點擊 Undo 時再查目前權威 Tab 收據，只有**最後一次**轉移的 ID 相符、來源仍存在且未封存才用另一筆 `tab.transfer` 反向移回；原 pane group 尚在則恢復到原 group，否則用目前的 active group。**同一個原 PTY 不重啟、不重建**。 | `8ddc98a97` → **`efce4bea6`** |

**這批仍未解決／不可啟用的界線**：Host `tab.transfer.capabilities` 依然回報 `supported:false`；需明確驗證 Terminal explicit close／Detach 在多視窗與多程序同時發生時不會終止已移轉 PTY；清理後若來源 Workspace／原啟動路徑不存在，Shell Restart／Hook 仍需安全降級；確認 Watcher fence、同步、Undo 回讀與不同 process runtime owner 的行為。Host 暫時拒絕有 Pulse、Automation／Orchestration、Pending Agent Prompt 的 Session 移轉，這些拒絕條件及提示仍須產品級整理。正式開關前也要更新 `docs/tab-layout-operation-contract.md` 和 `docs/terminal-operation-contract.md` 並執行目標測試／編譯；目前依使用者先前指示**沒有測試、Analyze、Cargo check、Build 或實機驗證**，不宣稱 green、未推送／發布。

**本批 Git worktree 初始化事件**：三個新 worktree 在 Windows＋同步資料夾大量檔案 checkout 時超過初始命令時限，留下部分檢出與零長度工作樹專用 `index.lock`；已核對無運行中 Git 程序，只針對**這三個新 worktree** 修復缺少的 tracked files、用 `git reset --mixed HEAD` 修復不完整的 index、解除 `locked initializing`，三線提交前均確認 clean。**原本 dirty 的 `main` 和既存工作樹未清除或重設。**

## 最新：T2-B2 再平行三線合入（2026-10-29，仍停用）

本批從整合 `845886d57879b9a5b5801e55ab2d6b549ccb915f` 切出三個**互不修改同檔**的功能 worktree，分別完成來源碼修改、格式化、Git staged whitespace check 與獨立提交；再按 Host→Workbench→Terminal UX 順序 cherry-pick 回 `refactor/tdd-integrate-20261029`。

| 工作線 | 新增內容 | 隔離提交 → 整合提交 | 嚴格限制 |
| --- | --- | --- | --- |
| T2-B2 Rust Host | 新增本地 `tab.transfer` Actor route：要求活 PTY／Session ID、同一來源 Tab，對接已實作的 Rust Store CAS；一個交易提交後同步修改活 Session 的顯示 `workspace_id`，**Session ID／PTY process／output buffer／working_directory 不變**，通知來源和目的 Tab watchers 與 Layout watchers；原 Claude/Codex Hook 可用持久化來源身分驗證，再重新歸到目前顯示 Workspace。新增 `terminalExecutionWorkingDirectory` 保存原啟動資料夾 | `a9985034a` → **`f3977fa99`** | 針對已開啟 Pulse、Automation-owned Tab、Pending Agent Prompt、活 Orchestration Dispatch、同 Tab 多個活 PTY 明確拒絕。直接 RPC 存在但**未經編譯／測試**；`tab.transfer.capabilities` 刻意返回 `supported:false` |
| T2-C1 Flutter Workbench | 新增來源/目的 Workspace watcher fence，避免單側舊 snapshot 把 Rust Store CAS Layout 回寫覆蓋；Host ACK 後讀回兩側 Tabs/Layout **只套用不持久化**，再切換目的地／focus；Runtime transfer capability probe、Provider/Controller/Sidebar callback 接線 | `7d99dc6a5` → **`8443c2f0e`** | Host `supported:false` 時 Sidebar Callback 仍是 `null`，不會接受跨專案拖放；timeout/lost reply、遠端重連與旁路 watcher 等尚未完整驗證 |
| T2-D0 Terminal UX／重新啟動 | Terminal Tab 若顯示 Workspace 與原始 Execution Workspace 不同，顯示來源及原始啟動目錄的 Tooltip；後續重新建立 PTY（顯式 Restart／Host recovery）時，使用保留的原始啟動路徑及原 Agent Hook Workspace ID，避免突然變成目的專案的命令環境 | `359353a10` → **`683b99dc8`** | 保存的是**原啟動路徑**，不是偵測使用者後續 `cd` 的即時 CWD；原路徑失效／跨平台 Shell 實際行為尚未驗證 |

**啟用前仍需解決**：跨視窗／跨程序 Detach+Close 同時執行的終止競態、Host 不明結果／斷線後的冪等重播與讀回、Watcher 更新序與跨 client 事件、Agent Presence 轉移即時性、Pulse／Orchestration 限制下的拒絕 UX、來源 Workspace 已刪除時的重開處理與 Undo、更新兩份 operation contract。依先前使用者指示**本批只格式化／Git diff check，沒有 Flutter/Cargo Tests、Analyzer、Build、實機測試，也未 push／release；禁止宣稱正式支援**。三線程式碼在隔離整合分支，原始 dirty `main` 未動。

歷史 Feature worktree：`.worktrees/feat-cross-project-terminal-tabs-20261029`；歷史基準 `e1ef633ca87e408c0ab5e651c1e827bc161a51f6`。**真正實作時必須從當時最新、乾淨的整合分支另切工作樹，不能沿用此舊基準直接修改。**

## 最新：T2 三條平行功能基礎（2026-10-29）

從乾淨整合 HEAD `061216941` 切出三個隔離 worktree 並各自提交，之後成功 cherry-pick 回 `refactor/tdd-integrate-20261029`：

| 工作線 | 已新增的來源碼／能力 | 隔離提交 → 整合提交 | 尚未完成 |
| --- | --- | --- | --- |
| T2-A Flutter UI | `WorkbenchTabDragData` 增加 tab kind／來源 Workspace／Host，Workspace 側欄和 Project 標題有 Terminal-only Drop Target、同 Host 與未封存限制；Project 多個 Workspace 時顯示選擇對話框 | `42c28ea1e` → **`8c119f0a1`** | 接收 callback 刻意留空（null），沒有 Host 支援時不會接受跨 Workspace Drop；仍未接線實際轉移與 Undo |
| T2-B1 Rust Store | `RuntimeStore::transfer_terminal_tab_storage` 的 SQLite 原子 compare-and-swap：核對來源／目的 Workspaces、Host、終端種類、兩側 Layout 快照與所有 Tab membership，一個交易更新 Tab 歸屬和兩側 Layout；保存來源 execution workspace 和 mutation receipt，方便回應遺失後辨識重送 | `800124981` → **`0fc27062b`** | 目前**只有 Store 方法**，沒有 `tab.transfer` Actor route、Session owner 轉移或 watcher 廣播；此方法在正式 Host flow 中尚未使用 |
| T2-C0 Dart 協調準備 | `WorkbenchTerminalTabTransferCoordinator`、窄 `WorkbenchTerminalTabTransferPort`、`RuntimeTerminalTabTransferPort`：先取得雙側布局與 Tab 清單、建立預期值／新布局、限制同時移動同一 Tab、送出固定 mutation ID，確認返回的 Tab/Session ID | `12cd69a41` → **`97e02eedb`** | 尚未在 Workbench/Riverpod 組合、未呼叫；Client adapter 預備的 `tab.transfer` RPC **還不存在**，禁止先接 UI |

**硬性啟用前提（T2-B2）**：Rust ServerActor 活 Session 的 `workspace_id` 目前同時影響 source workspace 移除時的 Session 終止、terminal restart 的 tab/workspace 驗證、Agent hook `workspace_id` 核對、terminal pulse／自動化路由。必須在同一 Runtime owner 下設計並完成「顯示 Workspace 歸屬」與「實際執行 Workspace／工作目錄」的區分，協調原子 Store commit、活 PTY reconciliation、雙端事件廣播、同時 Detach/Close/Exit、unknown-outcome replay，才可接上 UI callback。**不准直接改寫 Tab DB 的 `workspaceId` 假裝功能已完成**。

本批只執行格式化、差異檢視與 `git diff --cached --check`；**Flutter／Rust 測試、Analyzer、Build 和 macOS/Windows 實機尚未執行，沒有任何 green 或已發版聲明**。既存的同 Workspace 重排、Split、Windows detach 預期保持原狀，但本批尚未回歸驗證。

## Goal and visible behavior

- Drag a Terminal tab onto another project or workspace in the sidebar (select the destination's existing workspace; do not silently create one).
- 拖到「專案」時若存在多個適合的 Workspace，顯示目的地選擇器；無可用 Workspace 時顯示不可放置原因，不在背景建立新的 Workspace。
- Preserve the same tab id, terminal session id, live PTY/process, scrollback, output delivery, and user-entered title. A move is **not** close + create, restart, or a new shell.
- The running process retains its original CWD and inherited environment. The target tab must make this explicit; the app must **not** claim that the process has changed projects.
- Move display placement and persistence to the destination; source layout must remove the tab and destination layout must place it once. Support normal split-tab layout rules and a recoverable error path.
- 轉移完成後能透過**反向轉移／復原**把同一個 Tab 送回原 Workspace，保持原 PTY／Session 不變；Undo 需要再確認原 Workspace 是否仍存在，不可用 close+create 假冒復原。
- 同專案內重新排序、分割面板及 Windows 拖出獨立視窗的既有功能不退化。
- Don't treat detaching a tab into a native window as the same operation. A tab that is being detached/reparented must not be simultaneously transferred.

## Verified current behavior

- `workspace_workbench_tab_strip_drop.dart` only accepts `details.data.workspaceId == workspaceId` for strip and append targets; `workspace_workbench_pane.dart` rejects different workspace ids.
- `_WorkspaceTabDragData` includes the source `workspaceId`, `sourceGroupId`, and `tabId`; `WorkbenchTabLayoutOwnerTabs.moveWorkspaceTab` only mutates the layout of one workspace.
- `WorkspaceTabService` persists the terminal session id in the tab payload. `RuntimeWorkbenchRepository` uses `tab.upsert` and `layout.upsert` separately; they are not an atomic cross-workspace transfer.
- `RuntimeStore.upsert_workspace_tab` can overwrite `workspaceId`, but **must not** be used alone as a transfer API. A live Rust `Session.workspace_id` is separately used to terminate sessions when a workspace is removed; `terminal.restart` rejects workspace/tab metadata mismatches. Updating the DB row alone risks killing the moved PTY or failing a later restart.
- `docs/tab-layout-operation-contract.md` contains no `tab.transfer` contract; `docs/terminal-operation-contract.md` pins the PTY, restart, removal and checkpoint behavior.

## Other refactor: file ownership and collision risks

目前整合分支是 `refactor/tdd-integrate-20261029`；W1/T1/C1 都已有部分實作。**現在已有尚未接線的 Rust Store 原子 transfer 基礎，但仍沒有完整的活 PTY ownership transfer 或可呼叫的 Host RPC。** 最新 T1 `45941055a` 僅保障**同一 Desktop runtime registry** 中多個檢視共用 PTY 不會因關閉其中一個檢視而被殺死，不等於跨專案轉移已完成。

| Concurrent branch / workstream | Observed edits on 2026-10-29 | Overlap decision |
| --- | --- | --- |
| `refactor/tdd-b3-agent-20261029` (A0) | agent profile/adapters, quota icons, settings and tests | No current same-file overlap. Existing running agent processes still require identity-safety tests. |
| `refactor/tdd-b3-git-20261029` (G1) | source-control controller / Git backend query ports | No current same-file overlap. |
| `refactor/tdd-b3-q0-20261029` (Q0) | Git history UI relative-time extraction | No current same-file overlap; Q0's later Terminal file-size split can overlap. |
| `refactor/tdd-b3-rust-20261029` (R1) | `rust/alera-cli/src/terminal_host/server_runner.rs` and child module | Do not simultaneously edit its queue/run-loop or share Cargo build targets. Host route mutations need coordination before merging. |
| W1（已有部分修正） | Workbench workspace switch、layout hydrate、tab lifecycle；第八批 `a7ee48fbd` 拒絕 stale background layout | 高語意重疊：transfer 完成時來源與目的的 watcher／layout 不能覆蓋新狀態。 |
| T1（已有部分修正） | Terminal detach、session ownership、renderer、restore；第八批 `45941055a` 保留同一 registry 的共享 PTY | 高語意重疊：移動僅換 presentation owner，不可殺死 PTY 或清除活躍 agent output。 |
| C1（已有部分修正） | Store transactions、request replay、lost reply；第八批 `4437ee71e` 僅處理 Mobile 偏好設定 timeout 讀回 | 高語意重疊：需獨立的 `tab.transfer` 一次提交／冪等交易，現有範圍不足。 |

**Work ownership:** This feature may own the drag acceptance and a dedicated application transfer coordinator **in its own worktree**. Do not concurrently edit the refactor's `server_runner.rs`, terminal-host queue machinery, shared generated bindings or Git index. Before touching terminal host / store / workspace lifecycle source, recheck the live W1/T1/C1 assignments, branch diffs, and ownership. Keep this feature unmerged until the transfer transaction and PTY semantics are verified against the latest refactor integration branch.

## Required host contract (to implement once ownership is coordinated)

Use an explicit, authenticated transfer operation (proposed `tab.transfer`); do not synthesize it from `tab.remove` and `tab.upsert`:

1. Validate terminal kind, tab id, expected source workspace, destination existence/state and any active detach/closing operation. Reject unsupported tab kinds and stale source ownership before mutation.
2. Persist the new display workspace and both layouts in one transaction or use a proven equivalent reconciliation protocol. Include replay/unknown-outcome handling (stable client mutation id or a conditional source check) so a lost reply does not duplicate or delete tabs.
3. Reconcile the live PTY session's workspace ownership **without** restarting it. Align terminal teardown, pulse, session restart, host checkpoint and automation/agent ownership rules. Distinguish original execution context from display location; existing process CWD/`ALERA_WORKSPACE_ID` cannot be changed retroactively.
4. Broadcast changes to **both** workspaces after successful commit. The UI must wait for acknowledgement and refresh both layouts/subscriptions; on failure keep the source tab visible and operational.
5. Update `docs/tab-layout-operation-contract.md` and `docs/terminal-operation-contract.md` and their existing contract tests before announcing functional support.

## 後續實作分期與平行界線（待執行）

| Phase | Owner／可平行性 | 交付物 | 前置條件／阻擋 |
| --- | --- | --- | --- |
| T2-A：UI 選擇與預覽 | Flutter Shell／Sidebar／Workbench UX；可**與 T2-B 的 API 設計平行** | Terminal-only drop eligibility、目標 project/workspace 選擇、hover/拒絕提示、單一 Workspace 自動選取與多 Workspace 選擇器；維持舊版 same-workspace 拖放 | **主機不支援前不得把跨 Workspace Drop 當作已成功**；UI 只做 preview／feature gating |
| T2-B：Runtime/Store 原子轉移 | Rust Runtime/Store **單一擁有者**；可與 T2-A 平行讀取／設計，但 Host transaction 及 PTY owner 同時修改必須序列化 | `tab.transfer`（預期來源 Workspace、目的 Workspace、Tab、Mutation ID）原子更新 Tab+兩側 Layout+活 PTY owner，明確返回 revision/結果；廣播雙側變更 | 先界定 runtime session、remove/sleep、restart、agent/hook 的所有權；避免單純 `tab.upsert` |
| T2-C：Coordinator + UI 接線 | Workbench application owner；**依賴 T2-B 完整契約** | 轉移中鎖定拖曳、等待 Host ack 再切換 UI；成功清理來源／定位目的地並更新雙側 watcher；失敗維持原 Session/Tab；反向 transfer 支援 Undo | 和 W1 Tab/Layout hydration、T1 Close/Detach 不得併行編輯同檔 |
| T2-D：復原與相容性 | Desktop／Mobile／Runtime 跨層契約；T2-C 後 | 重開 App、Host 重啟、斷線重試、來源 Workspace 移除後仍可尋得同一 PTY；終端輸出不丟序；原 cwd/env 與執行位置清楚標示 | 若 Host 版本不支援 `tab.transfer`，UI 必須停用跨專案拖移，而非退化成關閉後重建 |

**目前分期狀態（以最新 B2 節為準）**：T2-A UI 框架已落地但停用；T2-B1 Store CAS 已落地；T2-B2 Host route／live session 基礎已落地但 capability 仍 OFF；T2-C0 Dart Coordinator/Port 已落地；T2-C1 Workbench watcher fence／controller 接線已落地但仍停用；T2-D0 原執行位置提醒／重新啟動路徑已落地。**T2-D1：驗證和解決跨 client/Detach/Close/丟失回覆／Undo 剩餘風險**，才可對外啟用及宣稱支援。

### 使用者可見的完成定義

- [ ] 將執行中 Terminal Tab 拖至另一專案／Workspace，Session ID、PTY PID、輸出、scrollback、Shell history 不變；不自動 `cd`。
- [ ] 原 tab 與兩側 Layout 在重啟、Watcher 同步和多視窗環境下仍一致；可以從目的地復原回來源 Workspace 而不重啟程序。
- [ ] 運作中的 Claude／Codex 不因轉移被中止；需要明確告知**執行環境與 cwd 未隨顯示專案改變**，避免誤把命令送到錯誤 Git repository。
- [ ] 拖錯目標、重複 Drop、目的工作區已刪除、同時 Close／Detach、失敗／timeout 等情境不會孤立或重複建立 Session。
- [ ] 同 workspace 重排、Split Pane、Windows detach 保留原功能；Host 不支援時安全拒絕。

> 以上是**整體 T2** 未來實作完成條件，並非這次已支援或測試的功能。首次規劃時只有設計文件；目前已另外合入三條**未啟用**的 UI／Dart／Store 基礎。正式啟用仍待 Rust Actor 活 Session owner 的安全轉移與契約／跨端驗證。

## TDD acceptance matrix (not yet executed)

| Case | Red / green condition |
| --- | --- |
| Invalid drop | Same-workspace reorder remains unchanged; non-terminal tab and missing destination are refused. |
| Live session migration | PTY session id and process remain identical across move; output continues during/after drag, with no lost sequence or duplicate renderer. |
| Workspace retirement | Closing/removing the source workspace after a successful move does **not** terminate the moved session; closing the destination follows destination ownership. |
| Restart and recovery | Host restart, terminal reattach/restart and saved layouts resolve the destination tab exactly once; stream/history preserved. |
| Cross-project identity | CWD/environment stay as launched, visible origin indicator tells the truth, agent/hook writes cannot silently retarget the wrong repository. |
| Race and failure | Concurrent close, detach, session exit, workspace switch, double-drop, failed store write and timeout/retry have explicit, tested outcomes. |
| UI target | Dropping on destination sidebar workspace/project places it predictably; dragging across panes inside one workspace still behaves as before. |

## Safe execution plan

1. **Completed:** inspect active worktrees, collect exact current dirty file lists, verify transfer blockers, create an isolated feature worktree based on the integration baseline, record this plan.
2. Pending: define pure Dart drag eligibility/target-resolution tests without changing shared Runtime/Store files. Focus UI source ownership remains separate.
3. Pending / coordination required: agree on host/Store transfer transaction and state ownership with W1/T1/C1; first write failing contract/race tests, then implement the narrow operation. Existing `tab.upsert` alone is insufficient.
4. Pending: wire feature UI, run targeted Flutter + Rust tests, architecture guards, `git diff --check`; merge into the *updated* refactor integration branch only after conflict review. Do not automatically merge `main`, push or publish.
