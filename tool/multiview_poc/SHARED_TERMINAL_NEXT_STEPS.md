# Alera Windows Terminal 共用 Engine：後續交接與待辦

> 更新：2026-10-23（Asia/Taipei）。**這是接手工作清單，不是完成驗收報告。** 過去的實驗與測試數據見 [SHARED_TERMINAL_EXPERIMENT.md](SHARED_TERMINAL_EXPERIMENT.md)。

## 1. 接手時的基準狀態

- 專案：Alera；隔離 worktree：`.worktrees/exp-shared-terminal-20261023`。
- 分支：`experiment/shared-terminal-view-20261023`；本次整理前 HEAD：`ff78a30cc`。相關已提交修改：`36ae47211`（Rust EOF／讀取失敗分類）、`704971252`（E2E 雙 SQLite fixture）、`22c0e344c`（Socket frame 邊界與失敗診斷）。
- **Terminal 共用 Engine 是編譯期實驗開關，預設關閉。** Editor 共用 Engine 的既有預設與 `--alera-legacy-detached-tabs` 退回路徑不得破壞。不能在完成驗收前合併成正式預設、推送發行版或觸發部署。
- 最後檢查時，隔離 GUI PID `7464` **已不在執行**，`e2e_support/terminal_host/host.json` 不存在。PID 與點擊座標僅代表舊測試，重新啟動後必須重新取得，**不可沿用 PID**。
- **有 3 個尚未提交的工作檔，請保留，不要 reset/clean：**
  1. `tool/multiview_poc/build_alera_in_worktree_debug.cmd`：容許用 `ALERA_E2E_REUSE_CLI=0` 強制重建 Rust sidecar；平常仍可重用。
  2. `tool/multiview_poc/probe_isolated_e2e_mouse.ps1`：新增 `-PreflightOnly`，改為在 Per-Monitor V2 DPI context 下將畫面邏輯座標換算成螢幕實體座標，且檢查視窗歸屬。
  3. `tool/multiview_poc/probe_real_alera_editor_cycles.ps1`：Terminal 新增 `-NativeMenuClick`，交由上述工具點選選單；尚未證明整個最新腳本的拆窗／關窗循環通過。
- 最新看到的隔離產物時間（**UTC，僅是檔案時間，不是版本驗證**）：`alera-dev.exe` 約 11:35、`data/flutter_assets/kernel_blob.bin` 約 12:17、`resources/alera/alera.exe` 約 13:56。**Flutter kernel 可能沒有包含最後的程式修正，應優先確認重新產出。** 不要只看 EXE 的修改時間。
- 正式 `main` 工作樹可能有其他 agent／使用者的未提交內容；**全程只在隔離 worktree 操作，不覆寫、清理或合併 main 的既有改動。**

## 2. 建議依順序執行的待辦

### P0-A：確認並提交現有測試工具（尚未完成）

- [ ] 在上述 worktree 執行 `git status --short --branch`、`git diff --check`，逐項審查前述 3 個 dirty 檔案；不要直接 `git reset --hard` 或 `git clean -fdx`。
- [ ] `pwsh` Parser 驗證 `probe_isolated_e2e_mouse.ps1` 和 `probe_real_alera_editor_cycles.ps1`，確認 `-PreflightOnly` **不會送出任何滑鼠事件**、超出 HWND 範圍的座標會拒絕，異常／非 E2E PID 會安全中止。
- [ ] 補驗證在 DPI 120 與 192 的螢幕上，`PrintWindow` 邏輯座標和 `WindowFromPoint` 實體座標正確對應；使用 **同一個 E2E 視窗** 的擷取畫面校正 Tab 與 `Move to New Window` 選單位置，不能沿用其他視窗的位置。
- [ ] 工具測試通過後，以獨立小提交保存這 3 個腳本；若未通過，保留 diff 並記錄失敗案例。不得把「點擊命中」冒充產品拆窗成功。

### P0-B：產出可追溯的「同一版本」隔離 Windows Debug（尚未完成）

