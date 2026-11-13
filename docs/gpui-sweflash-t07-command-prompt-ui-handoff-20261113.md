# GPUI T07 — Host Command／Custom Prompt 編輯 UI 續作驗收

日期：2026-11-13（Asia/Taipei）
分支：`feat/gpui-sweflash-t07-command-prompt-20261113`
前置通道 Commit：`5f0d602e6ffcb785142d5d5cd6ea4a86159574a8`
狀態：**GUI 原始碼接線完成、CLI + 模組測試通過、Windows GUI 實際點擊驗收待完成**。原計畫 NOW 仍 T07，`supported=false`。

## 本輪實作

- `agent_profiles.rs`：明確取得 Host Show 中 ID、revision、mode、Command、Custom Prompt 的有界暫時投影。捨棄 Managed config/credentials；Prompt 保留真正換行並將 Windows CRLF 正規化。拒絕超過 4 KiB Command／8 KiB Prompt、無效型態、非法 Host ID 或 revision。
- `agent_runtime_bridge.rs`：`load_agent_profile_content` 驗證 Host identity/revision/launch mode；原有 `update_agent_profile_command_prompt` 透過有界 stdin JSON 與 Host CAS 寫入，成功後額外執行獨立 Host Show 讀回比對內容（成功才回傳 display row）。只傳固定 flag/Host ID/revision 到 argv。
- `quick_open_ui.rs`：新增獨立 `profile_command`（4 KiB）與 `profile_multiline`（8 KiB、保留 newline）兩種輸入模式。舊 Explorer/Search 和 Terminal 的單行 CRLF、輸入規則不變；多行 Enter 未組字時才插入換行，避免 Windows 注音候選期間誤插入。
- `main.rs`：Command／Prompt 編輯輸入與原始快照僅存在瞬時 GPUI Workbench state；Host catalog 或選取 Profile 的 ID/revision/mode 變動時清除；不放進共享設定或重啟還原檔。
- `native_settings_ui.rs`：新增 Load Command／Prompt、Save Command／Prompt、Clear Prompt；Command-mode 顯示 Command，Managed-mode 不提供 Command 修改，兩者都能編輯 Prompt。執行前檢查未完成 IME 組字、使用者選取 ID/revision、無變更 no-op，清空 Prompt 必須按獨立 Clear 按鈕。異步結果以 Workspace generation／request ticket 擋掉過期回覆。

## 真正取得的驗收證據

- CLI `cargo test --offline --locked -p alera-cli --bin alera edit_content`：**7/7 PASS**。
- GPUI Bridge `cargo test --offline --locked --lib command_prompt`：**3/3 PASS**。
- 隔離 Host 真實 stdin 修改、版本衝突拒寫、清空、再次讀回整合測試 `agent_profile_stdin_command_prompt_roundtrip_on_isolated_host`：**1/1 PASS**；RuntimeGuard 在測試結束停止測試用 Host。
- GPUI 新增 transient Host Content/CRLF 投影測試：**1/1 PASS**。
- GPUI 原生 Binary 新增多行 Prompt/單行模式一致性測試：首次因測試錯估舊 CRLF 為單空格 FAILED，校正為原行為後重跑 **1/1 PASS**；並於本輪最後重跑通過。
- `cargo check --offline --locked --bin alera-gpui-desktop`、Windows Debug EXE `cargo build --offline --locked --bin alera-gpui-desktop`：皆有成功紀錄；最終重新編譯也以 exit code 0 完成。
- Rustfmt 與 `git diff --check`：PASS。
- E01 平行工作線另在隔離分支 `25926c6b9` 的兩個 Rust 測試 **2/2 PASS**，**沒有提前整合**。

- GPUI Library 回歸（排除一項既有 Git History 長測）及 Native Binary 回歸整批執行：**exit code 0**，未把非本輪直接操作的 UI 流程誤列為人工 GUI 驗收。

## 尚未通過的 Gate

1. 主控嘗試啟動獨立 GUI 測試 Host：先使用外部建置目錄、再改專屬 worktree 內的 smoke 目錄，兩次都被工作區工具政策預先拒絕（`WORKSPACE_PATH_PROTECTED`）；確認專用 smoke 目錄未建立，沒有啟動或碰觸真實 Host，不繞過保護。未使用桌面滑鼠/鍵盤實際驗收 Load→Command／多行 Custom Prompt 修改→Save／Clear Prompt→獨立 Host Show；目前不宣告 GUI PASS。
2. 需以不影響既有桌面/正式 Host 的隔離 Windows GUI 驗證：中文 IME、Enter 換行與明確清空、Command/Managed 選擇與 revision 失效；此輪只做 CLI 真 Host 寫入驗收。
3. 真實未登入／過期 Provider 提示、Managed Model／Persona 等 T07 子項仍待補；不能因 Command/Prompt 單一子項過關就宣告 T07 DONE。
4. 子代理曾負責安全 stdin 傳輸實作；本次 UI SWE Flash 嘗試逾時且無原始碼變更，由主控直接接線/測試。沒有子代理在背景繼續執行。

## 整合限制

只允許按原追蹤計畫 T07→E01，審核過且正常 GUI 驗收成功之 Commit 才整合回 GPUI integration。**沒有合併 main、Push、部署、Release，supported=false**。使用者原 worktree 的 protected fixture 和其他未追蹤檔案保持不動。
