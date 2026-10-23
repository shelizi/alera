# Windows 預設共享 Flutter Engine：啟用、回退及驗證

> 2026-10-23。此文件記錄由原本的 opt-in Windows Editor 模式，改成 Windows 正常啟動的預設模式。既有 `ALERA_PRODUCTION_INTEGRATION_HANDOFF.md` 記錄了此前預設關閉的研究歷史；本文的啟動政策優先。

## Windows 啟動政策

| 執行方式 | 主視窗 Flutter Engine | Editor 拆窗 | 非 Editor 拆窗 |
|---|---|---|---|
| `alera-dev.exe`（沒有參數） | 共用 Flutter Engine / `runMultiApp` | 同程序新原生 HWND / Flutter View | 原本的獨立程序 |
| `alera-dev.exe --alera-shared-engine-tabs` | 共用 Flutter Engine（舊參數相容） | 同程序 | 原本的獨立程序 |
| `alera-dev.exe --alera-legacy-detached-tabs` | 原本 `FlutterViewController` | 原本的獨立程序 | 原本的獨立程序 |
| `alera-dev.exe --alera-detached-tab ...` | 原本的獨立子程序 Engine | 不再作為主窗拆窗 | 子程序依舊 |
| 未知／複合啟動參數 | 為了相容性採原本模式 | 原本的獨立程序 | 原本的獨立程序 |

macOS、Linux、Android、iOS 不變；Windows 的 Dart 和 Win32 啟動判斷在 `lib/main.dart` 與 `windows/runner/shared_engine_startup_mode.h`，各有自動化測試。已啟動的 Alera 執行個體有 single-instance gate：要使用回退參數，需先**完整 Quit**，再以該參數重新啟動；只關閉到系統匣不算重新啟動。

## 資料安全及目前限制

- 新版同程序只接管一般 Editor。Terminal、Git Diff、Git History、PDF、Markdown 還未具備完整每窗生命週期，繼續走已驗證的多程序實作；對不能安全直接拆出的 Dirty 舊程序 Editor，維持原有先儲存要求。
- Editor 的 Dirty Buffer 在拆出、Return 或原生 WM_CLOSE 時保留；真正 Quit 會檢查 Dirty Editor，允許 Cancel 或 Save and Quit，磁碟寫入失敗則取消退出。
- Windows Native `WM_CLOSE` 仍經 Dart window lifecycle / hide-on-close 處理；**不要將關閉主視窗的 X 和明確 Quit 視為同一件事。**
- 本變更並未將新架構設為 macOS/Linux 預設，也沒有驗證各種 IME、跨螢幕拖回手勢、Release 下完整互動延遲與長期記憶體洩漏。保留參數回退以應對未知 plugin 不相容。
- 測試資料僅使用 `build/windows/shared_gui_ninja/e2e_support` 及 `e2e_workspace`，與使用者一般 Alera profile 隔離。

## 2026-10-23 實機驗收（Windows Debug, DPI 192 即 200%）

1. 首次在 Windows GUI `alera-dev.exe` **不帶參數**啟動，顯示主窗。選 Project/Workspace、Editor 的 Move to New Window 後原生視窗由一個變兩個，但所有視窗都屬同一 Windows PID；真正 CodeForge Editor 能顯示。
2. 在新子 Editor 輸入標記、確保未存檔後 Return：視窗回到一個，主 Editor 原 Dirty Buffer 仍在、磁碟尚未寫入；主 Editor 的 Save 按鈕真正存入磁碟，PID 未變。
3. 完整 Quit 測試程序。用 `--alera-legacy-detached-tabs` 重新啟動同一 Debug EXE；拆出 Editor 實際得到第二個 `alera-dev.exe --alera-detached-tab ...` Windows PID；Return 後子程序結束，主程序保留，最後正常退出。
4. C++ Runner CTest `test_windows_native_runner.cmd` **3/3 通過**，含 Win32 單一實例、Flutter View 銷毀防護、預設/回退命令列判斷；Dart 模式判斷、共用 backend、Quit guard **11/11 通過**。
5. 之前同 Engine opt-in 已進行高 DPI 原生建立/關閉 30/30 輪（部分合成右鍵事件需重試）；更大範圍 regression 見歷史交接文件。模式改成 default 後，將在此基礎上繼續補足高 DPI/混合螢幕、IME、鍵盤快捷鍵與真實拖回的 UX 驗收。

## 2026-10-23 Windows Release AOT 實機驗證（新增）