- [ ] 先確認沒有執行中的 **同路徑隔離** `alera-dev.exe`；若有，透過 `quit_alera_e2e.ps1 -Configuration Debug -TargetPid <新的測試 PID>` 正常結束。若仍有隔離 Host，僅能在驗證其 `host.json`、執行路徑與 PID 完全匹配後關閉，**不可停止其他 Alera/Host 程序**。
- [ ] 於 worktree 根目錄重新產生 Flutter Debug 設定／kernel，四個 Dart define 都要存在（見下方命令）。標準 `flutter build windows --debug` 曾卡在這台主機的 CMake C++ 編譯器偵測；可用既有 Visual Studio + Ninja 腳本完成編譯，但需要記錄實際 Flutter kernel 是否更新。
- [ ] 執行 `ALERA_E2E_REUSE_CLI=0` 的 Ninja fallback，強制重建 Rust CLI sidecar，而不是沿用舊版。確認 `ISOLATED_DEBUG_BUNDLE_READY` 且退出碼為 0、`kernel_blob.bin`、Rust `alera.exe`、native DLL 都存在；核對 Git HEAD、編譯 define、產物時間與執行路徑。
- [ ] 只操作忽略目錄 `build/windows/shared_gui_ninja/{runner/Debug,e2e_support,diagnostics}`；不得安裝或發行這個測試版本。

在 **Windows PowerShell / pwsh，且目前目錄已是實驗 worktree 根目錄** 時使用：

```powershell
git status --short --branch
$root = (Get-Location).Path.Replace('\', '/')
flutter build windows --debug --no-pub `
  --dart-define=ALERA_E2E_DESKTOP=1 `
  --dart-define="ALERA_E2E_SUPPORT_DIR=$root/build/windows/shared_gui_ninja/e2e_support" `
  --dart-define=ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true `
  --dart-define=ALERA_E2E_AUTO_SELECT_TERMINAL=true
# 若上一步遇到已知 CMake 編譯器偵測問題，不代表 Dart defines 已在真正 bundle 中完成驗收。
cmd /c "set ALERA_E2E_REUSE_CLI=0&&call tool\multiview_poc\build_alera_in_worktree_debug.cmd"
```

可用以下驗證 **generated_config.cmake 的 base64 Dart 定義**（仍須另外核對 bundle 編譯時間）：

```powershell
$config = Get-Content windows/flutter/ephemeral/generated_config.cmake -Raw
$flags = @(
  'ALERA_E2E_DESKTOP=1',
  'ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=true',
  'ALERA_E2E_AUTO_SELECT_TERMINAL=true',
  "ALERA_E2E_SUPPORT_DIR=$root/build/windows/shared_gui_ninja/e2e_support"
)
foreach ($flag in $flags) {
  $encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($flag))
  if (-not $config.Contains($encoded)) { throw "Missing E2E Dart define: $flag" }
}
```

### P0-C：完成最新程式碼的實際「拆出 → 原生關閉 → PTY 返回」驗收

- [ ] 在 **隔離 GUI 已退出** 的前提下執行 `python tool/multiview_poc/seed_shared_editor_test_workspace.py --configuration Debug --tab-kind terminal`，確認 `SEED_OK` 與 `SEED_LOCAL_OK`。Local Drift `alera.sqlite` 與 Host `runtime.sqlite` 都應有相同測試工作區；自動選取需要上面的兩個 E2E Dart define，日誌應出現 `E2E test workspace selected`。
- [ ] 從 `build/windows/shared_gui_ninja/runner/Debug/alera-dev.exe` 啟動**隔離版**，取得新 PID；`probe_alera_gui_windows.ps1` 預期在拆窗前看到一個 `Alera Dev` HWND。記錄當時 GUI／Host PID、DPI、Rust Host 日誌起點，避免將先前日誌當作此次執行。
- [ ] 先以 `capture_alera_gui_window.ps1` 擷取該 PID 的 HWND，再用 `probe_isolated_e2e_mouse.ps1 -PreflightOnly` 校對選單的邏輯座標與實際 DPI。沒有經過預檢，不要注入全域滑鼠／鍵盤動作。
- [ ] 以新的 `-NativeMenuClick` 跑 **1 個循環**，確認原生子視窗 `Alera - E2E Terminal` 確實出現在**相同 PID**、子窗終端機互動正常；對**這個子窗**送 `WM_CLOSE`，主窗應恢復為**單一 HWND**，原 shell session ID、先前 scrollback 與新輸入指令都應正常。以真正 PTY 命令（例如 `echo RETURN_PTY_ALIVE`）確認互動，而非只有 HWND 計數。
- [ ] 此 1 輪通過後再漸進擴大 3 → 10 → 30 次循環，記錄每輪成功數、開／關 P95、RSS／Handles／GDI。腳本會輸出 JSON 至忽略的 `build/windows/shared_gui_ninja/diagnostics`；失敗時同時保留 App 與 Host 日誌、視窗擷取及進程狀態。
- [ ] `WM_CLOSE` 再次逾時時，必須**分別**記錄：目標子 HWND 是否存在、Gui PID 是否存活、Host PID 是否存活、子窗是否回到主窗、PTY 是否可重連；不能直接將逾時寫成「產品崩潰」或「通過」。

