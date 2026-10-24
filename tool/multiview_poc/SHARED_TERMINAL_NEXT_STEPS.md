# Alera Windows Terminal 共用 Engine：後續交接與待辦

> 更新：2026-10-24（Asia/Taipei）。**這是接手工作清單，不是完成驗收報告。** 過去的實驗與測試數據見 [SHARED_TERMINAL_EXPERIMENT.md](SHARED_TERMINAL_EXPERIMENT.md)。

## 1. 接手時的基準狀態

- 專案：Alera；隔離 worktree：`.worktrees/exp-shared-terminal-20261023`。
- 分支：`experiment/shared-terminal-view-20261023`；本次整理前 HEAD：`ff78a30cc`。相關已提交修改：`36ae47211`（Rust EOF／讀取失敗分類）、`704971252`（E2E 雙 SQLite fixture）、`22c0e344c`（Socket frame 邊界與失敗診斷）。
- **Terminal 共用 Engine 在 Windows 上預設開啟**（`experimentalSharedTerminalTabs`，`bool.fromEnvironment` 的 `defaultValue: true`）。編譯期仍可用 `--dart-define=ALERA_EXPERIMENTAL_SHARED_TERMINAL_TABS=false` 把 Terminal 留在子程序。`--alera-legacy-detached-tabs` 仍讓 Editor 與 Terminal 都走原本的獨立程序。Git Diff、Git History、PDF、Markdown 即使在預設開啟時也維持子程序。尚未合併、推送或發布。華碩注音的選字、切窗後輸入，以及組字中的 Enter 都有送進 PTY。Enter 是在新 kernel 的 PID 15876 上，su3 之後按住 Enter，PowerShell 執行了「你」並印出字詞無法辨識。P0-C 的 3、10、30 輪 `-NativeMenuClick` 也在這個 PID 通過。
- 2026-10-24，隔離 Debug PID 76760，kernel `kernel_blob.bin` UTC 2026-10-23 15:51。120 DPI 子視窗的返回標題有畫出來，白字在 AppBar（bitmap 約 x=30-124、y=66-80）。先前拖回沒有開始，是拖到終端機本體，不是標題沒畫。從那個標題拖回主視窗後只剩一個 HWND；拖到非 Alera 視窗上放開，子視窗保持開啟。192 DPI 從較低的標題群拖回，以及對子視窗送 `WM_CLOSE`，都回到單一主視窗，第二個終端分頁仍在。30 次拖出再 `WM_CLOSE` 全部返回：RSS 約 644-659 MB、Handles 約 1087-1092、GDI 維持 38。結束時 `echo CYCLE_PTY_OK` 有印出。掃描碼 Ctrl+C 中斷了 ping，貼上出現 `PASTE_ALERA_OK`，選取改變了該字的像素，滾輪把較早的輸出捲回畫面。注音第一次沒有送出漢字：華碩注音停在英文模式，拼字小圖示有出現，Ctrl+Space 沒有切到注音，按鍵以 ASCII 進 PTY。同一 kernel（`kernel_blob.bin` UTC 2026-10-23 15:51，182404816 bytes）在隔離 Debug PID 51140 重測：左 Shift 切到中文模式後，掃描碼 s u 3 與空白在提示字元後送出「你」。Ctrl+C 中斷該行時殼層印出「你 ^C」，所以這個字進了 PTY，不是只畫在組字層。轉換模式設回測前的值之後，s 又回到 ASCII。HKL 維持 0x04040404。沒有改產品程式。Debug 啟動後 12 秒（2026-10-23T16:44:25Z）與 Release 選到工作區後約 30 秒（2026-10-23T17:39:09Z）各有一次 `socket_eof` / `peer_closed`。兩次之後 PTY 都還能印出指令，沒有停在 host-unavailable，起因沒有認定。Release AOT 是 `flutter build windows --release` 的 `build/windows/x64/runner/Release/alera-dev.exe`（`data/app.so`，dev flavor）。PID 22144 把終端拖成同 PID 的 `Alera - E2E Terminal`，`WM_CLOSE` 後只剩一個 HWND，`echo RELEASE_PTY_OK` 有印出。Debug PID 76760 與 Release PID 22144 都已用強制停止結束，沒有動到其他 Alera 程序。
- 最後檢查時，隔離 GUI PID `7464` **已不在執行**，`e2e_support/terminal_host/host.json` 不存在。PID 與點擊座標僅代表舊測試，重新啟動後必須重新取得，**不可沿用 PID**。
- **有 3 個尚未提交的工作檔，請保留，不要 reset/clean：**
  1. `tool/multiview_poc/build_alera_in_worktree_debug.cmd`：容許用 `ALERA_E2E_REUSE_CLI=0` 強制重建 Rust sidecar；平常仍可重用。
  2. `tool/multiview_poc/probe_isolated_e2e_mouse.ps1`：新增 `-PreflightOnly`，改為在 Per-Monitor V2 DPI context 下將畫面邏輯座標換算成螢幕實體座標，且檢查視窗歸屬。
  3. `tool/multiview_poc/probe_real_alera_editor_cycles.ps1`：Terminal 新增 `-NativeMenuClick`，交由上述工具點選選單。2026-10-24 在 PID 15876、kernel UTC 2026-10-23T19:06:05Z，以 `-TabX 510 -TabY 50 -MenuX 335 -MenuY 200` 跑完 3/3、10/10、30/30。30 輪結束 RSS 633.1 MiB、Handles 1097、GDI 28。之後 `echo cycleptyalive` 有印出。
