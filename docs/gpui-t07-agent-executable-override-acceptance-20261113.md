# GPUI T07｜Agent 執行檔覆寫與錯誤提示（Windows）

日期：2026-11-13（Asia/Taipei）；整合分支 `integrate/gpui-central-diff-into-main-20261112`，本輪起始 HEAD `606049563`。此為 **T07 子項**，尚非整項 T07 DONE／GPUI Release。

## 正常流程缺口與 TDD

1. 原 `NativeSettingsStore::compare_and_set_agent_executable_path` 僅檢查絕對路徑及 `Path::is_file()`，會將一個真實存在的 `agent-not-executable.txt` 寫成 `codex` 執行檔路徑。新增 `native_agent_override_rejects_an_existing_non_executable_file`：**RED**（`unwrap_err()` 反而拿到成功儲存的 settings）→ **GREEN**（`InvalidInput`，磁碟覆寫不變）。
2. Windows Agent CLI 路徑只接受存在的 `.exe`、`.com`、`.cmd`、`.bat`（副檔名大小寫不拘）。Unix 仍使用 executable mode bits。設定的 CAS 儲存與 New Agent 的本機掃描共用同一個 `executable_is_runnable` 判斷，舊設定殘留 `.txt/.ps1` 不會再被顯示成可啟動 Agent。既有 override 錯誤時不悄悄退回 PATH，維持 fail-closed。
3. 原本按 `Save executable path` 但輸入空白，會被視為 `None` 而刪除已儲存的覆寫，與獨立的 `Clear override` 按鈕語意衝突。新增 `agent_override_blank_save_does_not_act_as_clear`：**RED**（E0425，表單 guard 不存在）→ **GREEN**。只有按 Clear 才能清除；空白 Save 顯示「Enter an executable path, or use Clear override」。
4. Windows 無效路徑的 GUI 錯誤明確說明需存在的絕對 `.exe/.cmd/.bat/.com`，CAS 被其他視窗更新時顯示重新開設定的建議。Clear 成功則顯示「Agent executable override cleared; using Auto PATH」。

## Windows Native GUI 正向／負向驗收

- 先以舊版 EXE 在 `desktop_gpui/target/smoke-t07-exec-override-state-20261113-0731` 驗證基準：可誤存 `.txt`，**不將舊版 GUI 當作修正成功證據**。關閉舊視窗，重新 `cargo build --offline --locked --bin alera-gpui-desktop`，改用全新隔離狀態 `...state-20261113-0732`。
- 改版 GUI：不存在的 `C:\nonexistent\agent.exe` 被拒絕。真實存在的隔離 `not-an-agent.txt` 亦被拒絕；Codex 顯示 `Auto PATH`，磁碟沒有儲存覆寫。
- 真實存在的隔離 `probe-valid.cmd`（無害測試內容，**僅存路徑，未執行**）成功儲存；GUI 列表顯示該路徑，磁碟讀回副檔名 `.cmd` 且 `IS_FILE=True`。
- 在已儲存有效 `.cmd` 的情況再送 `.txt`：GUI 顯示錯誤，磁碟保留原本 `.cmd`；Clear override 後 GUI 回到 `Auto PATH`，磁碟覆寫為空。
- 第二輪 Windows EXE 更新了表單 guard／錯誤訊息，再用相同的獨立 state 重開：空白 Save 顯示專屬提示；儲存有效 `.cmd` 後清空輸入再按 Save，**舊 `.cmd` 仍保留**（磁碟 `BLANK_SAVE_PRESERVED_CMD=True`）；只有 Clear 顯示 Auto PATH 並清空磁碟。無效 `.txt` 另確認新副檔名錯誤提示。
- 這些流程不連接任何正式 Host、不啟動 Provider、不存取登入憑證，也沒有修改 Classic 設定。三次 GUI PID `6968`、`15464`、`26356` 均已關閉；受保護 fixture 的 `M center-diff-smoke.txt` 與 `?? center-diff-source-switch-untracked.txt` 維持原樣。產生在 `desktop_gpui/target/` 的隔離資料未清理。

## 回歸與後續

- `cargo test --offline --locked --lib -- --skip git_history::tests::explicit_full_history_search_finds_old_subject_and_message_body_beyond_page_limit`：**528 PASS／0 FAIL／1 FILTERED**（既有長測未執行）。
- `cargo test --offline --locked --bin alera-gpui-desktop`：**32／32 PASS**；Windows EXE Build、`cargo fmt --all -- --check`、`git diff --check`：**PASS**。
- **同日後續**：Host Profile 標準 Command Create、Rename 與 ↑↓ 排序已在 GPUI 完成並經隔離 Windows GUI／Host 實測；詳見 [管理驗收](gpui-t07-host-profile-management-acceptance-20261113.md)。T07 仍需真實 Provider 未登入／過期憑證提示、Advanced／Managed Profile 編輯、Clone 與安全 Delete。CLI Override 的通過**不等於 Provider 已登入／Agent 啟動成功**。
- `supported=false`；未合併 `main`、Push、部署或發版。
