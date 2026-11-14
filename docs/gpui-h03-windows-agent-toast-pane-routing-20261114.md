# Alera GPUI H03 — Windows Agent Toast 與精確 Pane／Host Session 定位

日期：2026-11-14（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；H03 起始 HEAD `92d439ba9`。
狀態：**Toast 點擊精確定位核心已 TDD 驗證；真正 Windows Toast 展示／通知點擊／Classic + Native 所有權仍待實機驗收。** `supported=false`。

## 現有通知接線（保留，不重複造輪子）

- GPUI `AgentNotificationTracker` 由已認證 Host Session 狀態觀察 Waiting／Blocked／Done（Done 必須啟用「完成通知」，中斷完成不通知），初次快照不重播。每一完整 Host Session 身分與 State 使用 60 秒 cooldown；`AgentNoticeCoalescer` 使用 3 秒聚合／10 秒上限與每 Session 最新狀態；重連、無權限與解除等待時清除候選。
- `enabled_notice_sessions` 僅接受 Runtime Host 認證的 Provider Hook switches 和 running Agent；通知偏好由共用 Rust Host DesktopSettings 控制，缺漏／離線一律拒絕。
- `WindowsAgentToastPresenter` 透過 Windows WinRT 建立原生 Toast，通知 XML 不包含 prompt、tool input、credential、Host Session Id；單一 Agent 的 Activated callback 傳入本機型別化 `AgentNotice`；多 Agent 聚合只顯示數量而不猜哪個 Tab。
- 真 GUI 的 `flush_agent_notification_delivery` 在 GPUI event loop 處理 Windows activation，再於 `Window` context 使用 `activate_agent_notice` 定位。
- **正式版並未取得跨 Classic + Native 獨佔發送權**：`native_agent_notification_policy` 只有在 Windows Debug 且 `ALERA_GPUI_AGENT_TOAST_EXCLUSIVE_TEST=1` 的明確隔離測試下才開啟 Native 發送；Classic 的 Flutter 通知另有自己的 App ID。不能把該測試開關當成生產雙客戶端協調機制。

## 此次 H03 真正修正

先前 `activate_agent_notice` 會在找到同名 Host Tab 後直接呼叫焦點，沒有共用一個「已認證 Host Agent + 存在於哪個 mixed Pane」的可測決策，後續對雙 Pane 與既有 Terminal preview 的維護風險較高。

本輪新增純函式 `validated_notice_pane_target`：

1. 再次確認目前**仍 running** 的 Host Session ID、Workspace ID、Tab ID、Agent Type 都與 Notice 完全相符；Notice 發出後 Waiting→Done 的同一 Agent 可以安全定位。
2. 搜尋目前 `WorkbenchPaneLayout` 的 Primary、Secondary 原生 Tab；只接受對應 `WorkbenchTabs` 中 `WorkbenchTabKind::Agent` 且 running 的預覽。普通 Terminal、Editor、過期 Session、回收 Handle 一律拒絕。
3. 返回完整 `WorkbenchTabIdentity` 和正確 `PaneSlot`；**純查詢不移動 Tab、改變焦點、建立或終止 PTY**。原 `focus_mixed_tab` 依既有 Pane owner 處理視窗輸入焦點。
4. 點擊時原本允許「目前 Workspace 中已驗證但還沒開預覽」以 `select_terminal` 開啟本地 view 的安全行為保留；不會做 Host `terminal.create`。跨 Workspace 仍明確拒絕，避免 Toast 驅動無同意的 Workspace/dirty Editor 切換。

## TDD：RED → GREEN

- RED：先新增 `h03_toast_click_routes_to_exact_secondary_agent_without_changing_other_pane`、`h03_toast_click_rejects_recycled_host_and_nonagent_preview`；`cargo test --offline --locked --lib h03_toast_click -j 2` 因缺 `validated_notice_pane_target` 得到預期編譯錯誤 E0425。
- GREEN：加入 Helper 並接上真 `main.rs` Toast activation 後，同一命令 **2/2 PASS**。
- Windows GPUI Rust Library 全量 `cargo test --offline --locked --lib -j 2`：**577 PASS／0 FAIL／1 ignored**（包含原 60 秒去重、聚合、重連、權限關閉、Toast 身分防護等回歸）。
- Windows Native Binary Test、EXE Build、格式／Git 差異檢查：最終結果見本輪提交／追蹤表。

## 第二階段：WinRT 實機 Smoke 與 Native AUMID 捷徑（2026-11-14）

