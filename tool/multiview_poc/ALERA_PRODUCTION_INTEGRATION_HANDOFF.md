# Alera 單 Flutter Engine 多視窗：正式主程式實驗整合交接（2026-10-23）

> **歷史整合紀錄。** 本頁下文保留最初「預設關閉、需指定 `--alera-shared-engine-tabs`」時期的測試狀況，並非目前 Windows 啟動政策。後續已完成預設啟用與回退實機驗證，**請以 [DEFAULT_WINDOWS_SHARED_ENGINE.md](DEFAULT_WINDOWS_SHARED_ENGINE.md) 為準**；Windows 不帶參數現在預設為 shared Engine，`--alera-legacy-detached-tabs` 可強制恢復原多程序模式。

## 範圍與安全隔離

- 隔離 worktree：`.worktrees/fix-right-sidebar-20261020`；專用分支 `poc/single-engine-multiview-20261022`。
- **未合併、未推送 main，亦未取代任何已安裝的 Alera。**
- **預設功能關閉**：一般 Windows 啟動維持 `FlutterViewController + runApp` 與 Socket/獨立程序拆窗。新插件會註冊，但實驗 Shared Engine 只在命令列唯一完整參數為 `--alera-shared-engine-tabs` 時啟動；獨立 Tab 子程序仍走舊流程。
- 實驗單引擎共享 `ProviderScope`，僅 Editor Tab `WorkspaceTabKind.editor` 可走共用狀態；Terminal、Git Diff、PDF 等保留多程序 fallback。尚未驗證其餘類型在 single-engine root 下與舊流程的實機共存。
- 使用 `ALERA_E2E_DESKTOP=1` 使原生 App ID 獨立；且必須把 `ALERA_E2E_SUPPORT_DIR=<worktree>/build/windows/shared_gui_ninja/e2e_support` 編譯進 Flutter kernel，隔離 Alera SQLite、TerminalHost 與 AppLogger 主要資料。建置腳本在缺少此 Dart define 時直接停止；不可在真實個人設定上測試此實驗版。
- 新版僅支援 Windows；macOS/Linux 插件相依變更雖生成註冊檔，但尚未編譯、驗證。

## 實作與修正

1. `windows/runner/main.cpp`, `flutter_window.cpp/.h`：原生精確啟動旗標，實驗路徑透過 `MultiViewDesktopPrepareEngine` / `CreateMainView` 共用同一 Flutter Engine，並從 engine registrar 取得有效 `BinaryMessenger` 註冊 Alera 五個 MethodChannel；native WndProc、font reload、close 與 Flutter 視窗生命週期分支。正常啟動不改原有 `FlutterViewController` 流程。
2. `windows/runner/CMakeLists.txt`：加入 `flutter_wrapper_plugin`，修正實際 MSVC `LNK2019 PluginRegistrar*` 連結錯誤。Debug Runner 正式編譯連結完成。
3. `lib/main.dart`：唯一參數精確 gate；實驗版用 `runMultiApp(globalScope: appScope)` 共用同一 Riverpod/Editor Session/Rust/DB provider。普通模式仍 `runApp`。
4. `lib/src/features/workbench/application/detached_tab_window_service.dart`：可插拔共享後端介面，合併 detached/pending 狀態通知，保留所有 unsupported tabs 的既有 Socket fallback。
5. `lib/src/features/workbench/presentation/shared_engine_detached_tab_window.dart`：先開輕量視窗，等來源 Editor 下架並 materialize CodeForge dirty snapshot 後才掛載子窗 Editor，避免同一 tabId 有兩份有效 Editor；關閉子 HWND 後標記 `nativeClosed`，等 child teardown frame 再恢復 main Editor；啟動中關窗不產生未觀測的 Future error。
6. 子窗 Editor 共用 `EditorSessionRegistry` 提供 Ctrl+S / Cmd+S、失敗 toast、Localization、TextActionsScope / ToastHost；返回主窗使用 native View ID，不呼叫全域 `windowManager.destroy()`。
7. 原分頁拖出/右鍵選單只有在確定共享後端適用時才允許 dirty Editor 移轉。多程序模式仍要求先存檔，Terminal 仍維持原資源管理。
8. `multiview_desktop` 版本鎖定 2.0.2。依賴與插件原始碼仍需供應鏈/相容性審查。

## 已通過驗證（2026-10-23）

