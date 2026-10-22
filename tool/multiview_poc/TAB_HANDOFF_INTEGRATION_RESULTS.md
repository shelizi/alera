# Alera 同 Engine Tab 移轉 PoC：第二階段紀錄（2026-10-22）

## 範圍與結論

此為獨立 Windows Flutter PoC，位於 `poc/single-engine-multiview-20261022` 的 `tool/multiview_poc`，**尚未接進正式 Alera**。目的在驗證以下三件實際架構問題，而非宣稱完整 Alera 已能拖出 Tab：

1. 全部原生視窗操作都明確以 View ID 定位，避免 `window_manager 0.5.2` 從子視窗操作到主視窗。
2. Tab 編輯 Buffer 與 Terminal OS 程序不應綁定 Widget 或 HWND 的生命週期；在單 Engine 的兩個視窗間做「原 Widget 卸載 → ticket 提交 → 新 Widget 掛載」。
3. 子視窗被原生 OS 事件關閉時，經 `WindowObserver.onWindowClosed` 自動恢復所有權，無須由測試腳本直接呼叫 recover。

## 新增原型元件

| 程式 | 目的 |
|---|---|
| `lib/view_window_controller.dart` | 明確 View ID 的 `ScopedNativeWindowController`；包裝 `MultiViewDesktop.fromId` 的 title、position、size、focus、close，不依賴全域 HWND |
| `lib/tab_lease_registry.dart` | 編輯 Buffer、Terminal Transcript 的長壽命 Session；單一 Widget mount；generation ticket；unmount barrier；提交、取消、OS 關閉恢復 |
| `lib/persistent_windows_shell_host.dart` | 測試用 `cmd.exe` 管線程序、相同 PID、stdout 持續輸出；**不是 ConPTY，也不是 Alera 真正 Terminal runtime** |
| `lib/handoff_harness.dart` | 真實 Win32 原生視窗／Flutter View 的多次拆出→拖回、儲存內容、Shell PID 保留、close observer，產出可重現 JSON |
| `test/tab_lease_registry_test.dart` | 未儲存內容、游標、100 次原子移轉、重複 mount 拒絕、stale token／失敗取消／突然關窗恢復 |
| `test/view_window_controller_test.dart` | 視窗操作路由，以及錯誤／零 View ID 拒絕 |
| `probe_external_close.ps1` | 只找目標 PoC PID + 指定子視窗標題，傳送實際 Win32 `WM_CLOSE`，確認由原生事件觸發自動恢復 |

正式 Alera 的 `EditorSessionRegistry`、Rope 編輯器、PT​​Y、Riverpod 主容器、Native Channel、Flutter/C++ Runner、Tab Drag 實作都**尚未更動**。

## 編譯與驗證

於 `tool/multiview_poc`：

```powershell
flutter analyze
flutter test
.\build_poc_ninja.cmd
.\build\windows\poc_ninja\runner\Debug\alera_multiview_poc.exe --poc-handoff
.\build\windows\poc_ninja\runner\Debug\alera_multiview_poc.exe --poc-handoff --poc-handoff-stress
powershell -NoProfile -ExecutionPolicy Bypass -File .\probe_external_close.ps1 -Configuration Debug

.\build_poc_release.cmd
.\build\windows\poc_ninja\runner\Release\alera_multiview_poc.exe --poc-handoff --poc-handoff-stress
powershell -NoProfile -ExecutionPolicy Bypass -File .\probe_runtime.ps1 -Configuration Release -Mode handoff-stress
```

執行工作目錄需位於 EXE 目錄，才能直接找到 `poc_handoff_report.json` 等報告。`probe_runtime.ps1` 和 `probe_external_close.ps1` 會自行設定工作目錄。

Windows 11、Flutter 3.47.2、MSVC/Ninja、`multiview_desktop 2.0.2`、`window_manager 0.5.2`。原生 Debug 與 Release 均已編譯且啟動通過。新版 CMake/Ninja 仍會出現路徑較長的警告，正式移植前宜縮短 build 路徑並以 CI 編譯器環境測試。

## 結果：Tab 移轉不遺失 Session

- Flutter analyze：無錯誤；flutter test：**8/8 項測試通過**。
- Debug 8/8 與 Debug 20/20 次原生子視窗拆出→拖回成功，沒有多個 Widget 同時掛載單一 Editor Controller。
- Release/AOT 20/20 次原生子視窗移轉成功。
- 編輯 Buffer 每次移到子視窗後繼續更新，移回後 Dirty 仍為 true；游標 selection offset 保留。測試最後寫入 PoC 專用檔案並清掉 Dirty，讀回文字相同。
- 測試 Terminal 的 `cmd.exe` 程序從頭到尾**同一 PID、只啟動一次**，子視窗狀態前後都能輸出 marker。這只證明長壽命 OS 程序與 Stream 可以獨立存活，不等於已驗證 Alera 的 ConPTY、Ctrl-C、resize、terminal keyboard 或 Shell Resume。
- Debug、Release 20 次測試分別收到 22 次原生關窗事件：20 次正常拖回後關窗、1 次擁有 Tab 時的突然關窗、1 次 transfer pending 時關窗；全部由原生 observer 維護 ownership。
- 從 **外部 PowerShell** 對精確的子視窗 HWND `PostMessage(WM_CLOSE)`，不是 Flutter API 模擬關閉：報告 `externalWmCloseObserved=true`、`unexpectedCloseRecoveredByNativeObserver=true`，最後 Session 回主窗，過期 ticket 無法重新掛載。
- 測試程序正常結束（ExitCode 0）；最後沒有殘留子視窗；cmd host 在 app 結束前收回。

