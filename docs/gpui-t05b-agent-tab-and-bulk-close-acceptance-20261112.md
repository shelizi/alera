# Alera GPUI T05B：真實 Agent Tab 拖曳焦點與乾淨分頁批次關閉驗收

日期：2026-11-12（Asia/Taipei）。分支：`integrate/gpui-central-diff-into-main-20261112`；本輪起始 HEAD：`cf9d8d928`。

**結論：T05B Windows 正常流程 PASS，連同 T05A 可將 T05 標記完成，下一項 NOW T06。** 此結論僅涵蓋實測的 GPUI 日常情境，並非跨平台、全部 Agent 或完整 Release Gate；`supported=false` 維持。

## 隔離測試

- Windows GPUI 使用本分支 `desktop_gpui/target/debug/alera-gpui-desktop.exe` 與對應 `alera-cli.exe`，獨立 Runtime：`desktop_gpui/target/smoke-terminal-t05b-runtime-20261112`，獨立狀態：`desktop_gpui/target/smoke-terminal-t05b-state-20261112`。
- Workspace 僅使用已存在的 protected `.gpui-center-diff-smoke` 巢狀 Git fixture；沒有修改正式 Classic Host、沒有登入／登出或重置 Provider、沒有刻意觸發 CLI 更新。
- GUI 透過 `+ → Claude Code` 啟動原已登入 Claude Code v2.1.322，並由 `+ → New Terminal` 建立普通 `cmd.exe`。Host `terminal --no-start list/read` 核對 Session 身分與實際 TUI，沒有把單靠 chip 名稱當作 Agent 身分證據。無 Hook 的隔離 Runtime 可能顯示通用 `Terminal` chip／`agentType=null`，但其 Host TUI 確認是 Claude Code，Hook 元資料另於 H 系列處理。

## 已發現並修復的普通操作缺口

首次拖曳 Claude 到另一 Pane 時，`WorkbenchPaneLayout` 雖已移動 Host tab，但 `TerminalDock.active_tab` 仍指向先前 Terminal，因此目的窗格 tab 與實際 TUI／鍵盤焦點可能不同步，需額外點擊才恢復正確內容。

- `desktop_gpui/src/workbench_panes.rs` 增加 `post_cross_pane_drop_focus`：只有真正跨窗格、目的 Pane 已選中原始且有效的同一個 Tab 身分時，才回傳應聚焦分頁；同窗格純排序及關閉的身分不能搶焦點。
- `desktop_gpui/src/main.rs` 的 `reorder_mixed_tab` 接收 GPUI `Window`，對有效跨 Pane 拖曳使用既有 `focus_mixed_tab` 正確同步 Host `TerminalDock.active_tab`、TUI／輸入焦點與版面儲存；沒有更動 Runtime Host PTY 所有權。
- TDD 新增 `cross_pane_host_drop_requests_focus_only_for_moved_live_identity`，先因缺少方法編譯失敗（RED），實作後 GREEN PASS（1/1）。
- 使用修改後新建置的 Windows GPUI 重新 Attach *原本* Claude 與 cmd Session，再把 Claude 拖到已持有 cmd 的副窗格，畫面**立即顯示正確 Claude TUI**；`workbench-layout` 快照中 `secondary_active_host` 是原 Claude 身分。Host 清單仍只有原本兩個 running Session。也實際驗證 Claude 可移回主窗格，Host 不會因移動或選取而重建。

## 乾淨 Native Editor／Terminal 批次關閉

| GUI 情境 | 實機觀察 | 其它資料／Host |
| --- | --- | --- |
| Explorer 雙擊開啟既有 `center-diff-smoke.txt` | Native Editor 顯示 `Saved`，未修改檔案 | 不另建重複 Editor |
| Claude 分頁右鍵 `Close others` | 同 Pane 乾淨 Editor 關閉、Claude 仍在，另一 Pane Terminal 未關 | Editor 不寫檔，無髒稿提示 |
| 再開同一乾淨 Editor，Claude 右鍵 `Close to right` | 正確關閉右側 Editor，Claude 和另一 Pane 保留 | 原測試 Git fixture 未改 |
| 副 Pane `+ → New Terminal` 新增第三個 cmd | Host 真正新增獨立第三個 running PTY | 用 Host 清單確認 |
| 副 Pane 原 Terminal 右鍵 `Close others` | 第三個 cmd 的預覽關閉，主 Pane Claude 不受影響 | Host 清單仍三個 `running=true` |
| 副 Pane `+ → Attach running Host session` 重新連回第三個 PTY，原 Terminal 右鍵 `Close to right` | 只關閉右側第三個 Terminal 的預覽 | Host 清單仍三個 `running=true`，無新的第四個 PTY |

此輪使用的 Host 原始身分為：
- Claude Code：handle `361b24b1-dcb9-4d2d-b2c3-eb0b567a8521`、tab ID 同 handle。
- 第一個 `cmd.exe`：handle `67218196-b492-4970-9279-beb96abf0174`、tab ID `a9dbc0cf-71b3-41fc-9192-da2e4d76d6e0`。
- 第三個 `cmd.exe`：handle `1c3e4231-8abf-46be-9cee-f339c6e60627`、tab ID `83dfe984-55dc-49ac-8703-8b3f90c8c6af`。
- 三者 Workspace ID 都是此次隔離 Runtime 的 `25c87971-1dd4-41d8-a5ca-aa0b2ed4ece6`。

## 最終測試與收尾

- `cargo test --offline --locked --lib`：**516/516 PASS**，0 failed。已含本輪新增的 TDD。
- `cargo test --offline --locked --bin alera-gpui-desktop`：**31/31 PASS**，0 failed。
- `cargo build --offline --locked --bin alera-gpui-desktop`：Windows binary PASS。第一次建置時測試 GUI 未關導致 EXE 鎖定（Windows os error 5），確認 PID 僅屬本輪隔離 GUI，正常關閉後重新建置成功；不是 Rust 編譯錯誤。
- `cargo fmt --all -- --check` 和 `git diff --check`：PASS。
- GUI 正常關閉；明確只對 `smoke-terminal-t05b-runtime-20261112` 執行 `runtime stop --force`，回傳 `stopped=true`、`activeSessions=3`；最終 `runtime status` 回 `running=false`、`host=null`。
- 前後 protected nested Git fixture `git status --short` 均為原本的 `M center-diff-smoke.txt` 及 `?? center-diff-source-switch-untracked.txt`；未重設、清除或提交。整合分支原有的未追蹤 `.gpui-center-diff-smoke/`、`desktop_gpui/target/` 亦保持不變。
- T05 正常功能路徑完成，**NOW T06：Terminal 實際英文／繁中鍵盤、IME、剪貼簿、快捷鍵、resize／Shell**。Hook metadata 與各 Provider 錯誤提示另外列 H01–H03／T07，不會誤當 T05 通過。
- 未合併 `main`、沒有 Push、部署或發版，`supported=false` 不變。