- **21/21 Flutter 測試通過**（加上 LanguageIntelligenceActivityStore 新增的非同步通知回歸測試）：
  ```powershell
  flutter test --no-pub test/unit/language_intelligence_activity_test.dart test/unit/detached_tab_entrypoint_compile_test.dart test/unit/shared_engine_detached_backend_test.dart test/unit/shared_engine_detached_window_lifecycle_test.dart test/unit/detached_tab_window_service_test.dart test/widget/detached_tab_save_shortcuts_test.dart
  ```
  涵蓋既有 Socket 手握／失敗回退、共享後端 opt-in、重複拆窗、啟動參數、child WM_CLOSE 在來源卸載途中及已掛載後、返回需等 source/child frame barrier、Ctrl+S、Meta+S、Terminal 不誤觸 Editor save。
- MSVC 語法、正式 CMake Windows `flutter_window.cpp` / `main.cpp` 原生物件編譯通過。
- 使用 Ninja Multi-Config 完整編譯 **Windows Debug EXE** 並完成 CMake install，正確安裝 `alera_native.dll`、`code_forge.dll`、`flutter_local_notifications_windows.dll`、`dartjni.dll`、Flutter kernel + assets、terminal-related native DLL；程序 exit code 0。
- 先前缺 Rust DLL 的確切原因：單獨呼叫 Ninja 時沒有 `FLUTTER_ROOT`，導致 Cargokit 找不到 Dart SDK，且舊 build-tool script 回傳碼可誤判成功。測試腳本現在尋找 SDK 並**檢查實際 DLL 存在**，已從此 worktree 成功編出 Rust Alera Native 與 CodeForge。
- 為遵守 MCP workspace executable policy，另於本 worktree `build/windows/shared_gui_ninja/runner/Debug` 完成可執行的 Windows GUI Debug bundle（未複製日期/來源不明 DLL，並確認必需產物）。
- **實機主視窗 smoke 過關（於子窗最新改動前）**：使用 `--alera-shared-engine-tabs` 啟動了本 worktree 的測試 EXE，Dart VM service 與 Impeller 初始化；Windows 程序 Responding=True 且有主 HWND；實際載入上述 Alera native、CodeForge、multiview DLL；產生 `<worktree>/build/windows/shared_gui_ninja/e2e_support/alera.sqlite` 與 TerminalHost 相關測試資料；使用 `CloseMainWindow` 對該測試 PID 發送 WM_CLOSE，正常退出，其他 worktree 的 Alera 進程沒有受到影響。
- 後續已完成真正 Editor 拆出及保存、帶 dirty buffer 返回、原生 WM_CLOSE、20 輪實機壓測；見後方 2026-10-23 最新驗證段落。
- `git diff --check` 與 Dart formatter 通過。先前整個 root 的 `flutter analyze` 超時，尚未確認全量 analyzer lint clean。

## 尚未完成的驗證

1. Ctrl+S 實機（widget test 通過，但 Windows 前景政策拒絕了本次測試工具的 SendInput）。子窗與主窗 Save 按鈕實機通過。
2. 20 輪原生拆窗/返回及一次 dirty buffer 交接已成功；仍需真正滑鼠拖回命中測試、多 Tab 併發、selection、scroll、撤銷歷程、操作中跨窗切換等更深層狀態驗證。
3. 拖回主視窗目前**只支援按 Return 按鈕**，尚未做 drag-back 原生 HWND hit-test；要做使用者要求的「拖出／拖回」UX 仍需新增。
4. 子窗 WM_CLOSE（含 dirty buffer）已通過；仍需雙螢幕混合 DPI、IME、系統剪貼簿、Native method-channel 每窗路由、平台 tray/close、外部錯誤回退、Terminate/restart、長期資源變化及 Release 的完整互動延遲。
5. 預設安裝環境能否完全避免因新增插件而提升啟動資源需比較。
6. 正式 release 或預設啟用仍未達標；保留 Windows Editor-only opt-in + 多程序 fallback。是否將「預設關閉的實驗功能」合併 main，依最後的合併門檻測試判斷。

## 重現建置流程（Windows / MSVC + Ninja）

