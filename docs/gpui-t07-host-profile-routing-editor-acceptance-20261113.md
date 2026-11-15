# T07｜Windows GPUI Host Agent Profile Routing 編輯驗收

日期：2026-11-13（Asia/Taipei）
分支：`integrate/gpui-central-diff-into-main-20261112`；本輪起始 HEAD `c9eded39b`。
驗收子項：Classic Profile 的 **Description／Quota Group** 讀取、修改、清空、儲存；T07 整項仍 **NOW／部分完成**，`supported=false`。

## Classic 合約與最小改動

- Classic `agent_profile_editor.dart` 有 Name／Adapter／Command、Custom Prompt、Description、Quota Group、Managed Options／模型 Persona。這輪優先完成一般使用會改的 **Routing 兩欄**，不把其餘編輯誤標通過。
- Native Settings → Agent Profiles 選取 Host Profile 時，以專用背景 CLI `agent-profile --no-start --json show --profile-id=...` 取得 **僅** ID／Revision／Description／Quota Group 的投影。Command、Custom Prompt、Managed JSON 只留在該次 CLI 處理過程，不寫入 GPUI state JSON 或 Desktop Shared Settings。
- 導入 `Routing · selected Host Profile` 與 `Load routing`／`Save routing`，兩個 Native Unicode/IME 輸入欄；組字未完成不送出儲存。
- 透過原有 `agent-profile update --profile-id --expected-revision`，僅發送 `--description`／`--quota-group` 或 `--clear-description`／`--clear-quota-group`。不重建 Profile，不碰既有 Command、Prompt、Managed config、授權或 Provider 登入。
- 儲存結果驗證 Host Profile ID、Revision 遞增、Adapter／Managed Mode 不變；重新讀取 Host Catalog 時，若觀察到的 revision 或選取對象改變，舊 Routing 編輯快照失效，避免錯寫另一個 Profile。

## TDD／CLI 回歸

- RED：`routing_fields_are_host_owned_secret_free_projection` 初次 E0425 缺少 `parse_profile_routing_fields`／`validate_profile_routing_fields`。GREEN：只保存非機密投影、修剪值與拒絕換行控制字元。
- RED：`routing_update_requires_observed_revision_and_explicit_clear_flags` 初次 E0599 缺少 `AllowedAgentRequest::Show`／`UpdateRouting`。GREEN：`--no-start`、revision、變更及明確清空旗標均由單元測試確認。
- 新 CLI Host 端到端 `agent_profile_routing_updates_only_requested_fields_and_guards_revision`：在隔離 Host 更新 Command Profile 的兩欄、舊 revision 拒絕、明確清空、Managed Profile 只改 Routing 且保留 `managedConfig`／`customPrompt`。第一次新增測試將 Description 清空誤認成 `null` 而 FAILED；依原 Host String 型別修正測試為 **空字串 `""`**（Quota Group 才是 `null`），完整 CLI conformance 重跑 **3／3 PASS、0 FAIL**。沒有修改 Host 清空語意。

## 隔離 Windows GUI 實測與獨立 Host 讀回

使用 `desktop_gpui/target/smoke-t07-routing-runtime-20261113-1037` 的專用 Host、`desktop_gpui/target/smoke-t07-routing-gpui-state-20261113-1051` 的獨立 GPUI 狀態、以及獨立測試工作目錄。GUI PID `46692`；全程沒有建立真正 Agent PTY／Provider 登入，沒有修改正式 Host。

| 操作 | Windows GUI／Host 證據 |
| --- | --- |
| 選取 Command Profile | 自動載入 `Routing Before`、`qa-old`；正確顯示兩欄 |
| Command Profile 編輯儲存 | GUI 填 `Routing Saved via GPUI`／`qa-gpui`，Host 獨立 `show` 由 revision 0→1，兩欄完全一致 |
| Command Profile 清空儲存 | GUI Ctrl+A／Backspace 清空兩欄後 Save，Host revision 1→2；Description=`""`、Quota Group=`null`；`cmd.exe /Q` 和 Custom Prompt 不變 |
| Managed Profile 編輯儲存 | CLI 在隔離 Host 以**明確** `--confirm-reduced-protections` 建立既有降低防護測試 Profile；GPUI 選取後讀回舊 Description／Quota Group，儲存新值使 Host revision 0→1 |
| Managed 設定保留 | Host 獨立 `show` 確認 Managed `approvalPolicy=never`、`model=test-model`、Custom Prompt、Managed launch mode 全部保留；Native 沒有重新要求降低防護，也沒有改動既有核准設定 |

**測試範圍與收尾**：隔離 Host 的 PowerShell `Start-Process -Wait` 初次因 Windows 後代程序等待而逾時，但 Host 已正常啟動，後續透過獨立 CLI 驗證操作成功；停止時第一次即時 status 仍短暫顯示 `running=true`，等待停止寬限期後重新確認 **`running=false`**。隔離 GUI PID `46692` 已關閉。既有受保護 Git fixture 仍為 `M center-diff-smoke.txt`／`?? center-diff-source-switch-untracked.txt`，使用者其他 Worktree／`target/` 未清理。

## 最終回歸

- GPUI Rust Library `538 PASS／0 FAIL／1 FILTERED`（既有 Git History 長測未執行）、Native Binary `33/33 PASS`；不帶測試 Fixture 功能的 Windows EXE 編譯 PASS，Rust fmt PASS。
- CLI Host conformance `3/3 PASS／0 FAIL`，包含既有 Catalog CRUD、完整 Clone 與本輪 Routing Update/Clear/Managed 保留欄位。
- 另外確認受保護 fixture Git 狀態未改；隔離 Host 的最終 status `running=false`，GUI PID 已關閉，沒有 main Merge、Push、部署或 Release。

## 下一步與發版限制

- 尚未實作 Classic 全部 Agent Profile 編輯：Command 行、Custom Prompt、Adapter／Launch Mode、Managed 模型／Persona／各 Provider 專屬控制項，以及真實未登入／過期 Provider UX。需後續 TDD／Host／Windows GUI。
- Host 操作必須繼續拒絕未授權降低防護、保留 revision CAS，不把測試用降低防護設定帶入正式帳號。
- **T07 保持 NOW、部分完成；`supported=false`。** 不合併 `main`、不 Push、不部署、不發版。
