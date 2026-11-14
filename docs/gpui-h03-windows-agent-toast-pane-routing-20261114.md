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
- **歷史通知點擊 NOT VERIFIED**：前三輪 `Show()` 不報錯但各 45 秒無 Activated callback，桌面截圖未看到即時浮動橫幅。**第三階段補充發現 Windows Action Center 確實收到測試 Waiting Toast（OS 投遞 PASS）**；所以此處舊判斷不能再解讀成 OS 沒投遞。COM 點擊回呼仍待通過，測試與 Explorer 同屬 Windows Session 1。
- **預覽封裝 PASS**：`package_alera_native_windows.ps1` 僅將可選擇執行的捷徑註冊 PowerShell 工具加入 ZIP 與說明；兩個 PowerShell 腳本 Parser **2/2 PASS**，以獨立臨時假 EXE／CLI 封裝驗證 ZIP 含五項預期檔案與 `.sha256`，**H03_PACKAGE_SMOKE=PASS**；不自動執行、不授權公開 CI、不建 Release，`supported=false`。
- **更新後全量回歸 PASS**：Windows Rust Library **577 PASS／0 FAIL／2 ignored**（第二個 ignored 為刻意 opt-in 且實機失敗的 WinRT 測試，未冒充通過）；Native Binary 原 **44/44 PASS**、Windows EXE Build 原 **PASS**。

第三階段已把 Windows OS 通知中心投遞與點擊回呼分開驗證；OS 投遞已通過，仍須證實真正 Native 進程收到 COM 回呼並定位 Host／Pane，以及共享 Host 對 Classic／Native 的獨佔通知所有權（含重啟、衝突與到期）。正式 Native 繼續禁發通知，不能假稱 H03 DONE。

## 第三階段：WinRT 通知中心實機與 Native COM Activator（2026-11-14）

修正第二階段只憑桌面截圖判斷未投遞的結論：Windows 全域 ToastEnabled=1、WpnService 正在執行、ToastNotifier.Setting() 回報 NotificationSetting(0)／Enabled。WinRT History 由 Ok(5) 增至 Ok(7)，開啟 Windows Action Center 能看到 Alera Native Preview 的 H03 test Agent Waiting 訊息。因此 **OS 通知中心實際投遞 PASS**；立即浮動橫幅是否展示仍不保證，不能將兩者混同。

本輪新增 Windows 專用 native_toast_activation.rs：固定 Native CLSID {9C44E5C2-8303-4570-A80A-A78B41F532BD}，實作 INotificationActivationCallback 與 IClassFactory，獨立 MTA 工作執行緒在程序生命週期持有 COM 類別工廠，不自動寫系統登錄。只有 Native AppUserModelID 和限定格式的隨機短期 Token 可以進入回呼佇列，Classic／非 Native App ID 拒絕。通知 XML launch 只存 alera-native: 加 32 碼十六進位 Token，不序列化任何 Host Session／Workspace／Tab 或 prompt；實際 Tab 聚焦仍使用 validated_notice_pane_target 比對最新已認證 Host、Agent 與 Pane。未取得有效映射的跨程序／歷史通知拒絕定位，沒有建立 PTY。

WinRT Presenter 保留原 Activated 兼容回呼，另外接上 Toast.Failed 非同步診斷與 ToastNotifier.Setting 檢查；Native GPUI 啟動時註冊 COM class factory，並避免把 Windows COM -Embedding 參數誤認為 Workspace。正式版 Native 仍不持有 Classic 的通知 owner，不會因此開始發送雙重提醒。

Native opt-in 工具 install_alera_native_toast_shortcut.ps1 現在同時註冊和獨立讀回 System.AppUserModel.ID 與 ToastActivatorCLSID（PKEY 26），Install 僅於 CLSID 無衝突時建立 Native 專用 HKCU LocalServer32 精確 EXE 路徑；Check、Uninstall 必須完整驗證捷徑與登錄均屬 Native，拒絕覆寫 Classic。以專案內假 AleraNative.exe 測試：獨立 PowerShell CreateFile/VerifyFile **PASS**，HKCU 登錄、Start Menu 註冊讀回 **PASS**，Uninstall 與假 EXE 清理 **PASS**（H03_COM_SYSTEM_CLEANUP=PASS）。正式 Host、Agent Hook 與 Classic 安裝未動。

Rust Windows 專項 h03_com_（COM callback、錯 App ID 拒絕、ClassFactory）**2/2 PASS**；cargo check --offline --locked --no-default-features --lib **PASS**，PowerShell 安裝腳本語法檢查 **PASS**。GUI 真實通知中心點擊的 opt-in 測試仍 **0/1 FAILED**：通知進入 Action Center，人工點擊後 45 秒內既有 WinRT Activated 和新 COM token callback 均未回到測試程式；其 LocalServer32 為假 EXE（避免喚醒正式環境），尚需進一步驗證真正 Native 可執行檔的 COM 啟動／登錄與跨程序轉發。**不宣告 Windows 點擊定位 DONE**，不把單元測試取代 OS 實機驗收。

