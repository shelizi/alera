# Alera Windows 共用 Flutter Engine 多視窗：深入測試（2026-10-22）

> 範圍：僅 `poc/single-engine-multiview-20261022` 的 `tool/multiview_poc`，不是正式 Alera 的 Tab 拆出功能。主分支與 production Runner 未變更。

## 測試環境

- Windows 11（25H2），Flutter 3.47.2 / Dart 3.13.2。
- VS Community 2026 / MSVC、Ninja Multi-Config。
- `multiview_desktop 2.0.2`，整合兼容性測試還引入與 Alera 相同的 `window_manager 0.5.2`（及其 screen_retriever 依賴）。
- 各原生子視窗共用同一 PID / Flutter Engine / Dart isolate，**沒有額外啟動一個 Alera.exe**。
- 編譯腳本：`build_poc_ninja.cmd`（Debug），`build_poc_release.cmd`（Release/AOT）；建置完成自動 install 到 `build/windows/poc_ninja/runner/{Debug,Release}`。

## 重現指令

從 `tool/multiview_poc` 目錄執行：

```powershell
flutter pub get
flutter analyze
flutter test
.\build_poc_ninja.cmd
powershell -NoProfile -ExecutionPolicy Bypass -File .\probe_runtime.ps1 -Configuration Debug -Mode long
powershell -NoProfile -ExecutionPolicy Bypass -File .\probe_keyboard.ps1 -Configuration Debug
.\build_poc_release.cmd
powershell -NoProfile -ExecutionPolicy Bypass -File .\probe_runtime.ps1 -Configuration Release -Mode stress
```

手動測試使用建置後 EXE：

```powershell
cd build/windows/poc_ninja/runner/Debug
.\alera_multiview_poc.exe --poc-window-manager
.\alera_multiview_poc.exe --poc-display
.\alera_multiview_poc.exe --poc-anchor-close
.\alera_multiview_poc.exe --poc-anchor-keep
```

會在 EXE 的工作目錄輸出相對應 `poc_*.json` 與追蹤紀錄；build 及產出檔案預設不提交。PowerShell 鍵盤測試會短暫切換前景視窗，結束後嘗試還原先前的前景視窗。

## 一、長時間／首幀／資源監測

Debug：單程序、共用 Engine，連續 **200/200 次** 建立/關閉子視窗，全部通過；再同時建立三個子視窗（連主窗最多四個原生可見視窗），全部關閉後 Flutter View 數回到一個。

| 指標 | P50 | P95 | 最大 |
|---|---:|---:|---:|
| `openWindow()` API 回傳 | 191.6 ms | 220.6 ms | 297.1 ms |
| Dart 子視窗首幀回呼 | 171.5 ms | 190.3 ms | 253.9 ms |
| `closeWindow()` API 回傳 | 72.0 ms | 81.0 ms | 85.0 ms |

注意「首幀回呼」只指 Dart `addPostFrameCallback` 發生，並非 DWM / GPU 已完成呈現、可操作時間，更不是 Alera Tab 拖出到可編輯畫面的端到端延遲。

Windows OS 外部取樣共 185 筆、約 101 秒：
- 忽略程序尚未初始化的最早 0–5 秒取樣（當時 RSS 和 Handle 會接近 0）。
- 約 6.2 秒：Working Set 230.9 MiB、Handle 607、GDI 18、USER 39。
- 約 100.7 秒：Working Set 246.4 MiB、Handle 612、GDI 18、USER 34。
- 10–40 秒視窗建立期間工作集峰值 271.9 MiB；40–80 秒及 80 秒後維持相近範圍。Handle/GDI 沒有呈現每次開窗都持續累加的趨勢。
- Private Bytes 約 411→481 MiB，存在資源保留／快取／潛在洩漏的可能，**不能據此宣稱完全沒有記憶體洩漏**；後續需要更長且更貼近真實 Alera 工作量的測試。

## 二、Release/AOT 與相同插件的 Debug 對照

加入 `window_manager` 及 `screen_retriever` 後另跑：

| 指標 | Debug 50 次 | Release/AOT 50 次 |
|---|---:|---:|
| 通過 | 50/50 | 50/50 |
| `openWindow` P50 / P95 | 199.2 / 244.6 ms | 241.4 / 264.5 ms |
| Dart 首幀回呼 P50 / P95 | 174.1 / 224.6 ms | 219.7 / 245.3 ms |
| Close P50 / P95 | 73 / 81 ms | 72 / 80 ms |
| 程序最後工作集 | 242.4 MiB | 107.7 MiB |

Release 工作集比較低，但建立延遲本次測試反而較高。這不是嚴格控制背景負載與開機順序的 A/B 實驗，不能推論 Release 一定更慢，也不能把 Debug 測值當成正式產品保證。

