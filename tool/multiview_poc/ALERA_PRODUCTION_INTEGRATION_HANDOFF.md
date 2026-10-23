# Alera 單 Flutter Engine 多視窗：正式主程式實驗整合交接（2026-10-23）

## 範圍與安全隔離

- 隔離 worktree：`.worktrees/fix-right-sidebar-20261020`；專用分支 `poc/single-engine-multiview-20261022`。
- **未合併、未推送 main，亦未取代任何已安裝的 Alera。**
- **預設功能關閉**：一般 Windows 啟動維持 `FlutterViewController + runApp` 與 Socket/獨立程序拆窗。新插件會註冊，但實驗 Shared Engine 只在命令列唯一完整參數為 `--alera-shared-engine-tabs` 時啟動；獨立 Tab 子程序仍走舊流程。
- 實驗單引擎共享 `ProviderScope`，僅 Editor Tab `WorkspaceTabKind.editor` 可走共用狀態；Terminal、Git Diff、PDF 等保留多程序 fallback。尚未驗證其餘類型在 single-engine root 下與舊流程的實機共存。
- 使用 `ALERA_E2E_DESKTOP=1` 使原生 App ID 獨立；且必須把 `ALERA_E2E_SUPPORT_DIR=C:/a_mv_20261022/e2e_support` 編譯進 Flutter kernel，隔離 Alera SQLite、TerminalHost 與 AppLogger 主要資料。建置腳本在缺少此 Dart define 時直接停止；不可在真實個人設定上測試此實驗版。
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

- **17/17 Flutter 測試通過**：
  ```powershell
  flutter test --no-pub test/unit/detached_tab_entrypoint_compile_test.dart test/unit/shared_engine_detached_backend_test.dart test/unit/shared_engine_detached_window_lifecycle_test.dart test/unit/detached_tab_window_service_test.dart test/widget/detached_tab_save_shortcuts_test.dart
  ```
  涵蓋既有 Socket 手握／失敗回退、共享後端 opt-in、重複拆窗、啟動參數、child WM_CLOSE 在來源卸載途中及已掛載後、返回需等 source/child frame barrier、Ctrl+S、Meta+S、Terminal 不誤觸 Editor save。
- MSVC 語法、正式 CMake Windows `flutter_window.cpp` / `main.cpp` 原生物件編譯通過。
- 使用 Ninja Multi-Config 完整編譯 **Windows Debug EXE** 並完成 CMake install，正確安裝 `alera_native.dll`、`code_forge.dll`、`flutter_local_notifications_windows.dll`、`dartjni.dll`、Flutter kernel + assets、terminal-related native DLL；程序 exit code 0。
- 先前缺 Rust DLL 的確切原因：單獨呼叫 Ninja 時沒有 `FLUTTER_ROOT`，導致 Cargokit 找不到 Dart SDK，且舊 build-tool script 回傳碼可誤判成功。測試腳本現在尋找 SDK 並**檢查實際 DLL 存在**，已從此 worktree 成功編出 Rust Alera Native 與 CodeForge。
- 為遵守 MCP workspace executable policy，另於本 worktree `build/windows/shared_gui_ninja/runner/Debug` 完成可執行的 Windows GUI Debug bundle（未複製日期/來源不明 DLL，並確認必需產物）。
- **實機主視窗 smoke 過關（於子窗最新改動前）**：使用 `--alera-shared-engine-tabs` 啟動了本 worktree 的測試 EXE，Dart VM service 與 Impeller 初始化；Windows 程序 Responding=True 且有主 HWND；實際載入上述 Alera native、CodeForge、multiview DLL；產生 `C:/a_mv_20261022/e2e_support/alera.sqlite` 與 TerminalHost 相關測試資料；使用 `CloseMainWindow` 對該測試 PID 發送 WM_CLOSE，正常退出，其他 worktree 的 Alera 進程沒有受到影響。
- 上述 **主視窗啟動成功不等於實際 Editor 拆出成功**；最新的快捷鍵、關窗 frame barrier 只有 Flutter tests 驗證，尚未重編並進行該版 GUI 操作。
- `git diff --check` 與 Dart formatter 通過。先前整個 root 的 `flutter analyze` 超時，尚未確認全量 analyzer lint clean。

## 仍待實際 GUI/安全驗證

1. 重新以 `build_alera_in_worktree_debug.cmd` 編入最後的 shared Editor shell 變更，檢查 isolated Dart define 仍存在。
2. **使用真正的 Editor**：選擇測試工作區中的測試檔（不要操作個人工作專案），在 dirty buffer 狀態拆出/保存/返回，反覆至少 20 次，觀察文件快照、selection、scroll、CodeForge session 與 Ctrl+S；無主子雙重掛載。
3. 拖回主視窗目前**只支援按 Return 按鈕**，尚未做 drag-back 原生 HWND hit-test；要做使用者要求的「拖出／拖回」UX 仍需新增。
4. 子窗 WM_CLOSE、雙螢幕與混合 DPI、IME、視窗/文字輸入/系統剪貼簿、Native method-channel 視窗歸屬、平台 tray/close、安全回退、Terminate/restart、長期記憶體和啟動速度均待實機核對。PoC 的結果不可當成正式 Alera 結果。
5. 預設安裝環境能否完全避免因新增插件而提升啟動資源需比較。
6. 功能目前不應合併 main / 做正式 release；保留 opt-in + 多程序 fallback。

## 重現建置流程（Windows / MSVC + Ninja）

在本 worktree：
```powershell
# 首先讓 Flutter 產生包含隔離資料目錄的 Windows generated_config.cmake：
flutter build windows --debug --no-pub --dart-define=ALERA_E2E_SUPPORT_DIR=C:/a_mv_20261022/e2e_support

# 此主機上直接 flutter build 可能因 MSBuild 編譯器偵測失敗，
# 若 config 已成功產生，改用下列 Ninja 腳本。
# 此腳本會拒絕缺少精確 E2E 支援目錄編譯旗標的情況：
tool\multiview_poc\build_alera_in_worktree_debug.cmd

# 完整程式碼回歸：
flutter test --no-pub test/unit/detached_tab_entrypoint_compile_test.dart test/unit/shared_engine_detached_backend_test.dart test/unit/shared_engine_detached_window_lifecycle_test.dart test/unit/detached_tab_window_service_test.dart test/widget/detached_tab_save_shortcuts_test.dart
```
- 實驗測試版在 `build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe`；真正 E2E 啟動需要**唯一參數** `--alera-shared-engine-tabs`。
- 需 MSVC 18 Community vcvars、Ninja 與既有可重用同專案 CLI sidecar（測試專用環境 `ALERA_E2E_REUSE_CLI=1`），避免大量無關 Rust CLI 重編譯。
- 輔助工具 `tool/multiview_poc/` 含：native 語法測試、Ninja 配置、隔離 Build、FFI DLL 存在性檢查與日誌檔版本。CMake 短路徑 `C:/a_mv_20261022` 在本主機仍有，不應視為正式版本輸出。
- GUI E2E 路徑必須保持資料隔離，且勿以全域 Desktop 輸入干擾另一個使用中的 Alera/Parsec 工作階段。

## 結論

**進度：真實 Alera Windows 主窗單 Engine 實驗路徑可編譯、連結、完整打包並安全啟動退出；Editor 子窗只有契約、生命週期與儲存測試通過，尚未達正式 GUI 20 輪驗證或可合併主線的程度。**