第三階段完整 Rust 驗證：Windows cargo test --offline --locked --no-default-features --lib -j 2 **580 PASS／0 FAIL／2 ignored**（2 個 ignored 含刻意啟用時失敗的 OS WinRT click，不作為通過）；另 Windows cargo check --offline --locked --bin alera-gpui-desktop **PASS**、cargo fmt --check **PASS**、git diff --check **PASS**。 Native Binary 的 cargo test --offline --locked --bin alera-gpui-desktop -j 2 因 Windows GPUI 全部重新編譯後達執行期限，狀態 **TIMED OUT／NOT PASS**；不將上輪 44/44 舊結果冒充本輪通過。

結論：OS 投遞 PASS、COM 介面與 HKCU 註冊／撤銷測試 PASS；實際 Action Center Click→Native Callback→Pane/Session 仍 NOT PASS，Classic／Native Host 單一通知所有權也未做。H03 維持 NOW，supported=false，沒有合 main、Push、部署或 Release。

## 第四階段：跨程序通知點擊功能補齊（2026-11-14）

本階段從已提交的 `3c15da403` 繼續，實際修改 Native Rust 與 Windows Native-only 安裝腳本：

- Windows COM LocalServer32 改為真正 Native EXE 加 `-ToastActivated`；GPUI 不把 COM 啟動參數 `-ToastActivated`、`-Embedding` 誤當專案路徑，無儲存 Workspace 時使用使用者目錄作安全起始視窗。
- 每則單一 Agent Toast 先將完整 Session／Workspace／Tab／Agent 身分以 Windows DPAPI CurrentUser 加密保存於 Native 專用狀態目錄，Windows Toast XML 只送隨機 `alera-native:` Token；跨程序 COM callback 依 Token 一次性取用，過期、錯誤格式、重複使用都拒絕；限制檔案數與負載大小。
- 新 Native COM test 子程序呼叫已註冊的 ClassFactory，驗證非同一程序的 `CoCreateInstance`、Activate 與 Token 取回；先前使用的假 EXE 會得到 `0x800700C1`（不是有效 Win32 程式），已移除該假登錄測試環境，不能據此斷定真正 Native Click 失敗。
- GPUI 接收到通知時先等待已認證 Host Session 清單載入完畢；精確驗證 Session／Workspace／Tab／Agent 類型，再依 Host 註冊的唯一 Workspace ID 找到真實根目錄，透過既有保護未儲存 Editor 的 Workspace 切換流程導航，最後聚焦正確 Pane。通知點擊**不建立或終止 Host PTY**。
- `h03_winrt_smoke_toast_show_and_activation_for_isolated_agent` 現為 opt-in 手動 OS 驗收；它不在 Cargo Test 程序中註冊 COM ClassFactory，須由**真正已安裝 Native EXE**承接 Windows Action Center Click，不能再以假執行檔代替。
- **本輪已實測**：Windows `cargo test --offline --locked --no-default-features --lib h03_ -j 2` **9 PASS／0 FAIL／1 intentionally ignored**（OS 真實點擊測試）；`cargo fmt --all` **PASS**、`git diff --check` **PASS**、Native Installer PowerShell Parser **PASS**。完整 Native GUI Binary `cargo check` 結果獨立記錄，未通過前不能聲稱有可用的新 EXE。
- **驗收邊界**：靜態實作與單元／跨程序 COM 驗證不等於 OS Action Center 真實點擊；Classic／Native 的正式獨佔通知 owner 仍未實作，正式版 Native 不能啟用重複發送，`supported=false`，不合併主線／Push／部署。

## 完成邊界與下一步

- 已驗證：Agent 通知的核心冷卻、狀態聚合、失效防護；本輪新增雙 Pane 精確路由與 Stale Host 身分拒絕。
- **已驗證**：Native AppUserModelID＋ToastActivatorCLSID 的 ShellLink 屬性、HKCU COM LocalServer32 精確註冊與卸載、Windows 通知中心能看到 Waiting Toast；全部測試登錄及捷徑均已還原。**尚未通過**：Action Center Click callback、由背景喚醒 GPUI HWND 並聚焦正確 Pane、真 Host Waiting／Done、跨 Workspace 導航。
- **尚未完成**：Classic 與 Native 同時執行時的 Host 協調／獨佔 Toast ownership。安全原則是正式版 Native 在無可靠互斥所有權時不投遞，避免重複通知；不能把 Debug opt-in 當成 Host lease。
- 不修改正式 Runtime Host、Agent Hooks、Provider 帳號或使用者資料；不動受保護 `desktop_gpui/target`／`.gpui-center-diff-smoke`；不合 `main`、不 Push／部署／Release；`supported=false`。