在本 worktree：
```powershell
# 首先讓 Flutter 產生包含隔離資料目錄的 Windows generated_config.cmake：
$root = (Get-Location).Path.Replace('\', '/')
flutter build windows --debug --no-pub --dart-define="ALERA_E2E_SUPPORT_DIR=$root/build/windows/shared_gui_ninja/e2e_support"

# 此主機上直接 flutter build 可能因 MSBuild 編譯器偵測失敗，
# 若 config 已成功產生，改用下列 Ninja 腳本。
# 此腳本會拒絕缺少精確 E2E 支援目錄編譯旗標的情況：
tool\multiview_poc\build_alera_in_worktree_debug.cmd

# 完整程式碼回歸：
flutter test --no-pub test/unit/language_intelligence_activity_test.dart test/unit/detached_tab_entrypoint_compile_test.dart test/unit/shared_engine_detached_backend_test.dart test/unit/shared_engine_detached_window_lifecycle_test.dart test/unit/detached_tab_window_service_test.dart test/widget/detached_tab_save_shortcuts_test.dart
```
- 實驗測試版在 `build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe`；真正 E2E 啟動需要**唯一參數** `--alera-shared-engine-tabs`。
- 需 MSVC 18 Community vcvars、Ninja 與既有可重用同專案 CLI sidecar（測試專用環境 `ALERA_E2E_REUSE_CLI=1`），避免大量無關 Rust CLI 重編譯。
- 輔助工具 `tool/multiview_poc/` 含：native 語法測試、Ninja 配置、隔離 Build、FFI DLL 存在性檢查與日誌檔版本。CMake 短路徑 `C:/a_mv_20261022` 在本主機仍有，不應視為正式版本輸出。
- GUI E2E 路徑必須保持資料隔離，且勿以全域 Desktop 輸入干擾另一個使用中的 Alera/Parsec 工作階段。

## 2026-10-23 最新真實 Editor GUI 驗證（取代前述部分待驗證項目）

- 使用本 worktree 的 Windows Debug EXE，唯一實驗參數 `--alera-shared-engine-tabs`。先啟動/關閉一次讓隔離 Runtime DB 初始化，再以 `python tool/multiview_poc/seed_shared_editor_test_workspace.py --reset-test-file` 建立獨立測試 Project、Workspace 和 `handoff_test.dart`。測試資料全部位於 worktree 被忽略的 `build/windows/shared_gui_ninja/`。
- 真實 Alera 專案側欄能看到測試 Project / Workspace；選取後 CodeForge Editor 正常顯示。從右鍵 Move to New Window 成功建立第二個原生 HWND。Windows PID 32332 保持不變；主窗與子窗各有自己的 Flutter View。`capture_alera_gui_window.ps1` 已擷取兩窗實際內容。
- 向子 Flutter HWND 送出文字後，Editor 畫面呈現新的 dirty buffer，磁碟尚無新字串；點子窗 Save 後磁碟真的寫入。再建立未保存文字並按子窗 Return，主 Editor 重新掛載後保留 dirty 文字、磁碟未被修改；最後在主窗 Save 可成功寫檔。
- 另測試對含 dirty buffer 的子 HWND 直接發原生 `WM_CLOSE`；第二個 HWND 關閉，主 Editor 自動顯示原 dirty 文字、磁碟仍未改，主窗再 Save 正常落地。
- `probe_real_alera_editor_cycles.ps1` 跑過真正 Alera **20/20 次**原生 Editor 拆窗/返回：16 次 Return UI、4 次 Windows `WM_CLOSE`。每輪可見 HWND 由 1→2→1，沒有留下子視窗。原生 HWND 生命週期 Open P95 **526 ms**，Close P95 **278 ms**；曾見單次 Open 856 ms、Close 742 ms。**這不是放開拖曳至畫面完全可互動的端到端延遲，也不等於 release 版速度。**
- 記憶體及物件紀錄：20 輪起始 RSS 631.8 MiB，結束 657.3 MiB；Handle 1061→1092；GDI 18→18。短測有資源增量，尚須較長時間觀察回收斜率，不能聲稱完全無洩漏。明細：`build/windows/shared_gui_ninja/diagnostics/real_editor_native_cycles.json`（忽略版控）。
- 實測發現 Flutter 在 Editor dispose 時 `ref.read` 已失效，以及 CodeForge 建構 RenderObject 時同步廣播 LanguageIntelligence 狀態觸發 `setState during build`。修正：`workspace_editor_surface.dart` 於 initState 快取 keepAlive language registry；`language_intelligence_activity.dart` 的廣播改為 async。TDD 先 RED 後 GREEN；相關 21/21 Flutter 測試通過。新的 E2E 測試時段沒有新增 SEVERE，舊四筆記錄保留，其中一筆為測試工具注入不合法 Windows Ctrl modifier 的斷言，現已移除該測試注入。
- 真正 `Ctrl+S`：Flutter widget shortcut tests 已過；Windows `SendInput` 工具因 foreground 權限拒絕，未發送任何鍵盤事件，所以**實機 Ctrl+S 尚未證明**。子/主 Editor Save 按鈕的實機儲存均已成功。
- 尚需：滑鼠原生拖回 UX 實機手勢（目前 Return 按鈕可行）、Terminal/PDF/Git Diff 等類型、多 Tab 多視窗同時運作、跨 DPI/螢幕 IME 與 plugin channel 路由、Release 效能及較長時間資源壓力。此項清單建立於後續正式 Quit 防護測試前。