- 2026-10-24 重新產出的隔離 Debug kernel：`data/flutter_assets/kernel_blob.bin` UTC 2026-10-23T19:06:05Z，182437328 bytes，晚於 xterm 的 Enter 修正。不要只看 EXE 的修改時間。同一 kernel 的隔離 Debug PID 40592 重測了 Ctrl+C、文字選取與滾輪，三項都進了 PTY 畫面。同一 kernel 的隔離 Debug PID 11928 另測了貼上、Ctrl+Tab、分割、Ctrl+S，以及 120 DPI 子視窗的焦點和鍵盤。貼上重試後 `echo debugpasteok` 印出 `debugpasteok`。Ctrl+Tab 換成另一個終端分頁。Ctrl+Shift+D 拆成左右兩個終端，Ctrl+Shift+W 再合回。Ctrl+S 之後終端沒有停住，接著的 `echo slot` 印出 `slot`。120 DPI 子視窗 `Alera - Terminal 2` 取得前景，`echo pong` 印出 `pong`。視窗位置沒有照存檔原樣恢復。

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
- [x] 以新的 `-NativeMenuClick` 跑 **1 個循環**，確認原生子視窗 `Alera - E2E Terminal` 確實出現在**相同 PID**、子窗終端機互動正常；對**這個子窗**送 `WM_CLOSE`，主窗應恢復為**單一 HWND**，原 shell session ID、先前 scrollback 與新輸入指令都應正常。以真正 PTY 命令（例如 `echo RETURN_PTY_ALIVE`）確認互動，而非只有 HWND 計數。2026-10-24，PID 22456，`-TabX 510 -TabY 50 -MenuX 335 -MenuY 200`，`GESTURE_ATTEMPTS=1`，`WM_CLOSE` 後只剩 `Alera Dev`。報告 `goal_e2e_terminal_cycle.json`。子視窗開啟的約 1.4 秒內沒有另外打字。返回後 scrollback 仍有「好 ^C」，英文模式下 `echo returnptyalive` 有印出。
- [x] 此 1 輪通過後再漸進擴大 3 → 10 → 30 次循環，記錄每輪成功數、開／關 P95、RSS／Handles／GDI。2026-10-24，PID 15876，kernel UTC 2026-10-23T19:06:05Z，`-TabX 510 -TabY 50 -MenuX 335 -MenuY 200`。3/3 結束 RSS 618.5 MiB、Handles 1092、GDI 28。10/10 結束 RSS 621.5、Handles 1089、GDI 28，開 P95 1681 ms、關 P95 66 ms。30/30 結束 RSS 633.1、Handles 1097、GDI 28，開 P95 1663 ms、關 P95 65 ms。報告 `goal_e2e_cycles_3.json`、`goal_e2e_cycles_10.json`、`goal_e2e_cycles_30.json`。30 輪之後 `echo cycleptyalive` 印出 `cycleptyalive`。這幾輪沒有新的 `socket_eof`。
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
- 2026-10-24 凍結恢復實測（`host_freeze_resume.ps1` NtSuspendProcess，Host PID 57404、隔離 Debug PID 34916）：凍結後鍵入觸發 request timeout → 15s 心跳 ×3 失敗 → `terminal host transport disconnected origin=heartbeat_exhausted`（app 日誌 UTC 05:29:14），偵測總延遲約 90 秒。**注意**：心跳只在有 request 超時後才啟動，完全無流量的卡死 Host 不會主動偵測（設計如此，驗收時要接受這個延遲）。斷線後 app 立即重連；凍結期間約 90 秒內產生 **60+ 條 TCP 連線**（`_connectToPublishedControl` ~100ms 間隔重試，TCP 由 backlog 完成握手、hello 等不到、2s auth 超時即丟），Host 解凍瞬間需一次處理 84 筆 client cleanup——**重連風暴無退避/上限**——已在 `terminal_host_client.dart` 修正：connect 失敗採連續失敗計數退避（第 1 次失敗仍可立即重試，第 2 次起 `_connectRetryCooldown` 2s 遞增至 ×4 上限，語意性 `StateError` 不計入），回歸測試 `connect failures pace the next attempt instead of storming` 通過。**新 kernel 複測已做（UTC 2026-10-24，kernel_blob 05:57:00Z/182747976B，隔離 Debug PID 59572，Host PID 57404）**：凍結 host → 送鍵觸發 request → 06:02:51 `heartbeat_exhausted` 斷線 → 凍結期間 ~82 秒產生 ~44 條 TCP 嘗試，但呈**爆發-平台相間**模式（每 ~2s 一簇 1-2 條，隨失敗計數上升出現 19-21s 平台）——cooldown 退避生效、不再連續轟炸；解凍後 conn 10085 穩定建立、session=true 寫入流通、Host 無新 PTY bootstrap、兩個 durable PTY（57008/78732）全程存活，`sessions_detached=1`（session 停泊待 reattach 而非終止）。**注意**：~44 條仍偏多——每個 cooldown 窗口可讓 terminal 與 runtime 兩條獨立 future 各開一條 socket，且前期 streak 小時退避較短；穩態約每 ~10s 一個嘗試窗口。`sessions_detached=0`，Host 兩個 PTY bootstrap（57008/78732）全程未重建。凍結中 session 的 `start()` 撞到 `authentication timed out` → 分頁進入 Terminal unavailable +「重新連線」按鈕；**合成點擊（WindowFromPoint 驗證落在 FLUTTERVIEW、SetCursorPos+mouse_event、SendInput 絕對座標、Tab+Space）皆未觸發按鈕**，需人工或測試鉤子驗證。強制重啟 app（PID 34680）後還原工作區：新連線 port 14018，Host 無新 PTY bootstrap 產生 → **確認重新附著同一 durable session**，終端可正常打字回顯；舊 conn 8559 的 client_id 85 cleanup 顯示 `authenticated:true`（解凍後確已完成驗證）。 **續測（同日 UTC ~14:25，仍 PID 59572 + Host 57404）**：前記合成點擊失敗是座標系混用——SetCursorPos/mouse_event 走系統虛擬化座標，而截圖與 SetThreadDpiAwarenessContext(-4) 的 WindowFromPoint 是實體像素（view 實際 rect 93,138-1767,1167）。改用 PostMessage WM_LBUTTONDOWN/UP 直送 view 客戶座標 (860,590) 即成功觸發「重新連線」：終端立即恢復 PS e2e_workspace 提示符、可打字、命令可執行（殘留注音/掃碼注入造成亂碼屬注入雜訊非傳輸問題）。PTY PID 57008/78732 全程未變、runtime.log 無新 bootstrap、工作目錄同前 → reattach 同一 durable session 成立，Reconnect 路徑功能正確。**新發現 widget 層 bug（可能與第一字元問題同源）**：CustomTextEdit.updateEditingValue 把 value.text 全文當插入送出後才用 _connection.setEditingState(空) 重置平台狀態；當字元在重置落地前連續到達（PostMessage WM_CHAR 連發），平台在舊值上 append，下一字元的 delta 含累積全文而重送。input_trace 實證：p,w 各乾淨，d 觸發 onInsert "wd" 重送 [119,100]；echo rdy123 → ecchchocho+cho rcho rdcho...，最終命令行收到 pwwd（w 重複）。前兩字元乾淨、第三字元起累積——主視窗終端也中，非子視窗限定。另 SendInput 掃碼 0x08 被 Flutter 映成 Select 鍵（合成掃碼路徑不可靠）。 **已修正並真機驗證（kernel UTC 2026-10-24 重建，隔離 Debug PID 24388）**：custom_text_edit.dart 的 updateEditingValue 改為對前次平台文字做前綴差分——newText 與 previousText 不同且以其為前綴時只送後綴（平台重置未落地造成累積的情境）；previousText 以 newText 為前綴（平台緩衝變短，如重置回聲）時不送；其餘（全新值、同值重打、整段取代）送全文。回歸測試 "updates that still carry consumed text only emit their new suffix" 通過，xterm 全檔 877 測試僅 2 個既有 textScaler 失敗。真機複測：重啟 app（PID 24388）reattach 後 terminal 保留全部舊 scrollback；同樣 PostMessage WM_CHAR 連發 echo fixok → 乾淨執行印出 fixok，input_trace 顯示每字元只送一次（對照修正前 onInsert "cho rdy123" 全文重送）。殘留小角落：重置已落地且多字元被平台合併成 一個恰巧以舊已消費文字為前綴的 update 時會只吃後綴——機率極低，若要根除需改用 enableDeltaModel（平台端算 delta）。第一字元消失是另一個機制（連線 attach 前 WM_CHAR 被 embedder 丟棄），本次未處理。