建議的循環命令（**座標先用該次螢幕擷取校正**）：

```powershell
pwsh -NoProfile -File tool/multiview_poc/probe_real_alera_editor_cycles.ps1 `
  -Scenario TERMINAL -ChildTitle 'Alera - E2E Terminal' `
  -Cycles 1 -NativeCloseEvery 1 -NativeMenuClick `
  -TabX 477 -TabY 65 -MenuX 400 -MenuY 268 `
  -ReportName terminal_postfix_cycle_01.json -PreflightOnly
# 預檢成功、確認該次 popup 真正位置後，移除 -PreflightOnly 再執行。
```

### P0-D：定位啟動後偶發 `socket_eof`，確保恢復舊 PTY

- [ ] 測試過程若再次出現 App `SocketTerminalHostClient origin=socket_eof`，同時比對 `e2e_support/logs/alera.log` 與 `e2e_support/terminal_host/logs/runtime.log` 的 UTC 時戳、客戶端身分與 Host 原因（`peer_closed`＝乾淨 EOF、`transport_read_failed`＝讀取失敗）。**不能僅因時戳接近便認定是拆窗造成。**
- [ ] 確認 Terminal Host **程序是否仍在**、經驗證的 read-only `terminal.driver.list` 是否仍列出原 session；過去確曾發生 GUI 報斷線而 Host 和 2 個 session 仍保留的案例。
- [ ] 追查 `terminal_host_socket_isolate.dart`、`terminal_host_client_socket_reader.dart`、`terminal_host_client_connections.dart`、`terminal_host_pty_session.dart` 的 Socket/Reader isolate 生命週期，區分遠端 EOF、讀取錯誤、客戶端主動 dispose、錯誤的 stream onDone 處理與正常 Quit。
- [ ] 以隔離 Host 測試修正 `Reconnect` 路徑：**重新附著既有 session，保留原 PID/PTY/scrollback**，不可只建立新 Shell 來隱藏問題。若可重現，先寫失敗回歸測試再做小幅修正；不得無上限自動重連或輸出憑證與 Terminal 內容至日誌。
- [ ] 必須重複啟動、閒置、開關子視窗和持續輸出，確認連線穩定後才能將這一項標記完成。Rust EOF/讀取失敗的分類測試已通過，不代表偶發 Socket 中斷的根因已修復。

### P1：完成體驗與長時間穩定性驗收

- [ ] 混合 DPI（120/192）雙螢幕：實際拖出、拖回、焦點、鍵鼠命中、視窗位置記憶；`PrintWindow` 和 Win32 物理座標不得混用。
- [ ] Windows 繁體中文注音 IME：候選字組字／選字、Enter、切窗後持續輸入；並測 Ctrl+C、複製貼上、文字選取、捲動、Tab／分割視窗／Ctrl+S 的預期行為。
- [ ] 兩個以上 Terminal 同時獨立視窗、背景持續大量輸出、關窗時仍有輸出；確認不會雙重掛載 FlutterView/GlobalKey、不會意外終止 PTY。
- [ ] 高輸出至少數十分鐘至數小時，重複 30–60 輪以上並記錄 RSS、Handles、GDI、Host 進程與 PTY 狀態。過去 60/60 與 1,200 行的結果是**較早版本**的證據，不能自動當成本次 Socket 修正後的回歸通過。
- [ ] 再用**包含最新 Socket 修正**的 Windows Release AOT 隔離版重做相同 PID 雙 HWND、Terminal 互動、原生關閉及 PTY 返回。過去 Release AOT smoke 成功，但早於最新 Socket 讀取端改動。

