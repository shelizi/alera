# Alera GPUI H01 — Agent Hook 四態：晚到 Host 身分、雙窗格、Tab Tooltip

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；H01 起始 HEAD：`7df5d39c6`。
狀態：**核心四態與 Host 身分投影已整合；Windows 實際 Codex／Claude GUI 操作及現場 Hook 驗收待辦。`supported=false`。**

## 既有能力盤點

- `terminal_stream_bridge::parse_agent_presence_stream_event` 從已認證 Host Terminal stream 讀取嚴格的 redacted `agentPresenceChanged`（Session／Workspace／Tab／Agent／State／Timestamp），沒有轉發 Prompt／Tool Input／Token；`main.rs` 以 `TerminalDock::apply_agent_presence_changes` 更新現存 Host Session，`WorkbenchTabs::refresh_agent_statuses` 只改 Tab metadata、不切換焦點。
- 既有 `AgentActivityState` 及主副窗格共用 `render_mixed_tab_chip` 已顯示 `working／waiting／blocked／done` 的專用顏色，但只有短字串，沒有對應工作區與 Session 的完整 Tooltip。
- **真日常缺口**：Host 清單可以先回報正在運行的 PTY，但 Agent Hook 尚未識別 `agentType`。後續已認證、完整身分的 Hook 變更即使 `workspaceId／tabId／sessionId` 全相同，也因 `session.agent_type == None` 被舊 `apply_agent_presence_changes` 全域拒絕；必須等完整 Host 清單再同步，四態無法即時出現在正確 Tab。

## 本輪修正

1. `terminal.rs`：對已存在、`running=true` 且 Host `session_id + workspace_id + tab_id` 三個身分一致的 Session，接受有合法 Agent Type 與四態的**已認證**晚到 Host delta，在 Session 原 `agent_type` 為 None 時一次性建立識別並立即套入狀態與時間戳。原本已確認 `agent_type` 的 Session 不允許其他 Agent Type 蓋掉；失配／過期／無效事件維持拒絕；不建立新 PTY、不切換 Editor／Tab／Workspace。
2. `workbench_tabs.rs`：新增 `WorkbenchTab::agent_status_tooltip()`，只為 **正在運行的 Agent Host Tab** 產生 `Agent working／waiting／blocked／done（中斷則 interrupted）` 與 Tab Title、Host Session／Workspace／Tab ID；一般 Terminal、Editor、停止的 Agent PTY、無 Host Hook 四態都不產生 Tooltip。
3. `main.rs::render_mixed_tab_chip`：將 gpui-kit Tooltip 直接掛在現有焦點 Tab 元件的 `div().tooltip(...)` 上；主、副 Pane 均走同一渲染路徑，無新增功能入口、未攔截分割窗格既有焦點／拖曳、Close 或 Host Session 所有權。

## TDD 與驗收範圍

- **RED 1**：`h01_late_authenticated_hook_identifies_exact_running_session_without_waiting_for_inventory` 最初因 `apply_agent_presence_changes` 回傳 false 失敗；修正後 **GREEN 1/1**。測試同時驗 `Waiting→Blocked→Done→Working`、錯 Tab／錯 Adapter 拒絕、其他 Agent 不動、原 Tab 焦點與已開 Session 數量不動。
- **RED 2**：`h01_agent_tooltip_reports_four_host_states_and_exact_tab_without_claiming_stopped_pty` 因缺 `agent_status_tooltip` E0599；新增後 **GREEN 1/1**。測試四態及中斷、Session／Tab ID 辨識、普通 Terminal、Editor、停止 PTY 不顯示虛假狀態。
- **GREEN 3**：`h01_four_host_states_remain_in_original_split_pane_without_focus_or_pin_changes` **1/1**；主、副窗格分別有 Agent，副窗格四態逐一變化，主窗格持續 Working，Pin、Focus 與各 Pane Tab Identity 不變，Tooltip 不會指向另一個 Agent。
- **Windows 原生 UI**：GPUI Native Binary test target 已能編譯含 `div().tooltip` 的新 Tab 畫面；仍需真正滑鼠 Hover 驗收（不能把無視窗的 Rust test 當成 GUI Tooltip 已顯示）。
- **全量 Windows 回歸**：Rust Library **575 PASS／0 FAIL／1 ignored**；Native Binary **44/44 PASS**；Windows EXE Build、`cargo fmt --check`、`git diff --check` 全部 **PASS**。原有隔離 Host 預設 ignored 測試未執行，不誤算為 PASS。

## 不包含與後續

- 本輪是正常 Hook Presence 資料流／正確窗格身分／Tooltip 的功能整合；**未實際用 Codex／Claude 進入等待人員確認、Blocked、完成的全流程 GUI 操作，也沒有在正式 Host 變更 Hook 設定**。若現場 Provider 尚未登入或 Hook 根本未發送，UI 不得臆測 `working`。
- H02 繼續驗證 Hook 開關與通知偏好在隔離 Host 的真正 CAS／重啟讀回；H03 處理 Windows 原生通知與點擊定位。H01 真 GUI Hover 與 Agent 流程仍留待後續實機確認，不阻礙核心功能推進。
- 不清除原 WIP／target／smoke fixture；不合 `main`、不 Push／部署／Release。`supported=false`。
