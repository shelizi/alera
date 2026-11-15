# Alera GPUI S03 — Classic／GPUI 共用設定：真 Host 雙 Client CAS、事件與重啟

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；S03 起始 HEAD：`36604b0d0`。
狀態：**隔離真 Runtime Host 雙 Client／Classic 原生批次 RPC／重啟持久化已通過；Classic＋GPUI 兩個實體 GUI 手動操作待驗，`supported=false`。**

## 核心與 Classic 接線盤點

- Classic Flutter `RuntimeSettingsRepository.compareAndSetSharedDesktopChanges` 使用 Host `desktopSettings.compareAndSetBatch`，`SettingsController` 遇到 `desktop_settings_revision_conflict` 時載入 Host 新值、提示使用者不覆蓋。
- Classic 的 `runtimeSettingsChangesProvider` 接收真正 Host `runtimeSettingsChanged` 事件，遞增 revision 重新載入設定；GPUI 使用 `RuntimeCliBridge::desktop_settings_snapshot`，Settings 開啟時由 1.5 秒 housekeeping 依快慢週期定期刷新。兩者讀寫同一個 Rust Core `RuntimeStore::desktop_shared_compare_and_set_batch`（`BEGIN IMMEDIATE`）。
- 原先 Rust Core 已有雙 `RuntimeStore` 輸入與 CAS 測試，但缺少 **啟動真 Runtime Host、Classic batch TCP RPC、GPUI Windows CLI Bridge、Host stop/restart** 的連續驗證。

## S03 新增真 Windows Host 整合測試

`desktop_gpui/tests/s03_shared_host_settings.rs` 的 `s03_classic_cli_and_gpui_bridge_share_cas_and_persist_after_host_restart`：

1. 建立 `tempfile` Runtime／Home／APPDATA／LOCALAPPDATA，檢查無 Host 時兩條設定讀取都拒絕、沒有偷偷啟動使用者 Host。
2. 啟動**這個整合樹的實際 `rust/target/debug/alera.exe runtime-host`**，等待控制檔 PID／Port 與 Settings CLI 健康讀取，從獨立 `RuntimeCliBridge` 與直接 CLI 讀回一致的 Snapshot。
3. Classic CLI 單欄位更新 `editor.tabSize 4→6`，GPUI 讀 6，再 GPUI 更新 6→8，Classic 讀 8；過期 expectedValue=4 的 GPUI CAS 被拒絕。
4. Classic 更新 `editor.twoRowTabs false→true`，GPUI 讀 true 並改回 false；同時確認過期 CAS 拒絕、舊 Native JSON 不參與。
5. 兩端交互修改 `general.language`（system→繁中→english）及 `terminal.cursorShape`（block→bar），依 Host 真 Snapshot 讀回，不覆蓋不同欄位。
6. 使用 **Classic Flutter 實際的 TCP RPC**：通過隔離 Token `hello` 認證，`desktopSettings.get` 與 `desktopSettings.compareAndSetBatch` 三欄位原子更新 `editor.tabSize`、Fetch 間隔與 `general.confirmWorkspaceRemoval`（僅 Classic 授權 UI 才能修改的安全值）；GPUI CLI 一次讀回三項一致。
7. Classic 發送過期二欄位批次，Host 回傳上層 `errorCode=desktop_settings_revision_conflict` 並拒絕**整筆交易**，沒有只部分覆蓋另一個可寫欄位。
8. GPUI 修改後，已認證的 Classic 連線收到 `runtimeSettingsChanged` 廣播；Classic 再次 RPC 讀回最新版，符合 SettingsController reload 條件。
9. 一般 CLI 對 `general.confirmWorkspaceRemoval` 修改被拒絕，與 Classic batch 的人員授權權限分離。
10. 真正停止 Host（kill+wait）、測試控制檔移除，確認兩端此時 fail closed；同一隔離 Runtime＋Home **重新啟動 Host**，讀回 Tab Size、Two-row tabs、語言、Cursor、Fetch 間隔、安全確認值全部正確持久化，測試後關閉 Host。

**執行方式（需明確 opt-in）**：將 `ALERA_GPUI_TEST_CLI` 指向專案所建 Windows `alera.exe`，執行：

```powershell
$env:ALERA_GPUI_TEST_CLI = (Resolve-Path 'rust/target/debug/alera.exe').Path
cargo test --manifest-path desktop_gpui/Cargo.toml --test s03_shared_host_settings -- --ignored --nocapture
```

**結果：真 Host 整合測試 1/1 PASS**（Windows 兩次啟動，同一 test fixture，完整用例；預設 ignored 避免沒有明確 Host 依賴時意外執行）。全程未使用正式 Host Token／設定庫。

## 本輪發現的 Bridge 正常功能缺口與 TDD

- `RuntimeCliBridge::desktop_settings_compare_and_set` 原本僅確保 CLI 以 0 狀態碼結束，且 Snapshot 的 `values` 是 Object；並**未驗證返回指定 Path 的實際值等於要求寫入的 next**。Host 若返回不一致的成功 Snapshot，GPUI 可能顯示保存成功而未保存對應值。
- 新增 `validate_desktop_settings_cas_readback(path,next,snapshot)`，在 CLI CAS 成功後強制檢查該欄位；錯誤／缺欄位回傳 `InvalidData`，`terminal.fontSize` 保留等值整數／浮點數語意比較，不向 UI 暴露無關 Host 值。
- TDD **RED**：新單元 `s03_shared_settings_cas_rejects_mismatched_or_missing_host_readback`，先因 E0425 缺 helper 失敗；**GREEN**：正常、錯誤、缺欄位、浮點數等值 4 種情況回歸通過，真隔離 Host CLI 測試重跑 **PASS**。

## 最終回歸

| 驗證項 | 結果 |
| --- | --- |
| Windows 真 Host／Classic Batch RPC／GPUI CLI／重啟 | **1/1 PASS**，測試含安全權限、CAS、事件及多欄位持久化 |
| 新增 Bridge 單元 TDD | **1/1 PASS**（RED→GREEN） |
| GPUI Library | **571 PASS／0 FAIL／1 ignored**（原有隔離 Host 測試未執行、不計作通過） |
| GPUI Native Binary | **43/43 PASS** |
| Windows EXE、fmt、diff | **全部 PASS**（`cargo build --bin alera-gpui-desktop`、`cargo fmt --check`、`git diff --check`） |

## 尚待 Windows 真 GUI 驗收

- 在**隔離 Host 與隔離 Classic 設定個人資料**下，實際啟動 Classic／GPUI 兩個桌面視窗，從 Classic GUI 改 Tab Size／語言→GPUI 可見；反向從 GPUI GUI 修改→Classic Event reload 可見。
- 兩個視窗同時編輯同一欄位，確認過期 CAS 錯誤 UI 提示、另一個視窗的新值不被覆蓋；兩者關閉／重啟與 UI 顯示值一致。以上目前**沒有真 GUI 滑鼠／鍵盤操作證據**，不標記 GUI DONE。

## 版本控制與下一項

- 僅改 GPUI Bridge 讀回防護與可重現的隔離 Host 回歸；正式 Classic Host／設定／`main` 不動；不 Push／部署／Release；受保護 WIP／Build 目錄原樣。
- **S03 核心／真 Host 已驗、GUI 待驗 → NOW S04 Settings i18n**；保留 `supported=false`。