### P1：完成體驗與長時間穩定性驗收

- [ ] 混合 DPI（120/192）雙螢幕：實際拖出、拖回、焦點、鍵鼠命中、視窗位置記憶；`PrintWindow` 和 Win32 物理座標不得混用。標題是否畫出來、拖回主視窗、以及在非 Alera 視窗上放開，這三項已在 kernel UTC 2026-10-23T19:06:05Z 的 PID 15876 通過，所以不再是擋預設的缺口。這格先不勾。焦點和子視窗鍵盤已在 PID 11928 通過，視窗位置沒有原樣恢復。192 DPI 主視窗 (40,40) 1700x1100。同 DPI 子視窗在 (1900,40)，AppBar 標題群 y=103-125，從 bitmap (143,115) 拖到 (900,600) 後只剩 `Alera Dev`，第二個終端分頁還在，scrollback 仍有 `cycleptyalive`。120 DPI 子視窗在 (-1700,40) 1500x900，`GetDpiForWindow=120`，標題白字在 y=67-80；第一次拖到終端文字沒有收回，第二次從 bitmap (92,74) 拖到主視窗後只剩一個 HWND。接著在 192 DPI 把標題拖到 PID 23468 的視窗上放開，子視窗保持開啟，再送 `WM_CLOSE` 才回到單一主視窗。截圖在工作階段暫存：`dpi-same-title-mark.png`、`dpi-same-returned.png`、`dpi-mixed-title-visible.png`、`dpi-mixed-title-mark.png`、`dpi-mixed-returned.png`、`dpi-off.txt`。2026-10-24，PID 11928：子視窗 `Alera - Terminal 2` 移到 (-1700, 40) 1500x900，`GetDpiForWindow=120`，前景標題是 `Alera - Terminal 2`。第一次按鍵少了開頭的 e，重試 `echo pong` 印出 `pong`，同時主視窗畫面沒有這行。截圖 `child-keys-retry.png`、`main-while-child.png`。關掉再拆出後，新子視窗在 (-1616, 90)，120 DPI，寬 1312、高 900，不是上次的位置和寬度。啟動時 `app_window_state_table` 仍是 left 20, top 20, width 850, height 550，主視窗實際開在 (1070, 482)、1700x1100。根因是 `runMultiApp` 在還原之後套用預設 `Alignment.center`，把已還原的原點改回工作區中央、尺寸不變。`sharedEngineMainWindowOptions` 改為 `alignment: null` 後，kernel UTC 2026-10-23T20:57:44Z（182343432 bytes）的隔離 Debug PID 69424 把 sqlite 種成邏輯 (40, 40) 850x550、`updated_at` 1792788200，兩次取樣主視窗都是實體 (80, 80)-(1780, 1180)、192 DPI，啟動沒有改寫該列。子視窗位置另有缺口：原生標題拖曳常常沒有 `moved`，一般 `WM_CLOSE` 也不發 `close`，所以記憶是空的，重開又回到置中。現在在子視窗還開著的時候每 250ms 讀一次實體邊界，並多聽 `move`/`resize`。同一 PID：第一個子視窗在 (870, 312) 2100x1440，設成 (1900, 80) 1400x900 後 `WM_CLOSE`，重開仍是 (1900, 80)-(3300, 980)。再設到 120 DPI 的 (-1600, 60) 1100x700，`WM_CLOSE` 後重開是 `Alera - Terminal 2`、DPI 120、RECT=-1600,60--500,760，再取樣一次相同。120 DPI 子視窗的 Flutter 標題在 bitmap (81, 74)，從螢幕 (-1519, 134) 拖到 (900, 600) 後只剩 `Alera Dev`。2026-10-24 另開隔離 Debug PID 46892，同一顆 kernel。子視窗 `Alera - Terminal 1` 放在 (-1600, 40) 1200x800，`GetDpiForWindow=120`，前景 HWND 就是該子視窗。點進終端後第一次打字變成 `cho dpi120key`，開頭的 e 不見。主視窗當時是另一個終端的空提示，沒有這行。截圖 `dpi120-type-46892.png`、`dpi120-main-46892.png`。焦點有到子視窗，鍵盤第一個字元沒有完整送出，所以這格仍不勾。2026-10-24 隔離 Debug PID 14404，同一顆 kernel，沒有換 DPI。主視窗裡已經開著的終端，第一次打字是完整的 `echo imecheck14404`。把 Terminal 1 拆到同一 PID 的 `Alera - Terminal 1`（870,312-2970,1752，192 DPI）後，點進子視窗的第一次打字是 `cho childchar14404`。主視窗沒有這行。第一個字元遺失不是 120 DPI 才有，新的子視窗也會掉。截圖 `ime-typed-14404.png`、`child-typed-14404.png`、`main-during-child-14404.png`。文字輸入客戶端原本在建構時固定用 implicit view。現在 `TerminalView` 把正在組裝的 view id 傳進 `CustomTextEdit`，view 變了會重開連線。`text input follows the view that builds the terminal` 與 Windows Enter 組字測試通過。新 kernel UTC 2026-10-23T21:56:31Z（182450552 bytes）的隔離 Debug PID 60664 把 Terminal 1 拆成同一 PID、192 DPI 的 `Alera - Terminal 1` 後，第一次打字仍是 `cho viewfix60664`。改 view id 沒有讓第一個字元留下來。截圖 `viewfix-child-60664.png`。這格仍不勾。2026-10-24 attach gap 定案並真機通過。冷探針（probe_child_first_char_pm.ps1，每 ~10ms 輪詢新可見 toplevel）在舊 kernel 上決定性重現：FLUTTERVIEW HWND 出現後 4ms 內 PostMessage 的 13 個 WM_CHAR 在 TextInput.attach 送達前被 embedder 靜默丟棄（trace 只有 open attached=null，全程無 edit/onInsert）。修法：shared_engine_detached_tab_window.dart 改傳 WindowOptions(showOnInit:false)（子窗原生建立但隱藏），viewId 改從 builder 擷取，body 包 EarlyInputAttachScope——CustomTextEdit mount 時即 TextInput.attach 不等 OS focus；reportContentMounted 後才 completeNativeShow(viewId) 放行顯示。setClient 平台訊息 FIFO 保證先於 FFI 直送的 ShowWindow 落地，gap 歸零。另加 per-view _attachedOwners：focused client 被 eager attach 偷走 client 時能 reclaim（非陳舊 boolean）。失敗路徑先釋放 show waiter 再清理，不洩漏 openWindow future。生命週期測試檔 15/15、xterm 新增 eager-attach 與 reclaim 兩測試通過。另修 updateEditingValue 前綴縮短誤判：上一輪把「平台緩衝變短」一律當重置回聲靜默，實測發現 reset 已落地後的新輸入也是前綴縮短（殘留 "echo attok" 後打 "e"）會被吞掉首字——改為前綴縮短照送全文，回歸測試補此案例。真機複測（kernel UTC ~07:35、隔離 Debug PID 50776、Host 57404）：探針在子視窗變可見後 ~13ms 送完 echo attok2，trace 顯示 open requestedView=2 → focusChange attached=2 → 每字元 edit→onInsert→textOut→pty units 全鏈路（session=true），中途平台 reset 落地時 "echo at"→"t" 前綴縮短亦正確放出新輸入——無 keyDrop/textDrop。Enter 後命令確有執行（錯誤訊息第一 token 為舊殘留 劃從誒親喔今年初誒額cho，attok2 作為其後 token——durable PSReadLine 緩衝跨 app 實例保留所致，非丟字）。暖路徑 echo zz9 乾淨印出 zz9。WM_CLOSE 子視窗合回主視窗、scrollback 完整。第一字元 attach gap 機制就此關閉；子視窗第一字元在 120/192 DPI 混測仍需在新 kernel 各複測一次才勾。
- [x] Windows 繁體中文注音 IME：候選字組字／選字、Enter、切窗後持續輸入；並測 Ctrl+C、複製貼上、文字選取、捲動、Tab／分割視窗／Ctrl+S。注音這幾項已通過，所以不再是擋預設的缺口，Tab、分割和 Ctrl+S 已在 PID 11928 測過，當時不勾，是因為 Ctrl+S 沒有對到編輯器存檔，而且幾次第一個字元被吃掉。這兩項已在 PID 69424 通過，見本段結尾。2026-10-24，PID 22456：左 Shift 在 Alera 具有前景時從英文模式切到注音/英文混合模式；候選清單第 2 項 support 送進 PTY（`support^C`）；切到檔案總管再回來後「好」送進 PTY（「好 ^C」）。組字中的 Enter 在舊 kernel 只把「你」留在提示列。xterm 改為組字期間看到 VK_RETURN 仍按著時，在組字結束後補送一次 Enter。Widget 測試通過後，PID 15876（kernel UTC 2026-10-23T19:06:05Z）對開啟中的 su3 按住 Enter，PowerShell 執行了「你」並印出字詞無法辨識。測完已切回英文模式，HKL 仍是 0x04040404。貼上是同一版 Release PID 40084 的 `RELEASEPASTEOK`。Ctrl+C、選取、捲動已在這個 kernel 的隔離 Debug PID 40592 重測：`ping -n 8 127.0.0.1` 在兩次 Reply 後被 Ctrl+C 停住並印出 Control-C；滾輪把畫面從 `SCROLLLINE38` 附近捲到 `SCROLLLINE09`；拖曳讓 `SCROLLLINE43` 出現選取底色，`SELECT_CHANGED_SAMPLES=1374`。截圖在工作階段暫存：`interrupt-during.png`、`interrupt-after.png`、`scroll-before.png`、`scroll-after.png`、`select-before.png`、`select-after.png`。PID 11928：第一次貼上變成 `cho debugpasteok`，重試貼上 `echo debugpasteok` 後殼層印出 `debugpasteok`。Ctrl+Tab 之後，分頁內容從有 `SCROLLLINE42` 的終端換成另一個終端的提示字元。Ctrl+Shift+D 出現左右兩個 PowerShell，Ctrl+Shift+W 合回單一窗格。Ctrl+S 當下作用中的是終端分頁，接著 `echo slot` 印出 `slot`，終端沒有停住。截圖：`paste-after.png`、`tab-before.png`、`tab-after-body.png`、`split-after.png`、`unsplit-after.png`。2026-10-24，同一顆 kernel UTC 2026-10-23T20:57:44Z 的 PID 69424：放開修飾鍵並等待後，第一次貼上就是 `echo firstcharok`，Enter 印出 `firstcharok`。重新點進終端後第一次打字就是 `echo typedok`，開頭的 e 都在。Quick Open 打開 `handoff_test.dart`，游標處插入 `//pid69424` 後磁碟仍是 2026-10-23T08:48:19Z、138 bytes；Ctrl+S 之後變成 2026-10-23T21:05:20Z、148 bytes，內容是 `// ALERA_E2E_//pid69424ORIGINAL_DO_NOT_EDIT_OUTSIDE_TEST`。Tab 和分割沒有在這顆 kernel 上重做，沿用 PID 11928。截圖：`paste-once-69424.png`、`paste-enter-69424.png`、`type-once-69424.png`、`editor-saved-69424.png`。
- [ ] 兩個以上 Terminal 同時獨立視窗、背景持續大量輸出、關窗時仍有輸出；確認不會雙重掛載 FlutterView/GlobalKey、不會意外終止 PTY。
- [ ] 高輸出至少數十分鐘至數小時，重複 30–60 輪以上並記錄 RSS、Handles、GDI、Host 進程與 PTY 狀態。過去 60/60 與 1,200 行的結果是**較早版本**的證據，不能自動當成本次 Socket 修正後的回歸通過。
- [x] 再用**包含最新 Socket 修正**的 Windows Release AOT 隔離版重做相同 PID 雙 HWND、Terminal 互動、原生關閉及 PTY 返回。2026-10-24，`build/windows/shared_release_ninja/runner/Release/alera-dev.exe`，`data/app.so` UTC 2026-10-23T19:32:14Z，28115848 bytes，PID 40084。選單 Move to New Window 後同一 PID 出現 `Alera - E2E Terminal`。`WM_CLOSE` 後只剩 `Alera Dev`。拆出時 RSS 196.5 MiB、Handles 1064、GDI 26；返回後 RSS 197.3、Handles 1068、GDI 26。`echo releaseptyalive` 印出 `releaseptyalive`，Ctrl+V 貼上 `RELEASEPASTEOK`。這次 Release 日誌沒有 `socket_eof`。這不是 30 輪，也沒有在這份 AOT 上重測混合 DPI。2026-10-24 另編了包含 bounds poll 的 Release AOT：`data/app.so` UTC 2026-10-23T21:20:40Z，28148616 bytes。隔離 PID 41412 用選單 Move to New Window，同一 PID 出現 `Alera - E2E Terminal`（870,312-2970,1752，192 DPI）。`WM_CLOSE` 後只剩 `Alera Dev`（40,40-1740,1140）。拆出時 RSS 201.5 MiB、Handles 1077、GDI 24；返回後 RSS 194.5、Handles 1067、GDI 24。分頁回到原提示字元。接著打 `echo releaseptyalive`，畫面是 `cho releaseptyalive`，Enter 後 PowerShell 回報無法辨識 `cho`。開頭的 e 不見，所以這份 AOT 不能記成乾淨的 `echo releaseptyalive`。截圖 `release-returned-41412.png`、`release-typed-41412.png`、`release-enter-41412.png`。這次 app 日誌只有選到工作區。Host 在 2026-10-23T21:31:38Z 以 `host_shutdown` 結束，`sessions_detached` 1，沒有 `socket_eof`。混合 DPI、P0-D、soak、多視窗仍不勾。