## 三、插件：找到確定的視窗路由問題

`--poc-window-manager` 在子視窗中呼叫 `windowManager.setTitle('WM_CHANGED_FROM_CHILD')`：

- 子視窗原標題保持 `MVD_CHILD_BEFORE_CALL`。
- **主視窗標題意外變成 `WM_CHANGED_FROM_CHILD`**。
- `MultiViewDesktop.fromId(childId).setTitle(...)` 正確修改指定子視窗，不影響主窗。

結論：`window_manager 0.5.2` 現有 HWND 選擇不支援以不同 Flutter View 為基礎的呼叫來源；Alera 不可直接把原本 `windowManager` 全域控制用在子視窗。需新建以 `Window ID` 為參數的 WindowService，並調查其他 native plugin 及自訂 channels。

## 四、雙螢幕與有效 DPI

Windows 連接兩個解析度不同螢幕：3840×2160（主）、3072×1728（副）。`--poc-display` 測試：

- 起始主螢幕 Flutter View `devicePixelRatio=2.0`。
- 子視窗移到副螢幕時，該 View 自動更新到 **1.25**。
- 改變子視窗大小成功（`setSize` 回傳 true），View 的 physicalSize 隨之更新。
- 移回主螢幕時，子視窗 DPR 恢復 **2.0**。
- 子視窗關閉後只剩主視窗的 Flutter View。
- PowerShell 非 PMv2 context 下用 `GetDpiForMonitor` 看到 192/192 不具代表性；本實驗的 Flutter View 真實 DPR 記錄是 2.0↔1.25。

**尚未測試**注音候選字幾何／Terminal 游標位置／Edit hit-test，不能用一般 Flutter View 成功來取代該測試。

## 五、鍵盤焦點與輸入

`probe_keyboard.ps1` 會先辨識同 PID 的兩個 HWND，確定目標真的成為前景視窗後再使用 `SendInput` 送 Unicode 鍵盤事件。

- 第一輪主動切回第一視窗時，Windows Foreground Policy 拒絕 SetForegroundWindow（事件未送到該窗）；第二個視窗正常收到文字。此為測試環境焦點權限，不能判為 Flutter 輸入路由失效。
- 補強「Alt + AttachThreadInput」且確認當前 HWND 後再測：兩個子視窗分別正確收到 **ALERAA**、**ALERAB**，沒有交叉輸入。
- Unicode `SendInput` 不涵蓋台灣注音 TSF/IME 組字及候選字選擇，後續需真正 IME 手動／原生自動化測試。

## 六、主視窗關閉生命週期

兩個子視窗先建立並正常渲染，然後主視窗呼叫 `closeWindow()`：

- `--poc-anchor-close`：預設 `CloseMode.softCascade` 會關閉全部視窗、退出程序（觀察到 ExitCode 0，沒有後續子視窗 Dart 活動）。
- `--poc-anchor-keep`：明確選擇套件 `@experimental CloseMode.none` ＋ `enableDynamicAnchor=true`，主窗關閉後兩個子視窗仍正常存在，程序與 Dart 計時回呼繼續運作。原生視窗列表剩 [2,3]，主 Flutter View 0 仍在 `PlatformDispatcher.views`（隱藏 anchor／Flutter Engine 宿主）。
- 因為 `CloseMode.none` 是套件實驗 API，正式產品還需測關閉最後一個子視窗、重新打開主窗、Tray App Quit、插件 HWND 與 Flutter Engine 回收時機。

## 關鍵風險與下一階段

1. 此原型仍未掛上真正 Alera 的 Tab、編輯器 Dirty Buffer、Terminal PTY、Rust FFI、Git Diff、Riverpod、Native Channels、拖出／拖回 UX。
2. `window_manager` 子視窗路由有可重現問題；這是進入 Alera 整合前的 **阻擋項**。
3. 主窗關閉後保留子窗仰賴實驗 API；要另外設計退出策略／崩潰復原。
4. 需在同樣隔離 worktree 中做 feature flag／fallback，先完成原生 Window ID 和 Tab ID 對應，再以 Editor→Terminal→Git Diff 次序遷移；所有已修改但未儲存內容必須單一 owner 原子交接。
5. Release 端到端拖出→首幀→可互動測試、IME、雙 DPI 上的 xterm cursor hit-test、1000 次開關與長時間記憶體是否增長，仍未完成。
6. 套件僅限 PoC，正式採納前需審查該第三方 Flutter 插件的維護風險與相依套件供應鏈。

**結論：共享 Engine 多視窗已由原生 Windows 程序、Dart 首幀回呼、200 次長跑、Release AOT、跨 DPI 和鍵盤輸入實驗支持；但 Alera 正式整合尚有明確阻擋項，不能宣告可直接取代目前多程序方案。**
