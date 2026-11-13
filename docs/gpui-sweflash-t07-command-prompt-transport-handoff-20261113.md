# GPUI T07 SWE-2 Flash：Command／Custom Prompt 安全更新通道

日期：2026-11-13（Asia/Taipei）
工作分支：feat/gpui-sweflash-t07-command-prompt-20261113
初始基線：b805a033eca3a6254a4b836226e9047ac06d8957
範圍：**傳輸與驗證底層完成；GUI 尚未接線，T07 仍部分完成。**

## 本輪成果

- CLI 的 Agent Profile `update` 新增明確 `--edit-content-stdin`：Command／Custom Prompt 透過有界 JSON stdin 傳入，而不是命令列 argv。要求 `--expected-revision`；拒絕與原有 CLI Command／Prompt 參數、`--managed-config-stdin` 混用。
- stdin JSON 僅接受 `command`、`customPrompt`、`clearCustomPrompt`，強制合法型態、明確 Prompt 清除、禁止 replace+clear 混用；整體 32 KiB、Command 4 KiB、Prompt 8 KiB 上限，拒絕空命令和 NUL。其他欄位維持 CLI 原有 Host-owned draft 流程，預期 revision 透過 Host CAS 驗證。
- GPUI 的 `RuntimeCliBridge` 新增 `UpdateCommandPrompt` 請求與 `update_agent_profile_command_prompt`，僅將 Host ID／revision／固定 flag 放進 argv，將內容透過既有 child stdin 管道傳遞；只保留 Secret-free Profile row，檢查回傳 ID、Adapter、Managed mode 與 revision 遞增。
- 不新增第二份 Host Profile 設定、不接觸真實 Provider 認證，既有 Profile Routing／Clone／刪除保護不變。
- 主控審查時修正 SWE Flash 新測試的三個 `profile(id, name, revision)` 呼叫錯誤，及錯將含空白命令認作非法的測試敘述。

## 實際驗證

- `cargo test --offline --locked -p alera-cli --bin alera edit_content -- --nocapture`：**7 PASS／0 FAIL**。首次編譯因新測試 helper 參數不足失敗，修正後重跑完成。
- 在 `desktop_gpui`：`cargo test --offline --locked --lib command_prompt -- --nocapture`：**3 PASS／0 FAIL**。
- Rust CLI workspace 與 GPUI 的 `cargo fmt --all -- --check`、`git diff --check`：**PASS**。
- 使用專案 Windows build-storage script，由主控依標準在 D:\c\t 編譯；不執行正式 Host、PTY 或 Provider 登入。
- 尚未進行可拋棄 Host 上的 CLI stdin 寫入／重新讀回，以及真正 GPUI Windows GUI 多行 Custom Prompt 的使用者驗收；**不得以單測綠燈宣稱 T07 DONE**。

## 後續正常功能

1. 補 GPUI 真正多行 Custom Prompt 編輯元件（Classic 為 3–8 行），不可偷用會去除換行、512 字節上限的 QuickOpenInput；Command 單行輸入可用既有安全元件。
2. 從 Host 安全載入內容到編輯中 transient 狀態，依選取 Profile／Host Revision 失效；Save 透過新增 stdin CLI 更新，Host 狀態獨立讀回並回報錯誤與無變更。
3. 用隔離 Host Command Profile 做 Command／Custom Prompt 正向存取及清空、重啟持久化；Windows GPUI 驗收後再將已過 Gate 的提交整合到 integration 分支。
4. T07 Managed 模型／Persona 與 Provider 未登入／過期提示仍排在後續；現在 NOW 仍 T07，不合 main／Push／Release，supported=false。
