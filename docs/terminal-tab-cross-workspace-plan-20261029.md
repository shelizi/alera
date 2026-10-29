# T2 — 跨專案／跨 Workspace Terminal Tab 移動：功能規劃與並行重構邊界

**Priority：P2；Status：planned / not implemented（2026-10-29）。** 本次依使用者要求列為後續功能並落檔，**沒有修改產品功能或執行測試、靜態分析**。

本文件原是 `feat/cross-project-terminal-tabs-20261029` 分支上的設計稿（commit `ec150a493`），現**只收編此規劃檔**至 `refactor/tdd-integrate-20261029`；未合併該舊分支的產品程式。原設計所記載的並行分工為 2026-10-29 第三批的歷史狀態；最新整合已包含第八批功能修正及 macOS Claude Code 貼上 hotfix。

歷史 Feature worktree：`.worktrees/feat-cross-project-terminal-tabs-20261029`；歷史基準 `e1ef633ca87e408c0ab5e651c1e827bc161a51f6`。**真正實作時必須從當時最新、乾淨的整合分支另切工作樹，不能沿用此舊基準直接修改。**

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

目前整合分支是 `refactor/tdd-integrate-20261029`；W1/T1/C1 都已有部分實作，但**尚無跨 Workspace Terminal 的持久化 transfer transaction 或 PTY ownership transfer**。最新 T1 `45941055a` 僅保障**同一 Desktop runtime registry** 中多個檢視共用 PTY 不會因關閉其中一個檢視而被殺死，不等於跨專案轉移已完成。

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

### 使用者可見的完成定義

- [ ] 將執行中 Terminal Tab 拖至另一專案／Workspace，Session ID、PTY PID、輸出、scrollback、Shell history 不變；不自動 `cd`。
- [ ] 原 tab 與兩側 Layout 在重啟、Watcher 同步和多視窗環境下仍一致；可以從目的地復原回來源 Workspace 而不重啟程序。
- [ ] 運作中的 Claude／Codex 不因轉移被中止；需要明確告知**執行環境與 cwd 未隨顯示專案改變**，避免誤把命令送到錯誤 Git repository。
- [ ] 拖錯目標、重複 Drop、目的工作區已刪除、同時 Close／Detach、失敗／timeout 等情境不會孤立或重複建立 Session。
- [ ] 同 workspace 重排、Split Pane、Windows detach 保留原功能；Host 不支援時安全拒絕。

> 以上是未來實作完成條件，**不是本次已完成／已測試的成果**。本輪僅盤點及保存規劃；正式實作時再安排契約／UI／跨端驗證，且以當時使用者對測試的優先順序為準。

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