## 效能（不是最終使用者端到端數字）

無額外 OS 資源取樣時，真實 GUI Editor 子視窗的「建立→成功掛載」：

| 同 Engine 原型測試 | 20 次成功 | Editor 掛載 P50 | Editor 掛載 P95 |
|---|---:|---:|---:|
| Debug | 20/20 | 324 ms | 461 ms |
| Release/AOT | 20/20 | 305 ms | 515 ms |

Release 20 次原生視窗建立 API P50 約 231 ms、P95 約 264 ms；包含 Editor + Terminal Widget 卸載/重掛載到子視窗的時點 P50 約 305 ms、P95 約 515 ms。以上是 Flutter Widget 掛載之後再經 frame barrier 的計時，**不是 DWM/GPU 最終完成畫面呈現與可輸入延遲**，而且 Editor 與 Terminal 均為簡化的測試 UI，不能拿此值直接保證正式 Alera 拆 Tab 速度。

外部 500ms Windows GUI/Handle/GDI 取樣會增加 GUI 負載，應與無監測數據分開比較。資源取樣結果見 `probe_runtime.ps1` 產出的 `poc_handoff_process_samples.csv`，不宜將工作集增長直接稱為洩漏。

### 20 次實際交接時的系統資源取樣（另一次實驗）

取樣時排除頭 3.5 秒初始化樣本，再比較仍有 GUI 視窗的取樣點：

| 指標 | Debug | Release/AOT |
|---|---:|---:|
| 取樣筆數（含初始化） | 34 | 35 |
| 工作集約 3.5 秒後 → 測試結束前 | 244.4 → 258.9 MiB | 108.5 → 121.6 MiB |
| 工作集峰值 | 262.4 MiB | 123.4 MiB |
| Windows Handle | 612 → 603 | 590 → 594 |
| GDI objects | 20 → 20 | 20 → 20 |
| USER objects | 39 → 34 | 36 → 34 |
| Editor 掛載到子視窗 P50 / P95（受監控影響） | 373 / 883 ms | 627 / 998 ms |

兩組監控測試同樣 **20/20 次通過、22 次原生關閉事件、Terminal 只啟動一次**。資源取樣時 P95 明顯劣於無監控結果；須區分監控造成的效能負載、其他系統干擾與產品本身效能。Handle/GDI 沒有隨此 20 次移轉持續增加，但尚無 1000 次以上或數小時的可靠記憶體洩漏排除結果。

## 下一步實際整合順序與阻擋項

1. **AppWindowController Window ID 化**：現在正式 Alera 在 `lib/src/features/app_window/infra/window_manager_app_window_controller.dart` 直接使用單例 `windowManager`。新介面應明確帶 View ID 與原生 event listener，不可沿用單 HWND 版本處理子視窗。
2. **原生 Windows Runner 掛多 FlutterView**：`windows/runner/flutter_window.cpp` 目前仍會建新的 Engine／新程序；先封裝在可關閉且預設關閉的 feature flag，保留目前 multi-process fallback。
3. **共用 Workspace 與 Editor Session**：`EditorSessionRegistry`、`WorkbenchState` 等 Provider 必須共享同一原始 Session；Tab move 須先卸載原 Widget 再交付 lease，`Dirty`、Ctrl+S、檔案監控通知與 Document Session 不得重建或誤寫。
4. **Terminal 真正 PTY**：單一 PTY owner；視窗轉移只換 input/output attachment。驗證輸入法、組合鍵、複製選取、Resize、Ctrl+C、背景輸出、子視窗 crash。
5. **拖曳與 UX**：Tab 真的拖出、拖回、跨 DPI、拖曳預覽、第一幀、焦點、視窗關閉、存取最後關閉之視窗的恢復策略；目前此 PoC 是程式化開關，不是 Alera drag UX。
6. **生命週期及安全**：原生 bridge 的所有方法需以 viewId 授權，WindowObserver 在元件 disposed 後執行要避開死引用；子視窗 close 的競態和主視窗 Exit 還需整合測試；第三方套件須審計供應鏈與維護能力。
7. **全平台**：目前**只通過 Windows**，Mac Intel/Apple Silicon、Linux 仍無移植結果。

**結論：** 兩個先前高風險的設計選項（Window ID 控制、Editor/terminal「單一 owner」交接）已在實際 Windows 原生子視窗中完成成功 PoC。尚不能宣稱 Alera 已支援同程序拖出 Tab；正式整合的剩餘風險主要是 Alera 的真 Editor/PTY/Provider 與原生 Runner/插件生命週期。