重新建置時必須以 Dart define 指向這個 worktree 的 `build/windows/shared_gui_ninja/e2e_support`（不可使用舊的外部 C: 測試 profile）；`build_alera_in_worktree_debug.cmd` 將在 Dart define 不符合時拒絕執行。

## 2026-10-23 合併門檻補測（最新）

- **Windows 實際 Editor 生命週期**：已測 20/20 輪以及額外 200% DPI 的 30/30 輪同程序 Editor 原生拆窗、返回與 WM_CLOSE。高 DPI 30 輪中有 9 輪合成選單點擊需要重試；這是測試工具輸入的不確定性，不能算成 30/30 次手勢一次成功。沒有留下子 HWND；仍缺真正的滑鼠跨視窗 drag-back 端到端手勢驗證。
- **明確 Quit 的資料保護**：在子 Editor 修改但尚未存檔時，主窗選單 Quit 出現 Unsaved detached editors；Cancel 保留兩窗與 Dirty Buffer，Save and Quit 真正把內容寫到磁碟再退出。普通 WM_CLOSE 的「隱藏至背景」是 Alera 原本的視窗政策，不能混同於 Quit。
- **預設舊行為實機回退**：不帶 `--alera-shared-engine-tabs` 啟動隔離 E2E Alera，從 `handoff_test.dart` 的 Move to New Window 確實建立第二個 `alera-dev.exe --alera-detached-tab ...` 子程序（獨立 PID），子窗顯示 CodeForge Editor；Return to Alera 後子程序正常結束，主程序正常留存。這條路不依賴新的共用 Engine。
- **測試**：多視窗/退出/既有 Socket/快捷鍵 31/31、舊子程序初始化和 parser 14/14、Editor/Workbench Widget 59/59，另外 Controller/Shell/圖片預覽/Shared 生命周期擴大回歸 250/250、修改後 child bootstrap 15/15。各批有重疊，不能把數字相加當成不重複案例。Windows Runner 原生 CTest **2/2**（`tool/multiview_poc/test_windows_native_runner.cmd`）。
- **Flutter 靜態分析**：初次全量分析發現原本測試替身簽名及 benchmark 已落後 API，12 項編譯 error；已另行修正測試介面與兩個脆弱斷言，重新執行 `flutter analyze --no-pub --no-fatal-warnings --no-fatal-infos`，**0 error、exit 0**。仍有既有 warnings/info；這不代表 lint 全數歸零。
- **安全界線**：新方案在 Windows 只有命令列精確 `--alera-shared-engine-tabs` 才生效，且只接管 Editor；預設仍沿用多程序拆窗。Flutter `multiview_desktop 2.0.2` 會對 macOS/Linux 產生插件註冊，但本次 Windows 主機未編譯其他桌面平台，**不可把其他平台當成已驗證或做正式全平台 release**。真正拖回操作、Ctrl+S 實機 SendInput、IME、混合 DPI 長期資源回收仍待驗證。
- **合併政策**：以上證據足以在**維持預設關閉、仍有既有多程序回退**的前提下，合併實驗功能至本機 main 供後續測試；不表示可預設啟用或公開發布。

## 結論

**截至 2026-10-23：Windows Editor 單 Engine 多 HWND、儲存/Dirty Buffer、子窗關閉、主程式明確 Quit 防護、以及預設多程序回退均有實機證據；程式分析無 error。可保守合併「預設關閉的實驗功能」到本機 main；仍不可宣稱功能全面成熟、已跨平台驗證或預設取代原本拆窗流程。**