本階段增設 **預設 `#[ignore]`** 的 `h03_winrt_smoke_toast_show_and_activation_for_isolated_agent`，需要 `ALERA_GPUI_AGENT_TOAST_SMOKE=1` 明確啟用，僅傳一則虛構 Waiting Agent；不啟動 Host、不修改正式 Provider Hooks、不寫正式設定。測試先將 Cargo test runner Process AppUserModelID 設為 `dev.leynier.alera.native`（正式 GPUI Main 已有相同設定），呼叫原 `WindowsAgentToastPresenter::show`，輪詢至多 45 秒確認單一 Agent 的 activated callback 身分。

發現既有 Native 只有 Process App ID、**未附 Windows 開始選單 AppUserModelID 捷徑**。參考 Microsoft unpackaged desktop Toast 要求，新增 `tool/release/install_alera_native_toast_shortcut.ps1`（ShellLink／IPropertyStore／IPersistFile，x64 PROPVARIANT），選擇式 `Check`／`CreateFile`／`VerifyFile`／`Install`／`Uninstall`。所有模式都僅允許 Native 的獨立 App ID，預設唯讀；不存在就拒絕覆寫其他人捷徑；Uninstall 只移除能驗證 Native App ID 的固定項目。

- **捷徑屬性驗證 PASS**：先在專案暫存路徑新建 `.lnk`，另一個 PowerShell Process 使用 `VerifyFile` 讀回正確 App ID；測試 `.lnk` 已移除。
- **Windows 開始選單註冊 PASS**：測試前 `Get-StartApps` 無 Alera Native，手動 opt-in 註冊後能查得 `Alera Native Preview → dev.leynier.alera.native`，另可 `Check` 讀回；兩次測試後均以經驗證的 `Uninstall` 移除，沒有修改 Classic、已存在其他捷徑或 Runtime Host。
- **WinRT Toast 展示／Activation NOT VERIFIED（隔離測試失敗）**：單一 Toast 的 `Show()` 無 HRESULT 錯誤，但在原測試、註冊 Start App 後、以及 Cargo runner 顯式設定 Process App ID 後，各 45 秒都**未收到 Activated callback**；兩個 Windows 顯示器截圖均未確認看到這則測試 Toast。測試行程與 Explorer 同屬 Windows Session 1，因此不能單純歸因於 Session 0。這是可重現的實機驗收缺口，**不是 PASS**。
- **預覽封裝 PASS**：`package_alera_native_windows.ps1` 僅將可選擇執行的捷徑註冊 PowerShell 工具加入 ZIP 與說明；兩個 PowerShell 腳本 Parser **2/2 PASS**，以獨立臨時假 EXE／CLI 封裝驗證 ZIP 含五項預期檔案與 `.sha256`，**H03_PACKAGE_SMOKE=PASS**；不自動執行、不授權公開 CI、不建 Release，`supported=false`。
- **更新後全量回歸 PASS**：Windows Rust Library **577 PASS／0 FAIL／2 ignored**（第二個 ignored 為刻意 opt-in 且實機失敗的 WinRT 測試，未冒充通過）；Native Binary 原 **44/44 PASS**、Windows EXE Build 原 **PASS**。

後續在真正 GPUI 運作時仍需查核 Windows 通知設定／靜音或橫幅政策、AppID 及通知歷史、通知 callback 的生命週期，並用經驗證的 GUI Native Agent 走 Waiting／Done 真實正向流程。另應透過共享 Host 做 Classic／Native **獨佔通知所有權**（含重啟、衝突與到期），不能以不同 AUMID 取代互斥；在那之前保持 Native production 端禁發通知，不假稱 H03 DONE。

## 完成邊界與下一步

- 已驗證：Agent 通知的核心冷卻、狀態聚合、失效防護；本輪新增雙 Pane 精確路由與 Stale Host 身分拒絕。
- **已驗**：Native AUMID Start Menu shortcut 的 ShellLink 寫入、讀回與 Windows `Get-StartApps` 註冊，測完完整撤銷。**尚未通過**：Toast 系統實際顯示、點擊 callback、從背景喚醒 GPUI HWND 與正確鍵盤焦點、真 Host Waiting／Done、跨 Workspace 導航。
- **尚未完成**：Classic 與 Native 同時執行時的 Host 協調／獨佔 Toast ownership。安全原則是正式版 Native 在無可靠互斥所有權時不投遞，避免重複通知；不能把 Debug opt-in 當成 Host lease。
- 不修改正式 Runtime Host、Agent Hooks、Provider 帳號或使用者資料；不動受保護 `desktop_gpui/target`／`.gpui-center-diff-smoke`；不合 `main`、不 Push／部署／Release；`supported=false`。
