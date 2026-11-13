# Alera GPUI T07｜Host Agent Profile 刪除／影響預覽 Windows 驗收

日期：2026-11-13（Asia/Taipei）。原分支 `integrate/gpui-central-diff-into-main-20261112`；起始 HEAD `011ffd8f2`。**本輪只完成 T07 Host Profile 安全刪除子項，不代表整體 T07 或 Classic 進階編輯完成。**

## Classic 規格與 Host 實際契約

- Classic `agent_profiles_pane_profile_actions.dart` 在刪除前呼叫 Host `agentProfile.removalImpact(id, expectedRevision)`，對 Automation、Tab、Execution Policy Run 的引用禁止確認；若只有 Default／Automation Policy，可說明 Host 的原子清除。確認後使用 Host Revision CAS 刪除。
- Host CLI 提供 `agent-profile --no-start --json removal-impact --profile-id ID --expected-revision N` 以及 `remove --profile-id ID --expected-revision N --confirm`；隔離 Host 實測前者包含 `profileId / exists / revision / isDefault / hasAutomationPolicy / automationIds / executionPolicyRunIds / tabs / referenceCount / blockingReferenceCount`，後者成功回覆 `{"removed":true}`。
- 刪除不應透過直接操作 SQLite、猜測 CLI 成功、儲存私人命令，或取消 Host 已存在的引用限制。

## TDD 與實作

1. `agent_profiles.rs` 新增只保留 ID、Revision、引用數量的 `AgentProfileRemovalImpact`，不攜帶 Command、Managed Config、Prompt 或認證資料。驗證 Host ID、精確 Revision、`exists=true`、完整引用清單與合理引用計數；格式不符、引用存在都不會出現可確認刪除的狀態。
2. `agent_runtime_bridge.rs` 新增受限 `RemovalImpact` 與 `RemoveConfirmed` 請求。統一 `--no-start`，不隱式啟動 Host。確認刪除前**再次查詢並比較完整最小影響快照**，不相符則拒絕；刪除時傳入相同 Host Revision 及 `--confirm`，成功後再次 Host `list`，確認目標 ID 已消失。
3. `native_settings_ui.rs` 提供兩個分開的操作：**Review deletion impact**、僅無阻擋引用且 ID／Revision 仍一致才顯示的 **Confirm delete selected**。切換選取、刷新清單、建立／Rename／Reorder 均會清掉舊預覽。錯誤或 Host 離線不假報成功。
4. 先加入 `profile_removal_preview_is_revision_guarded_and_fails_closed_on_references`，測試 RED（E0425，解析器不存在）→ GREEN；涵蓋 ID／Revision 錯誤、格式不完整與 Automation＋Tab 的阻擋引用。另有 `profile_removal_requires_revision_no_start_and_explicit_confirmation`，確認 CLI Preview 無 `--confirm`、刪除一定有 `--confirm`、兩者皆含 Revision 與 `--no-start`。
5. Windows 完整回歸：`cargo test --offline --locked --lib -- --skip git_history::tests::explicit_full_history_search_finds_old_subject_and_message_body_beyond_page_limit` **533 PASS／0 FAIL／1 FILTERED**（既有 Git History 長測仍未執行）；Native Binary **32／32 PASS**；Windows EXE Build、`cargo fmt --all -- --check`、`git diff --check` PASS。

## 隔離 Windows GUI 實機正向驗收

- 在 `desktop_gpui/target/smoke-t07-profile-remove-runtime-20261113-0842` 啟動獨立 Host；僅建立兩筆一次性 `cmd.exe` Command Profile，其中 `T07_DeleteFixture` 先直接經 CLI 刪除，用於確認 JSON 契約；GUI 所用的是另一筆 `T07_GUI_DeleteFixture`，ID `prof_cf8f64303a4347df86660faca6573356`、revision 0。
- `desktop_gpui/target/smoke-t07-profile-remove-gpui-state-20261113-0842` 中啟動新版 Windows GPUI PID `44052`，Settings → Agent Profiles 顯示 **1 Profile**。
- GUI 點 `Review deletion impact`：Host 回覆無阻擋引用，畫面出現「has no blocking references. Deletion cannot be undone.」及獨立的 `Confirm delete selected`；點下確認後，畫面回報 Host 已確認刪除、Host Profile 清單變成 **0 筆**。
- 獨立的 `alera-cli agent-profile --no-start --json list` 再讀回 **0 筆**，確認此測試 Profile 不存在，沒有誤刪其他 Profile。隔離 GUI 正常關閉（PID `44052` 不再存活），僅停止此輪獨立 Host，最終 `running=false`。
- 保護測試專案 `.gpui-center-diff-smoke` 的原有 `M center-diff-smoke.txt`、`?? center-diff-source-switch-untracked.txt` 完全未變；未清理 `target/`，沒有碰觸正式 Runtime、任何 Provider 登入或正式 Agent Profile。

## 追加獨立 Host CAS 負向驗收（同日）

- 使用另一個完全隔離的 Runtime：`desktop_gpui/target/smoke-t07-removal-independent-runtime-20261113-0850`，只建一次性 `codex`／`cmd.exe` Command Profile（沒有啟動 Provider），不存取正式帳號。第一次建立、讀取 `removal-impact`（0 個阻擋引用）、`--confirm` 刪除與最後 `list` 0 筆均成功。
- 第二次在相同隔離 Runtime 建 Profile，先以 Host `update --expected-revision 0` 將名稱與 revision 更新為 **1**。接著使用**舊 revision 0** 執行 `removal-impact`，Host 回覆 revision conflict 並拒絕；即使加上 `remove --confirm --expected-revision 0`，Host 仍拒絕且再次 `list` 證實原 Profile 及 revision 1 保留。
- 重新查詢 revision 1 的 impact（`blockingReferenceCount=0`），再以 revision 1 刪除，Host 回覆 `removed=true`。僅停止該隔離 Host，重複核對直到 `running=false`；原 fixture／正式 Runtime 及憑證未更動。**這是 Host CLI 實測，非被引用 Profile 的 GUI 阻擋實測。**

## 邊界與後續

- **被引用的實際 Host Profile 未在 GUI 觸發真實 Automation／Tab 引用案例**；本輪對此有 Rust parser/狀態限制測試，不把未實測的阻擋 GUI 情境寫成 PASS。
- T07 剩餘：Classic 的完整 **Advanced Edit／Managed 配置／Clone**、真實 Provider **未登入與過期憑證的可辨識提示**。基本 Create／Rename／↑↓ Reorder／Delete 已有獨立 Host 正向驗收，但不能推論所有進階選項已對齊。
- **NOW 仍 T07；`supported=false`**。未合併 `main`、Push、部署或 Release。
