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

## 本機測試命令

```powershell
flutter test --no-pub test/unit/detached_tab_entrypoint_compile_test.dart test/unit/shared_engine_detached_backend_test.dart test/unit/shared_engine_detached_quit_guard_test.dart

tool\multiview_poc\test_windows_native_runner.cmd
```

此模式不應在其他平台直接啟用。要降低事故擴散，發佈前須確認 Windows Release/installer 的 Runtime、資料路徑與同程序多視窗相容性；本次驗證是 Windows Debug GUI，不等於 Release 已完成。