此項驗證使用獨立的 E2E App ID、資料目錄與 Release 二進位，**沒有覆蓋任何使用者已安裝的 Alera**：

- 標準 `flutter build windows --release --no-pub` 在本機 VS 2026/MSBuild 環境遇到既有 CMake 編譯器偵測問題，故由 `tool/multiview_poc/build_alera_in_worktree_release.cmd` 使用 VC 環境與 Ninja Multi-Config 建置。Flutter 產出的 `flutter_windows.dll.lib` 補上 CMake `BYPRODUCTS`，解決 Ninja 首次連結圖缺少此產物的問題。
- 實際從此 worktree **重新編譯** Release Rust CLI、Flutter AOT、Windows 主程式、`multiview_desktop`、CodeForge 及 Alera Native DLL；隔離套件位於 `build/windows/shared_release_ninja/runner/Release`，其中 `data/app.so`、`data/icudtl.dat`、原生插件及 CLI sidecar 均存在。Release Ninja 建置及安裝退出碼 **0**。
- 初次無參數啟動 Release GUI：正常顯示完整主窗；OS 回報 DPI=192（200%）、程序 `Responding=True`，隔離 SQLite schema 已初始化。使用 `seed_shared_editor_test_workspace.py --configuration Release --reset-test-file` 植入 **獨立** E2E Workspace 和 `handoff_test.dart`。
- 再次無參數啟動、開啟 CodeForge Editor、右鍵 Move to New Window：主窗與子窗由一個變兩個原生 HWND，**PID 55708 不變且全程只有一個 Alera Release 程序**；子窗截圖顯示 Editor 確實渲染。
- 在子 Editor 以限定 E2E HWND 的 `WM_CHAR` 輸入 `RELEASE_DIRTY_20261023`；螢幕顯示文字、磁碟原檔仍無該字串。按子窗 Return 後 HWND 回到一個，主 Editor 顯示同一段未儲存內容，磁碟仍未更動；點主 Editor Save 後檔案確實包含該字串。
- 完整 Quit 後以 `--alera-legacy-detached-tabs` 重新啟動 Release：主程序 PID 54568，Editor Move to New Window 確實啟動 PID 5248，且 command line 含 `--alera-detached-tab`。子窗顯示 CodeForge，Return to Alera 後**子程序結束、主程序正常留存**。最後兩者均正常退出。
- 上述 GUI 自動化只向精確比對過的隔離測試程序及 HWND 送事件，結果截圖位於被忽略的 `build/windows/shared_release_ninja/diagnostics/`；不代表實機鍵盤 Ctrl+S、跨原生 HWND 滑鼠拖回或混合 DPI 多螢幕輸入均已完整通過。

## 本機測試命令

```powershell
flutter test --no-pub test/unit/detached_tab_entrypoint_compile_test.dart test/unit/shared_engine_detached_backend_test.dart test/unit/shared_engine_detached_quit_guard_test.dart

tool\multiview_poc\test_windows_native_runner.cmd

# 僅用於已使用隔離 Dart define 產出 generated_config.cmake 的 Windows 主機
tool\multiview_poc\build_alera_in_worktree_release.cmd
python tool/multiview_poc/seed_shared_editor_test_workspace.py --configuration Release --reset-test-file
```

此模式不應在其他平台直接啟用。Windows **Release AOT 已完成本 worktree 隔離 GUI 驗證**，但仍不是已簽章、已封裝的正式 installer，尚未做跨螢幕真實拖回、IME、長期記憶體與完整全平台發布驗收。新模式在 Windows 預設開啟，但保留 `--alera-legacy-detached-tabs` 作為恢復路徑。

## Fresh worktree 靜態分析與獨立 PoC 測試

在新的 worktree 直接 `flutter analyze` 時，獨立的 `tool/multiview_poc` 尚未生成自己的 `.dart_tool/package_config.json`，因此根專案的分析器會把該範例的 package import 誤當成 Alera 專案依賴，產生 14 項錯誤。根專案 `analysis_options.yaml` 已將獨立 Flutter 專案排除，與原本已排除的 `mobile/` 和 `rust_builder/` 相同。

- 在根專案執行 `flutter analyze --no-pub --no-fatal-warnings --no-fatal-infos`：**0 error、exit 0**，尚有 12 warning 與既有 info 提示。
- 另外在 `tool/multiview_poc/` 執行 `flutter pub get --offline` 與 `flutter test --no-pub`：**8/8 項通過**。不要以根專案的分析排除取代 PoC 自己的測試。