### P2：合併與釋出門檻

- [ ] Flutter Terminal/widget/PTY client、Rust CLI、Windows native CTest、Debug GUI 與 Release AOT 的重點測試全部通過；保留 `--alera-legacy-detached-tabs` 退回路徑。
- [ ] 所有新增工具與修正分批提交，確認 `git status --short` 沒有意外檔案；歷史日誌與忽略的 `build/` 截圖不要加入 Git。
- [ ] **不要直接合併／push／發布。** Windows 編譯期預設已改為共用 Terminal 視窗；下方未勾選的 P0/P1 項目仍是開放風險，不是已通過的驗收。`--alera-legacy-detached-tabs` 必須留著。

## 3. 已知結果與不能誤判的地方

| 項目 | 已有證據 | 限制／仍待處理 |
|---|---|---|
| Editor 共用 Engine | 現有 Windows 預設 | Terminal opt-in 不得破壞它 |
| Terminal Debug 共用視窗 | 舊版 60/60 native 開關、同 PID 雙 Terminal 子窗 | 不是最新 Socket 修正後的完整驗收 |
| 持續輸出 | 舊版實機 1,200 行與中途交接；回歸測試 1,500 framed output | 尚未做最新版本長時間實機壓測 |
| Windows Release AOT | PID 40084 同程序拆出、`WM_CLOSE` 返回，`echo releaseptyalive` 與貼上 `RELEASEPASTEOK`。較新的 `app.so` UTC 2026-10-23T21:20:40Z、PID 41412 也拆出並 `WM_CLOSE` 回到單一主視窗，殼層有執行輸入 | 不是 30 輪；沒有重測混合 DPI。PID 41412 的第一個字元被吃掉，打出的是 `cho releaseptyalive`，不能當成乾淨 echo |
| Socket framing | 長度上限、Reader 分類、回歸測試已通過 | `socket_eof` 偶發根因與 UI 重新附著仍未確認 |
| E2E 工具 | PID 15876 的 3/3、10/10、30/30 `-NativeMenuClick` 通過 | 之前曾誤點 Pin Tab；逾時分類沒有新案例 |
| 最後的原生關窗 | PID 15876 的 30/30 `WM_CLOSE` 都回到主視窗，結束 RSS 633.1 MiB、Handles 1097、GDI 28，`echo cycleptyalive` 有印出。同一版 Release PID 40084 也以 `WM_CLOSE` 返回 | Debug 子視窗開啟的約 1.4 秒內沒有另外打字 |

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