### P2：合併與釋出門檻

- [ ] Flutter Terminal/widget/PTY client、Rust CLI、Windows native CTest、Debug GUI 與 Release AOT 的重點測試全部通過；保留 `--alera-legacy-detached-tabs` 退回路徑。
- [ ] 所有新增工具與修正分批提交，確認 `git status --short` 沒有意外檔案；歷史日誌與忽略的 `build/` 截圖不要加入 Git。
- [ ] **在 P0/P1 尚未完成前，不要開啟 Terminal 正式預設，也不要直接合併／push／發布。** 若最後要合併，先只評估預設關閉的 opt-in 實作與主線 dirty 狀態；正式預設需另行通過驗收。

## 3. 已知結果與不能誤判的地方

| 項目 | 已有證據 | 限制／仍待處理 |
|---|---|---|
| Editor 共用 Engine | 現有 Windows 預設 | Terminal opt-in 不得破壞它 |
| Terminal Debug 共用視窗 | 舊版 60/60 native 開關、同 PID 雙 Terminal 子窗 | 不是最新 Socket 修正後的完整驗收 |
| 持續輸出 | 舊版實機 1,200 行與中途交接；回歸測試 1,500 framed output | 尚未做最新版本長時間實機壓測 |
| Windows Release AOT | 舊版成功建置及 1 次拆出／關閉／恢復 | 需重驗最新 Socket 改動 |
| Socket framing | 長度上限、Reader 分類、回歸測試已通過 | `socket_eof` 偶發根因與 UI 重新附著仍未確認 |
| E2E 工具 | 雙 SQLite fixture、自動選取、DPI 座標檢查 | 新 `-NativeMenuClick` 整輪仍待驗證；之前曾誤點 Pin Tab |
| 最後的原生關窗 | 最後一輪出現逾時且之後程序結束 | **不能宣稱 WM_CLOSE+PTY 返回已通過** |

## 4. 重要路徑與接手指令

在 worktree 根目錄：

```powershell
git status --short --branch
git log -5 --oneline
git diff --check
python tool/multiview_poc/test_seed_shared_editor_test_workspace.py
flutter test --no-pub test/unit/terminal_host_client_test.dart `
  test/unit/terminal_host_socket_isolate_test.dart `
  test/unit/terminal_host_frame_codec_test.dart `
  test/unit/shared_engine_detached_window_lifecycle_test.dart `
  test/widget/terminal_surface_test.dart
```

其他相關檔案：

- `tool/multiview_poc/SHARED_TERMINAL_EXPERIMENT.md`：完整歷史、已驗收與未驗收項目。
- `tool/multiview_poc/ALERA_PRODUCTION_INTEGRATION_HANDOFF.md`：Windows E2E App ID、獨立 support 目錄及 native build 注意事項。
- `tool/multiview_poc/probe_alera_gui_windows.ps1`：以明確 `-TargetPid` 驗證原生 HWND 與 DPI。
- `tool/multiview_poc/capture_alera_gui_window.ps1`：擷取**指定 E2E 程序**視窗，不使用全螢幕座標猜測。
- `tool/multiview_poc/probe_alera_gui_input.ps1`：限定 E2E HWND 的 native WM_CLOSE／合成點擊。
- `tool/multiview_poc/probe_isolated_e2e_mouse.ps1`：精確 PID、foreground 與 hit-test 保護的實體滑鼠操作。
- `tool/multiview_poc/probe_real_alera_editor_cycles.ps1`：重複拆窗、WM_CLOSE、資源統計及診斷 JSON。
- `tool/multiview_poc/e2e_terminal_stream.ps1`：僅限隔離 E2E workspace 的持續輸出測試。

**交接完成條件：** 先保留本檔及 3 個未提交腳本；接手者從 **P0-A → P0-B → P0-C → P0-D** 繼續。每完成一小批就測試、提交、更新此表，尚未完成的驗收不得標記為成功。
