# Alera Terminal / xterm2 5.4.0 整合與維護盤點（2026-10-31）

## 目標與決策

優先減少與上游分岔的產品程式碼、保留 Alera 特有的輸入與平台回歸修復，不為了追版犧牲已驗證行為。這次只調整 xterm2 相依邊界，不重寫 PTY、Ghostty VTE、跨 Workspace 的 Session 轉移或 Terminal Host 契約。

## 整合基線

- xterm2 fork：`chore/sync-xterm2-5.4-20261031`，由 `origin/alera` 合併官方 `SoFluffyOS/xterm2` 5.4.0。初始已驗證合併提交 `7826c7a`，維護規範追加提交 `b4fd960`。
- Alera：`chore/alera-xterm2-5.4-20261031`，起點為當時 `main` 的 `3f1e9d1f3`。既有版本更新提交 `0429a2d3f`。
- `xterm2 5.4.0` 與 `xterm2_core 1.0.0` 以同一份 `third_party/xterm` submodule 成對覆寫；不可只替換其中一個為 pub.dev 版本。
- Alera 直接宣告 `xterm2: ^5.4.0` 和 `xterm2_core: ^1.0.0`。Worker、Replica、Buffer、搜尋模型、連結解析與緩衝記憶體預算改由 `xterm2_core` 提供純 Dart 型別；Flutter 畫面仍使用 `xterm2/ui.dart`。
- 這是減少依賴耦合的改動；**未量測到的 CPU、FPS、記憶體改善不可當成成果宣稱**。Core 與 UI 仍由 Dart compiler 決定實際 tree-shaking。

## 上游已吸收：不要再重複實作

- Buffer resize / compacted history、OSC 52 剪貼簿解析與邊界、未協商時的 Shift+Enter CSI-u 相容、終端互動參數、文字快取與 OSC 8 清理頻率，皆有官方 5.4.0 對應提交。
- 上游另外新增 Windows ConPTY 選取/換行、TUI 捲動模式、Kitty private-use key 與 KeyUp 過濾、拖曳選取。
- 以上與 Alera 的原補丁可能只是部分重疊，合併時要依照**實際程式碼及回歸測試**判斷，不因提交名稱近似就刪除 Alera 測試。

## Alera 仍需保留的差異

| 範圍 | 依據 | 刪除前須證明 |
| --- | --- | --- |
| macOS Claude Code 貼上後多餘 `V`（文字回送、Kitty KeyUp、原生選單） | `6e38263`、`6b365fc` | 實機 Cmd+V、PlatformMenuBar 貼上、Kitty 模式及正常輸入不回歸 |
| macOS 注音組字與候選字框 | `3ddbd46` | 實機注音組字、候選框位置與 Enter |
| Windows IME Enter 與組字期間誤送 PTY | `84fe8a6`、`421bd10` | Windows 注音／英文輸入互不干擾 |
| Tab 切換／子視窗輸入 client | `9f6e7cb`、`9243ed9` | 多視窗切換與 PTY session 存活 |
| Shift+Click 擴展選取 | `8be5c31` | 滑鼠追蹤 TUI、跨行選取 |
| Reflow 邊界負索引 | `e522068` | `resize(0, ...)` 及滿行寬字、重複 resize 測試 |

這些補丁目前存在於合併 fork，**不表示已在 macOS / Windows 真機上證明全部修復**。

## 回歸門檻與結果

| 範圍 | 2026-10-31 的結果 |
| --- | --- |
| fork 的純 Dart core | 694 通過 |
| fork Flutter（依既有 CI 排除 `platform-golden`） | 339 通過，2 略過 |
| Alera 原先的 Worker / Buffer / Replica | 56 通過 |
| Alera 原先的 UI / Native Runtime | 190 通過，2 略過 |
| Alera 這次調整後的 Core / 搜尋 / 預算等 | 96 通過 |
| Alera 這次調整後的 UI / Runtime / xterm regression | 200 通過，2 略過 |
| fork 全套 Flutter analyze | 0 問題 |

上表不同輪次可能重跑同一個測試，不能把所有列數直接相加當成獨立測例數。平台 Golden 曾有 2 項像素差異，既有 fork CI 將它們獨立呈報，不能當成實機 UI 保證。Alera 全專案靜態分析與平台實機驗收另行判定。

## 更新與交付護欄

1. 在 fork 上保持上游 `master` 鏡像與客製 `next` 分開；新版本先在獨立分支比較 `git range-diff`，按核心、輸入、UI 模組逐項保留必要修補。
2. 至少執行 `dart test`（`third_party/xterm/core`）、`flutter test --exclude-tags platform-golden`（fork）、Alera Worker / UI / Runtime 回歸測試與靜態分析。任何核心輸入修補要附能重現舊錯的測試。
3. 提交並推送**可供 CI 抓取的 fork commit** 後，Alera 才能安全引用該 gitlink；只在本機存在的 submodule commit 不可直接發布。
4. 原本 Alera `main` 為 dirty（含 `third_party/xterm` 指標），不得以 `reset --hard`、強制 checkout 或覆寫方式整合；不碰其他進行中的 Terminal worktree。
5. 不改動跨專案 Terminal transfer 的 supported gate；該功能在其既有獨立門檻通過前仍不得合併、Push 或 Release。xterm 升級可獨立評估，但不代表跨專案 Terminal transfer 已可啟用。

## 下一輪上游 PR 的優先順序（無須擴大本輪範圍）

- 先提小型、可單獨測試的 `e522068` Reflow guard。
- 再提與通用 Kitty Keyboard 交疊的 KeyUp / modifier 相容測試。
- 最後再拆分 Mac/Windows IME 與原生選單貼上：它們需要平台事件證據，避免 upstream 因沒有真機可驗證而合進錯誤防護。
