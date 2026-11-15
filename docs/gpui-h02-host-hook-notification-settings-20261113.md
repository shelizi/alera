# Alera GPUI H02 — Hook 開關與通知偏好：Classic ↔ Native 真 Host CAS／事件／重啟

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；H02 起始 HEAD `a75bb103d`。
狀態：**Codex／Claude Hook 與通知偏好 Rust Runtime Host 接線已通過真隔離 Host 寫讀與重啟；Windows 真 GUI 開關／Devin Hook 警告待驗，`supported=false`。**

## 核心所有權與已存在接線

- GPUI `native_settings_catalog_ui::render_agent_hook_toggle` 對 Codex／Claude 使用 `Switch`，實際呼叫 `Workbench::change_agent_hook_setting`，且必須已有 `AgentHookSettingsSnapshot`；Host 不可用時不會假裝可修改。
- GPUI `RuntimeCliBridge::agent_hook_settings_versioned_snapshot` 透過 `alera.exe settings --no-start agent-hooks-get` 讀取 Host `runtimeSettings.get` 的機密排除投影；`agent_hook_settings_compare_and_set` 再透過 `agent-hooks-cas` 以 **Host revision＋原 Provider bool** 送 `runtimeSettings.update`，驗證原子 CAS 後的 Host Readback，更新失敗清掉舊授權狀態。
- Classic Flutter 的 `runtimeSettings.update` 管理原子 Hook map 與 `revision`，Host 更新完成會廣播 `runtimeSettingsChanged`；GPUI 另會重新載入版本化 Snapshot，操作期間不 Optimistic 顯示。
- `agents.agentStatusNotificationsEnabled` 和 `agents.agentStatusFinishedNotificationsEnabled` **不是** Host Hook revision 下的欄位，而是同一 Rust Runtime Host 所有權的 reviewed portable `desktopSettings`；Classic 使用 `desktopSettings.compareAndSetBatch`，GPUI 使用共享 `change_shared_desktop_setting` → CLI CAS／Snapshot。切勿將兩類設定混成一份本地 JSON。
- 為避免 Classic 與 Native 同時彈 Windows 通知，正式環境 Native Presenter 不主動獲得通知所有權；只有明確隔離 debug opt-in 才能交由 Native 發 Toast（H03 另行驗收）。

## 新增真 Windows Host 整合測試

沿用 `desktop_gpui/tests/s03_shared_host_settings.rs` 的 `IsolatedHost／ClassicRpc` 安全框架，新增 `h02_classic_hook_switches_and_native_notification_settings_persist_through_real_host_restart`，全程使用獨立 Temp Runtime、Home、USERPROFILE、APPDATA、LOCALAPPDATA，並由 Drop 關閉 Host：

1. **無 Host 時 fail closed**：Native Hooks Snapshot 不可用；沒有偷啟動真 Host。
2. **GPUI → Classic**：GPUI Versioned Snapshot 的 Codex bool 以 CLI CAS 修改；驗證 Revision +1、Codex 更新且完整 Snapshot 符合期望；Classic 直接 authenticated `runtimeSettings.get` 讀回同一版本與值。
3. **Classic → GPUI**：Classic 使用實際 `runtimeSettings.update` + `expectedRevision` 原子修改 Claude Hook；GPUI 讀回 Codex 不變、Claude 更新、Revision 再 +1；Classic Host 連線收到 `runtimeSettingsChanged`。
4. **過期 CAS 被拒**：GPUI 使用舊 Snapshot／Revision 修改 Codex 失敗，兩個 Hook 的 Host 值與 Revision 均沒有被篡改。
5. **通知偏好 Classic → Native → Classic**：Classic 使用原 `desktopSettings.compareAndSetBatch` 原子反轉 `agents.agentStatusNotificationsEnabled` 與 Finished Notifications，GPUI Snapshot 讀回兩項；Native 使用自身 CLI CAS 修改 Finished Notifications，Classic 立即可見且另一通知欄位不被覆蓋。
6. **通知偏好衝突保護**：使用舊 `expectedValue` 再寫，Host 拒絕，沒有以舊 Native JSON 覆蓋新設定。
7. **真正關閉／重啟隔離 Host**：停止時兩種 Native 讀取皆 fail closed；同一隔離 Runtime 重新啟動後，Hook revision、Codex／Claude bool 與兩個 Notification bool 都持久化一致。

**執行方式（明確 opt-in）**：

```powershell
$env:ALERA_GPUI_TEST_CLI = (Resolve-Path 'rust/target/debug/alera.exe').Path
cargo test --manifest-path desktop_gpui/Cargo.toml --test s03_shared_host_settings -- --ignored --nocapture
```

**結果：真 Host H02 1/1 PASS**，同檔原 S03 真 Host **1/1 PASS**，累計 **2/2 PASS**。程式除回歸測試與文件外無需變更：真實接線及 Host CAS 正常，故不無故重寫已工作的 Host／CLI／GUI 實作。

## 全量回歸

- **Windows 全量回歸**：Rust Library **575 PASS／0 FAIL／1 ignored**；Native Binary **44/44 PASS**；Windows EXE Build、`cargo fmt --check`、`git diff --check` 全部 **PASS**。既有 opt-in Host 測試為本輪明確手動啟用才實測，不冒充預設全量 Library 的通過。
- 真 Host 流程是兩個真 Client 介面的**端到端**驗證，不代表已點擊兩個 Windows App 的 Settings 或通知開關；不宣稱真 GUI DONE。

## 尚待

- Windows 真 GPUI 與 Classic Settings 同時開啟：人工切 Codex／Claude Hook 開關，跨視窗 UI 同步，關閉重開持久化；通知兩個開關亦實際點選，回讀 Host。
- 既有 T03 真 Devin TUI 曾報 JBContext 使用者 Hook failed；本輪沒有修改任何正式 Hook，也沒有重現／定位此訊息，仍是獨立 Provider 相容性待查，不誤標解決。
- 下一個 H03 再驗證 Windows 原生通知展示、去重、點擊定位正確 Pane／Session，維持 Classic/GPUI 不重複通知的所有權規則。

## 安全界線與排程

- 不動正式 Runtime Host／Agent 帳號／Hook 設定與使用者資料，不清除受保護的 Build／Smoke 目錄，不合 `main`，不 Push／部署／Release。
- **H02 真 Host INTEGRATED／GUI 待驗 → NOW H03**；`supported=false`。
