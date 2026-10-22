# Alera 單 Flutter Engine 多視窗原型 (Windows)

## 隔離範圍

- 工作樹：`.worktrees/fix-right-sidebar-20261020`
- 分支：`poc/single-engine-multiview-20261022`
- 原型：`tool/multiview_poc`
- Alera 主程式 `lib/`、`windows/runner/` 未修改；此為單獨 Flutter app，不是完整 Alera。
- 不需要預熱、沒有每次啟動子程序：原生套件將多個 Windows HWND / FlutterView 綁至單一 Flutter Engine / Dart isolate。
- 第三方套件固定 `multiview_desktop 2.0.2`；僅適合實驗，整合前需供應鏈與 API 穩定性審查。

## 環境與重現

測試環境：Windows 11、Flutter 3.47.2、Dart 3.13.2、Visual Studio Community 2026 / MSVC x64、Ninja Multi-Config，Debug。

在原型目錄執行：

```bat
flutter pub get
flutter analyze
flutter test
build_poc_ninja.cmd
cd build\windows\poc_ninja\runner\Debug
alera_multiview_poc.exe --poc-smoke
alera_multiview_poc.exe --poc-smoke --poc-stress
```

預設模式：10 次建立/關閉、再同時建立 3 個視窗。 `--poc-stress`：50 次建立/關閉、再同時建立 3 個視窗。輸出 `poc_multiview_results.json` 與 `poc_startup_trace.txt` 至目前執行目錄。正式基準應改成 Release/AOT 並從拖曳放開時計算至「可操作首幀」。

由於此遠端環境的 MSBuild FileTracker 啟動失敗 (`System.ArgumentException: Path.GetPathRoot`)，`flutter build windows --debug` 無法完成 CMake 編譯器偵測，改以 VS 2026 Developer Command Prompt 與 Ninja 構建。原型的 `windows/flutter/CMakeLists.txt` 補充 `flutter_windows.dll.lib` BYPRODUCTS，`windows/CMakeLists.txt` 修正 MSVC warning suppress 旗標（`/wd4100`）；皆只改原型。

## 實測（2026-10-22 台北時間）

| 指標 | 首輪 | 加強測試 |
|---|---:|---:|
| 循環 | 10/10 通過 | 50/50 通過 |
| 同時子視窗 | 未納入首輪 | 3/3 成功，連主窗共 4 View |
| 建立 API 回傳中位數 | 208 ms | 199 ms |
| 建立 API 回傳 P95 | 253 ms | 213 ms |
| 關閉 API 回傳中位數 | 74 ms | 73 ms |
| 關閉 API 回傳 P95 | 83 ms | 81 ms |
| Dart 共享狀態計數器 | 最後 10 | 最後 53 |
| 全部子視窗關閉後剩餘 View | 1 | 1 |

加強測試 PID 49292，建立 API 回傳範圍 183–216 ms、關閉 62–84 ms。Debug RSS 每循環取樣 224.58–247.40 MiB，首循環約 227 MiB、末循環約 247.4 MiB。此記憶體增量尚無法區分 cache/GC/retained resources；沒有足夠證據排除記憶體洩漏。

完整測試：`flutter analyze` 0 issues；`flutter test` 1/1 通過（驗證多個 Widget 可同時讀取共享 Dart notifier）；Windows Native Debug 編譯、安裝與 10 / 50 循環實際 GUI 端測試成功。Flutter 的 `PlatformDispatcher.views` 在多視窗時如預期從 1→2 或 1→4，關閉恢復為 1。子視窗未額外啟動 exe。

## 重大限制及整合前工作

1. 此原型沒有載入 Alera 的 Editor/Terminal/Git Diff、Riverpod、CodeForge、Rust、SQLite；因此上述毫秒數**不可等同正式 Tab 拖出至內容首幀**。
2. 當前 `openWindow()` Stopwatch 測量的是 API Future 回傳，不是實際 Flutter 首幀顯示、輸入可用時間或拖曳 UX。
3. 需測試 Windows Runner 自訂 Native Channel（拖曳、檔案拖入、rich clipboard、desktop presence）及 `window_manager` 在同 Engine 多視窗的路由是否正確；有些插件仍隱含單視窗假設。
4. 搬移 Terminal 需要 PTY 單一 owner 與多 View 繫結；搬移 Editor 需要 dirty buffer 與 save session 的原子移交；關閉任一子視窗不應觸發主程式 exit。
5. 需要驗證雙螢幕不同 DPI、IME 注音、焦點/鍵盤快捷鍵、多次拖回、視窗關閉 100 次、背景執行、正式 Release/AOT、記憶體長跑，以及 Windows 原生 `WM_DESTROY` 防護。
6. 共用 Engine 避免重複初始化，卻犧牲部分子程序崩潰隔離，應保留目前多程序方案作為可回退的 feature flag；確認無 crash 後才考慮合併。

本實驗禁止直接改 main / 直接替換 Alera production runner。
