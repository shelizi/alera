# GPUI Terminal：Classic Host 持久輸出串流（TDD 階段）

日期：2026-11-09，實驗整合分支 `integrate/gpui-p0p1p2-20261108`。

## 改造重點

原來 Native 每次按鍵都啟動一次 `alera terminal write` CLI，輸出由 `terminal.read` 在 100／350／1500 ms 的輪詢間隔抓取。Classic Flutter 使用已認證的 Host Socket 接收真正的 PTY output event，所以本輪將推播介面補到 Native；不複製 PTY，不啟動／重新啟動 Host。

資料流：

```text
GPUI Workbench／VtScreen
    → NativeTerminalStreamBridge（一次 Native 窗口一個 CLI 子程序）
    → 私有、有界 NDJSON stdin/stdout
    → alera terminal --no-start stream（保持既有 Host 認證長連線）
    → Runtime Host：terminal.observeExisting（僅現存 PTY）
                     terminal.writeObserved（每筆輸入核對 Lease/Workspace/Tab）
    ← Host output／outputResyncRequired／exit（Host 主動推播）
```

## 相容與安全

- **不能建立 Shell**：`terminal.observeExisting` 僅接受現存 running Session，拒絕不存在、已停止、已轉移或錯誤的 workspace/tab，必須提供視窗專屬 `viewLeaseId`。共用原 `createOrAttach` 中 Host output lane + delta / snapshot 機制，但不提供 `launch`、`create`、`restart` 入口。
- **每鍵身份核對**：`terminal.writeObserved` 在 Host queue 入口即驗證原已 attach 的 client `viewLeaseId`、workspace/tab/session、running，並沿用 PTY 非同步寫入佇列；即使移到其他 Worktree 也不能寫錯對象。
- **Classic 自動化接手機制**：常駐 CLI stream 對 Host 使用既有 `clientKind=app` 認證方式；Native `observeExisting` 與 Classic `createOrAttach` 共用 automation-owned Tab 的 durable takeover 標記，防止使用者正在觀看／輸入的 Agent Terminal 被 automation cleanup 回收。
- **憑證隔離**：GPUI 不讀 Host control-file token；原 `RuntimeHostRpcClient` 在 CLI 內持有認證長連線。私有 Native IPC 僅允許 `observe` 與 `write`、輸入最多 4096 bytes、待處理 RPC 最多 32，明確禁止建立、終止 Host 或 Session。
- **輸出恢復**：Host 先送 full snapshot，再轉為 push output。Native 依 session/workspace/tab 三重身分派送，Host event 可早於 ACK 的情況先緩存（最多 256KiB），UI 一個 tick 以 4–64KiB 自適應預算處理。UTF-8 跨幀解碼，游標以 raw bytes 計。
- **原始畫面尺寸**：Host attachment 的 `snapshotRows`／`snapshotCols` 必須隨 Native 分批回放保留，先將 VT 畫面設為原始 Host PTY 尺寸，再讀入 Snapshot 的絕對游標與換行；**僅改 Native 顯示，不改 Host PTY 的 driver／cols／rows**。
- **畫面喚醒速度**：有可視的 Native Host stream 時單一 Workbench timer 每 16ms 消耗推播事件；背景／尚未連線時仍為 50ms，舊 CLI read 仍依 100／350／1500ms 節奏。這只是排程上限，非聲稱已量到 p95 實機輸入回顯。
- **故障降級**：舊版 CLI 或 Host 沒有新增命令、Socket 斷線、lease 被拒、resync、無效 output 時，結束**只有私有 CLI stream**，Host PTY 保持；Native 使下一次 CLI read 從 Host 的 retained scrollback 重新建立 VT，而不是把片段重複套用。每次 Native Preview Tab 重新建立／身份 generation 變更也重新取得 snapshot。
- **與舊版共存**：原 Flutter `SocketTerminalHostClient` API 及既有 Host Session/driver 保留；此路徑是有舊版 CLI fallback 的過渡方案。CLI→GPUI 為 NDJSON/base64 output（尚未升級 Host binary frame 模式），不可誤稱零 copy。

## 測試範圍與門檻

- Host：`terminal_contract_tests.rs` 新增 observe 失敗不 remint、正確 lease/identity、同 output lane delta 恢復、寫入每鍵授權拒絕與真正 writer 邊界。
- CLI：`terminal_stream.rs` 測試只允許明確的 Native IPC 操作及 per-session event 轉發、`cli_tests.rs` 測試 `terminal stream --no-start`。
- IPC 僅把白名單欄位送到 Host；即使私有 stdin 收到額外的 `launch`、`bracketedPaste`、`deferredEnter` 也不得帶到 Host。真正的鍵盤資料只進 stdin，不走命令列參數。
- Native：`terminal_stream_bridge.rs` 測試解析 Host JSON output/拒絕非法字串、初始 ACK 前 output 的有界排隊、拒絕過期訂閱；`terminal.rs` 測試中文 UTF8 幀邊界、Host workspace/tab 三重身分及 fallback cursor/full snapshot。
- **不能視為已通過**：需要完成 isolated Runtime Host / CLI / GPUI 真實往返測試、Windows Native UI 量測 p50/p95 回顯、TUI 長輸出、畫面刷新／視窗/Tab 切換，以及 macOS/IME 驗收。Native 繼續 `supported=false`。

## 環境與部署注意

Native 與 CLI 必須一起編譯並放在同一套預覽執行檔位置；目前執行中的 Windows Native 預覽尚未換成此版本，舊的 `alera.exe` 不支援新 `stream` 時會回退輪詢。這輪不終止現有 Host、TTY/PTY，不合併 main、不推送也不發布。

## 第一批驗證紀錄

- **Rust CLI／Host 靜態編譯（通過）**：`cargo check --manifest-path rust/Cargo.toml -p alera-cli --tests --offline --locked`。Host 新增的 observe／write／automation takeover 契約測試均可通過型別檢查。
- **Host 真正執行測試（未完成）**：Windows `cargo test -p alera-cli observe_existing_terminal` 在整體 CLI test binary 連結階段運作超過五分鐘，未取得單元測試結果；僅終止該工具管理的測試工作，未中斷實際 Host／PTY。編譯成功不能替代 Host 執行測試。
- **GPUI Rust 單元測試（通過）**：串流事件解析、訂閱 ACK 前輸出順序、錯誤 lease、不經 CLI 命令列傳送文字的確認；另有 UTF-8 分段接收與 Host workspace/tab 身分檢查、串流斷線後 CLI retained scrollback 重建。
- **Windows GPUI 原生編譯（通過）**：`cargo check --manifest-path desktop_gpui/Cargo.toml --offline --locked`，無需啟動 GUI 或觸碰執行中的 Host。
- 原始 Host→CLI 的隔離寫入／推播往返已在下段取得 6/6 真實執行結果；**仍欠**大量輸出 backpressure／重連整合壓測、新 Native executable 的實際鍵盤、IME、TUI、畫面以及 p50/p95 延遲測試。CLI Sidecar 尚未搭配 Native 預覽程式更新部署。

## 2026-11-09：隔離 Host／CLI 端對端 Smoke

新增可重複執行的 `desktop_gpui/tests/terminal_stream_isolated_smoke.py`。它每次都建立隨機 TemporaryDirectory、獨立 Runtime Host 控制檔、測試 Project／Workspace／PTY；只使用 CLI 的指定 `--runtime-dir`，不接觸使用者原有 Runtime/PTY。測試結束後向**該測試 Host**送出 terminate/shutdown 並回收子程序。

測試指令：`python desktop_gpui/tests/terminal_stream_isolated_smoke.py PATH_TO_NEW_ALERA_EXE`

實際完成結果：**PASS 6/6 isolated Host/CLI stream contracts**：

1. 隔離 Host 真正建立新 PTY；
2. `observeExisting` 正確只附著現有 PTY，`created=false`；
3. 錯誤 `viewLeaseId` 的原生輸入被 Host 拒絕；
4. 正確輸入完成 Host PTY write ACK，且從持久 Host Socket 收到實際 PTY 的 output event；
5. 對不存在的 Session 執行觀察會被拒絕、不會重新建立 Shell；
6. 關閉 CLI 長連線後，該隔離 Host 的 PTY 仍可透過普通 CLI read 證明運作。

Windows 編譯注意：原正式 `alera.exe` 正被其他程序使用而無法覆寫。Smoke 為了**不關閉既有程式**，暫時以另一個 Cargo bin 名稱編譯，並明確套用正式 Host 的 `/STACK:8388608` 設定；驗證後已還原 Cargo manifest，並未改變正式建置。原先未套用該旗標時臨時 Host 的 1MiB 預設堆疊溢位，非正式 `alera` 的回歸。

上述 6/6 是真實 Host 與 CLI 的隔離整合測試，**不代表** GPUI 視窗的視覺/IME、長輸出 TUI、實機延遲或所有 Rust Host unit tests 均已通過。Native `supported=false` 維持不變。

Native 進一步執行 `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --offline --locked terminal`：**77/77 通過**（含 View Lease 封包、Host Snapshot 尺寸、中文分塊、錯誤恢復、雙分割 pane、16ms/50ms 喚醒條件）。Windows 原生 `cargo check` 也通過。實際 UI 尺寸及延遲仍待機器驗收。

## 2026-11-09：真正 Windows GPUI 視窗輸入驗證

已使用隔離的 GPUI 執行檔／Host／CLI／Runtime／Native State／HOME 完成實際視窗煙霧測試。CLI 配對採用 `ALERA_CLI_PATH`，Runtime 使用 `ALERA_RUNTIME_DIR`，Native 狀態使用 `ALERA_GPUI_STATE_DIR`，全都指向新的臨時目錄，不覆寫 Classic／使用者 Host；測試視窗結束時只回收自己的子程序。既有 Smoke 指令可加 `--native-app PATH_TO_GPUI_EXE --hold-seconds 40`，以臨時 Terminal 佈局在視窗開啟後顯示實際的 Windows Shell。

實機找到並修正兩個只有單元測試無法發現的問題：

1. 原 `QuickOpenInput` 把**所有純 ASCII TextInput Commit 丟棄**，以為文字一定從 KeyDown 到 Host，造成文字注入／貼上類輸入遺失。新 `TerminalTextCommitGate` 只對**真的送達 KeyDown 輸入佇列**的最近 ASCII 做去重，其餘 ASCII／Unicode Commit 照常送入 Host；gate 在 Tab／Workspace 清空時重置。
2. 終端內容列選取與 GPUI 的滑鼠預設聚焦可能覆蓋 1px 隱藏 IME 輸入元件焦點。現在在彩色 VT 與純文字 Transcript 的 MouseUp 後重新聚焦該 IME sink，維持無多餘文字輸入框；滑鼠選取仍可完成。另在視窗啟動時直接向 CLI `--no-start` 查 Host Session，無須等待背景 1.5s 清單 tick。

**實機正向證據**：視窗成功還原既有測試 PTY 的 Shell 輸出，實際逐鍵 `dir` 能列出檔案；修正後以文字提交輸入並執行 `echo FOCUS_AND_TEXT_OK`、`echo 測試中文貓`，GPUI 顯示成功。再由另一支 CLI 讀取同一 Host PTY retained scrollback，確認 `has_live_ascii_echo=True`、`has_live_chinese=True`。測試時曾使用**只記錄 Focus 與文字長度、不記錄任何實際按鍵內容**的環境變數追蹤，定位後已將所有診斷程式碼移除。Windows Native `cargo test --no-default-features --lib` **318/318 通過**。

**未完成／不過度聲稱**：實體注音組字候選與多組漢字長時間切換、兩個 Terminal pane 各自 IME、長輸出/backpressure、Host 斷線自動重連、真實 UI p50/p95 延遲與完整 TUI 壓測仍待驗收；只驗證目前隔離版本，沒有替換正式 Alera，`supported=false` 不變。

## 2026-11-09：Host 推播自動恢復、雙 PTY 與長輸出

再補兩項 Native TDD：

1. **Native 推播重新連線**：原來只要一次 Host Socket 中斷／View Lease 拒絕就永久把 `terminal_stream_unavailable` 設為 true，直到關閉/開新 Tab 才會再試。改用純 `TerminalStreamRetryGate`：失敗後延遲 1、2、4、8、16、32 秒，最多 32 秒，僅在仍有運作中 Host tab 可觀察時重試；收到正確 Snapshot 就重置。等待期間原 CLI retained-scrollback fallback 繼續運作，**不會啟動 Host 或重新建立 PTY**。單元測試確認時間邊界及重置。
2. **遲到 CLI read 的畫面覆蓋競態**：舊 CLI read 進行中，Host 推播 Snapshot 也到達時，兩者的 cursor 可能恰好相同，舊讀取就會覆蓋新 VT 畫面。先新增 failing test 實際重現，再讓 `TerminalDock::apply_ticket_chunk` 在 Host stream still attached 時拒絕舊讀取，僅在 `invalidate_stream_observers` 之後允許 Host retained-scrollback fallback，保留舊的斷線重建與 Unicode 分段測試。

擴展 `desktop_gpui/tests/terminal_stream_isolated_smoke.py`，使用同一獨立 Host 的**實際兩個不同 PTY、同一常駐 CLI Socket**。一個 PTY 完全斷開 Native stream 後，新的 View Lease 可以重接同一 Session（`created=false`）；雙 PTY 的命令及輸出按 Host Session ID 分隔；以真實 Windows `cmd.exe` 產生超過 64KiB 的連續 Shell 輸出，確認尾端標記到達而非提早匹配指令回顯；最後斷開 Native 觀察，兩個 Host PTY 仍各自運作。測試子程序與 Host stderr 使用有界記憶體紀錄並回收測試 PTY，避免 Windows 殘留 lock 住臨時 log。

**驗證：**隔離真實 Host／CLI 回歸 **10/10 PASS**，其中包含「重連新 Lease」、「雙 PTY 輸出分流」、「超過 64 KiB 的真實輸出」；Native Rust 單元測試 **320/320 PASS**，包含 late CLI read 的 RED→GREEN 重現與修正。

這裡的自動重連是 Native 客戶端策略及純邏輯測試；端對端已驗證*新 CLI stream 重新附著仍存在的 Host PTY*，但尚未直接驗證**Host 進程意外退出並重啟後的整個 GPUI 自動恢復路徑**、真實 TUI p95 畫面延遲、長時間背壓/IME 候選窗。Native `supported=false` 與 Classic/正式 Host 隔離維持不變。

**隔離測試清理補強：**首次加入第二個 PTY 的壓測雖然 10/10 通過，但檢查發現兩個**測試執行檔** `__alera-terminal-pty-worker`／`__pty-job-bootstrap` 意外留在背景。只依精確 PID、執行檔路徑及 worker 參數關閉了這兩個測試程序，未碰 Classic 或正式 Host。Smoke teardown 已調整為先對兩個 Test Session 發出 terminate，然後在**該臨時 Host 父程序仍存活時**，Windows 只對 `Popen` 取得的測試 Host PID 執行 `taskkill /T /F /PID`，以回收其整棵臨時 PTY Worker 樹，絕不按映像檔名稱或全域搜尋關閉。修正後重新執行：**10/10 PASS，測試執行檔剩餘程序 0 個**。

## 2026-11-09：雙窗格輸入身份與 Host 暫時中斷畫面保留

對 Native 視窗與模型檢查後，發現兩個會阻礙驗收的實際缺口，均已先加入失敗測試再修正：

1. **第二個 mixed Terminal 缺少 IME TextInput**：原本只有主窗格持有 `QuickOpenInput::terminal` 的隱藏 `EntityInputHandler`，第二窗格僅有焦點把手和 `KeyDown`，因此注音組字提交可能遺失或被當成原始按鍵重複送出。新增第二個獨立 IME capture 與 commit observer；兩窗格的 Unicode KeyDown 均改交由 TextInput，ASCII 去重各自獨立。每次提交重新核對目前焦點窗格與現行 Host Session／workspace／tab 身分，不允許切到 Editor、其他窗格或其他 PTY 後的遲到輸入誤送。切換 Host 或 Workspace 時清空兩份組字與待送 Commit。第二窗格滑鼠放開後重新取得 IME sink 焦點，候選窗仍需要實際 Windows 注音互動驗收。
2. **短暫 CLI Session list 失敗被誤判為 PTY 全部停止**：原來 `refresh_terminals` 將失敗套用成成功的空清單，使預覽被標記停止、串流恢復無可觀察目標。新增 `session_list_stale` 狀態；失敗時保留既有 Terminal 列表、VT 畫面和 PTY 預覽，但不再允許基於過期名單對 Host 寫入。重新取得有效 Host Session 清單後自動解除 stale、恢復原 Session 身分檢查。收到**成功**空清單時仍正確視為 Session 已消失，不會憑舊畫面重新建立 Shell。

TDD：`delayed_ime_commit_must_match_focused_pane_and_active_host_identity` 首先因缺少身份核對方法而 RED，修正後 GREEN；`transient_host_inventory_failure_preserves_preview_until_authenticated_recovery` 先 RED 再 GREEN。Windows `cargo test --manifest-path desktop_gpui/Cargo.toml --no-default-features --lib --offline --locked` 已完成、exit 0（原 320 項 + 2 項新測試）。`cargo check` 用於額外驗證 UI 實際主程式連結的可編譯性。

**尚未驗收**：Windows 注音真實候選窗／兩窗格連續輸入、真正 Host 進程意外退出再恢復的 GUI 端對端流程、真實 p50/p95、長時間 TUI 壓測。這次只修改 GPUI Native 整合工作樹，不更動 Classic／正式 Host，不代表 `supported=true`，不合併、推送或發布。

## 2026-11-09：隔離 GPUI GUI 雙 PTY、私有串流中斷與真實注音候選窗

延伸 `desktop_gpui/tests/terminal_stream_isolated_smoke.py`：原生預覽現在預載同一測試 Host 的兩個真實 Session/Tab，而非只開一個。可選的 `--native-reconnect` 測試，依精確 **Native 子程序 PID、測試專用 CLI 執行檔路徑及臨時 Runtime 參數** 找到唯一 `terminal --no-start stream` 子程序，故意殺掉這條**私有** CLI 串流，再等待 Native 自動建立新子程序；絕不殺使用者 Host／Classic／其他執行中的 Alera。原測試 teardown 維持只回收測試 Host PID 樹。

- 隔離 Host/CLI **10/10 PASS**；獨立 `tool/debug/test_runtime_host_worker_recovery.py` 真正殺掉測試專用 Host 再重啟，確認原 Worker PID 與 Shell PID 原封不動、仍可輸入及讀取。這個 Host 層整合測試**不是** GPUI Host crash/restart 的 GUI 驗收。
- `--native-app ... --native-reconnect --hold-seconds 85`：**12/12 PASS**，實際 GPUI 視窗啟動；CLI private stream 子程序 PID 改變，兩個仍運作中的 Host PTY 未被重建。後續不帶 `--native-reconnect` 的單獨視窗測試 **11/11 PASS**。
- 使用 Windows 桌面上的**實際注音鍵盤事件**（並非 `desktop_input.type` 的合成文字注入）確認 Explorer filter 能出現注音候選清單；Native 的 Primary Terminal 及透過 Tab 右鍵「Move to second pane」切出的 Secondary Terminal，均能呼叫注音候選窗。Secondary 以 Enter 完成候選詞選字後，中文出現在其 Shell 游標後。`desktop_input.type` 在一般 Explorer Filter 同樣沒有產生文字，不能拿該工具的文字注入失敗直接推論 Terminal 文本處理失敗。
- 實機發現初版隱藏 1px IME input 固定在窗格內容左上角，候選清單遠離可見 Shell 游標。新增純 `terminal_ime_anchor_px`，沿用原生 VT viewport 的 JetBrains Mono 欄寬 0.62×font、列高 1.3×font 和 12/8px 內距，限制座標不超出可視窗格；雙 Pane 各從自己的 `VtScreen::cursor()` 計算定位，GPUI 使用 absolute 1px capture 而不改 Host PTY 尺寸。TDD 先 RED（缺少函式）、補齊後 GREEN，Windows `cargo check` 與獨立 Native `cargo build` 通過。
- 新編譯的 Native 實機重驗：主窗格注音候選清單已落在可見游標旁，不再固定在頂端。第二個窄窗格也有自己的注音候選清單並能完成選字；候選清單會因螢幕邊緣改往上顯示。窗格間在*尚未選字*時切換，曾短暫同時看到兩份 IME 候選清單；選字後消失，仍值得另外驗收組字取消的焦點交接。

**驗收界線**：以上成功證明測試專用 CLI stream 自動恢復、兩個真實 Host PTY 仍存在、Windows Primary/Secondary 的注音候選與提交。尚未完成 GPUI 視窗全路徑 Host 真正崩潰／重啟、不重複指令的自動恢復、IME 中途換窗的取消策略、實機 p50/p95 和更長時間 TUI 背壓。Native 仍 `supported=false`；這批變更不包含 Classic、正式 Host、main、推送或發布。

## 2026-11-09：實際 OS 焦點防護與 Host crash/restart RED 驗收

### GPUI Unicode TextInput：明確核對 OS 鍵盤焦點

發現僅使用 `mixed_panes.focused_pane()` 和 `TerminalDock.active_tab` 仍有保護缺口：點到右側 Explorer／設定後，舊 Terminal 保持最後被選取的 Host 身分，若 Windows IME 遲到的 TextInput 提交觸發原 observer，仍可能被送入 PTY。新增 `authorizes_native_terminal_commit`，TDD 先因方法不存在而 RED、補齊後 GREEN；改用 GPUI `cx.observe_in` 取得視窗，在提交當下核對 **`input.focus_handle().is_focused(window)` + pane + Host 完整身分**，失焦的提交只丟棄、不送 Host。點回任何 Terminal 或切成 Editor 時清除殘留待提交組字與 ASCII 去重記錄。這不聲稱能直接關閉 Windows 系統本身浮動的候選視窗。

Windows Native `cargo check` 與 `cargo build` PASS，Rust 函式庫 **324/324 PASS**，隔離 GPUI GUI 與兩個真實 Host Session 的正常 smoke **11/11 PASS**。實際桌面驗證：先向 Terminal 輸入，再點 Explorer 搜尋鍵入注音；Explorer 出現候選而 Terminal 內容未收到該批字元；從第二窗格的原生 TextInput 提交後輸入顯示在第二 PTY，而非第一 PTY。這是針對輸入路由的局部驗證，未涵蓋跨窗格未提交 IME 的所有 Windows TSF 角落行為。

### 真正 Runtime Host 異常結束：原 Worker 還活著，但 Native 尚無法自動附著

新增測試工具 `--native-host-restart`（必須同時指定 `--native-app`）：只建立**臨時 Runtime、兩個專用 PTY、獨立 GPUI 視窗**，只為這個測試 Host 開啟 `ALERA_TERMINAL_WORKER_PROCESS=1`。測試專用 Host 由其 `Popen` 精確 PID 強制結束並重新啟動，從臨時 runtime 的 SHA-256 session Worker 紀錄驗證原 Worker／Shell 身分；teardown 僅針對專用 Host PID 樹及嚴格核對執行檔與 Worker 模式的原 Worker PID。**無全域停止或掃描正式 Host 的行為。**

驗收結果 **RED（尚未通過）**：初版等待 22 秒時無法自動恢復，為排除 Native 最高 32 秒串流重試退避的干擾，把觀察時間延長到 **75 秒**。重測仍回報 `original_workers_alive=True`，新測試 Host 存活，但原兩個 Session 在 CLI／Native 端未自動重新恢復，所以這不是 PTY Worker/Shell 遺失。測試刻意**不**使用可能新建 Shell 的 `createOrAttach` 代替自動恢復，避免把重建 Shell 假裝成恢復成功。

原始碼定位：`terminal.observeExisting` 只允許正在 Host `sessions` map 的 PTY；Host crash 後記憶體 Session map 被清空，持久 Worker 在另一個程序，必須先由 Host 可信任路徑執行 recover-only，才能安全重新 observe。原 `createOrAttach` 可能在無法 recover 時啟動新的 Worker/Shell，不應直接成為 GPUI 背景恢復 API。

**後續設計門檻：**需在專用整合分支設計受 Host 身分與權限保護的「只恢復已存在 Worker」能力（建議語意 `recoverExistingWorker`）：驗證已註冊的 `sessionId`／`workspaceId`／`tabId`、私有 Worker registry 的身份、PID／創建時間與存活驗證；Worker 缺失／過期／身份不符時回傳明確錯誤，**絕不可 fallback 為 spawn Shell**；成功後 Native 重新取得新的 Host view lease 與 VT snapshot，依序測試雙 PTY、重試退避、舊 CLI read 競態、無重複執行、進程零殘留。實作前保持 `supported=false`，也不允許改正式 Host、合併 main、推送或發布。

## 2026-11-09：Host recover-only 路徑與背景 Tab 自動恢復（前輪 RED → GREEN）

**問題收斂：**單純讓 `terminal.observeExisting` 在 Host 重啟後恢復缺失的 Session，確實能恢復 Native 前景一個 PTY；但原 75 秒隔離 E2E 仍為 RED，因為 Native 原本只觀察前景與分割的可見 Terminal，**第二個未顯示的 Tab 根本沒有發出觀察請求**。一律串流所有背景 TUI 雖可繞過此問題，卻會提高 CLI/VT 解析量，違反快路徑與隔離原則。因此區分「前景有輸出的 observe」和「只恢復 Host Session、不訂閱輸出的 recover」。

**安全實作：**

- Host `Session::start` 新增明確 `recover_only`，雙層防護：先驗證私有 Worker registry 與 PID／啟動身分，再在真正的 Worker attach 位置禁止任何 `spawn_pty`／`spawn_worker_pty` fallback；原本明確建立、重啟 Terminal 的呼叫一律傳 `false`，保留 Classic 行為。
- `terminal.observeExisting` 遇到 Host 記憶體中沒有 Session 時，只允許從已註冊的 Workspace／Tab／Session 三重身分與真正存活 Worker 重建 Session。缺少 Worker、Tab 移動、身分不符、Worker 在檢查後消失，均回錯，絕不建立替代 Shell。傳入的 `launch`／shell／環境變數完全不參與 recovery。
- 新增 Host `terminal.recoverExistingWorker`（需 Host 驗證）專用於背景 Tab，只重建原 Worker 的 Host Session，不建立輸出 view lease、不訂閱 TUI。CLI 私有 NDJSON `recover` 只轉送白名單三個 ID，不接受 shell/launch/任意 mutation，只有已驗證的 `observe` 才允許後續 `write`。
- Native 每條新的私有 CLI stream 對非可見預覽各送至多一次 recover-only，並在 Host 回覆 `running=true` 且 Session ID 完全吻合後重新列出 Session。**在 Host 正式清單確認之前，舊 Tab 不因恢復回覆直接獲得輸入權限。**可見 Terminal 仍以獨立 view lease + snapshot/推播運作，不擴大背景輸出訂閱。

**驗證紀錄：**新編譯 CLI `D:/c/t/debug/alera.exe` 與隔離 GPUI 執行檔 `D:/c/t/debug/alera-gpui-desktop.exe`，測試專用臨時 Runtime、兩個原生 Windows PTY。使用 `--native-host-restart --hold-seconds 3` 真正強制結束首次 Host PID，啟動另一個 Host PID，Native 自動建立新的 CLI 子程序並恢復兩個 Host Session；隔離腳本逐一比較兩個 Worker PID 與 Shell PID 都不變，**12/12 PASS**。沒有改動任何使用者正式 Host 或 Classic 的 Runtime。

Native 新增恢復回覆識別測試後，全套 **325/325 Rust lib 測試通過**。Host 專項 `native_observe_recovery_refuses_absent_worker_even_if_tab_and_launch_are_valid` **1/1 PASS**，保護「註冊 Tab + 偽造 launch 但無 Worker」場景。Windows CLI 與 GPUI `cargo check`、實際 `cargo build` 均成功；保留 `.gpui-smoke-20261108/` 原始資料。

**補充 GUI 真實畫面驗收**：重新執行 `--native-host-restart --hold-seconds 55`，測試 Host PID `35656` 被強制結束，由 Host PID `40116` 接替，兩個原有 Worker／Shell PID 均不變，原 Native stream sidecar 從 PID `7116` 恢復為 `30216`。完整隔離測試結束 **12/12 PASS，exit 0**。在這段真實 GPUI 視窗期間，透過獨立測試 CLI（非 GUI 合成輸入）向第一個原 PTY 寫入新 `echo AFTER_HOST_CRASH_GUI_OK_7391`，Windows 實際桌面截圖確認新標記與新提示字元出現在第一個 Native Terminal；切換到原第二個背景 Tab 後，原 `BULK_..._950` 尾端仍可見，再寫入新 `echo AFTER_BACKGROUND_RECOVERY_OK_5832`，實際桌面截圖確認新標記也顯示在第二個 GPUI Terminal，證實重啟後不只是 Host Session ID 恢復，連 UI VT snapshot、兩個 PTY 的後續輸出都能繼續呈現。**此檢查為依序切換兩個 Tab，不宣稱同時左右分割窗格已重新驗收。**

CLI `terminal_stream::tests` 私有 recover 指令白名單 **2/2 PASS**，確保多餘 shell/launch/工作目錄不會傳到 Host；Native 背景恢復回覆身份拒絕案例併入上述 **325/325 PASS**。測試用 Host/GUI 結束後自動清理其專用程序及臨時目錄。

**仍未驗收**：真正 GUI input-to-visible-output p50/p95、長時間多 TUI 背壓、同時雙分割窗格重啟後焦點／注音互動，以及 TSF 未完成組字焦點切換時 Windows 系統候選窗的取消行為。不能因 Worker PID、Host 清單及此次兩個 Tab 視覺恢復通過就直接標記 `supported=true`；不合併 `main`、不推送、不發布。

## 2026-11-09：雙窗格真實焦點、可見延遲與持續推播門檻

**同時雙窗格真實重啟與 IME 目視驗證。**在測試專用的 GPUI 視窗右鍵第二個 Terminal Tab，執行原生 `Move to second pane`；兩個 Host PTY 於左右窗格同時可見，實際 GUI 輸入 `echo SPLIT_PRIMARY_BEFORE_2391` 與 `echo SPLIT_SECOND_BEFORE_2391` 在各自畫面執行，沒有跨 Pane 串線。Windows 繁中注音組字候選窗由第一窗格切換第二窗格時消失，未確認文字沒有意外送入第二 PTY。使用 opt-in `--native-split-gate`（實際目視確認後才觸發一次性 worktree marker），測試隔離 Host 真正 crash/restart；左右分割保持，之後兩邊 GUI 再分別輸入 `echo GPUI_PRIMARY_AFTER_CRASH_8514` 和 `echo GPUI_SECONDARY_AFTER_CRASH_8514`，實際桌面截圖確認各自正確輸出。**85 秒隔離 Host／CLI smoke 12/12 PASS**；此為實機操作與截圖驗證，尚非完全自動化 OS/IME 圖形回歸。

**OS key-down 到畫面可見像素變更的 p50/p95 基準。**新增 `desktop_gpui/tests/native_terminal_visual_latency_probe.py`，嚴格限制為前景且命令列含唯一隔離 smoke profile 的 `alera-gpui-desktop.exe`，DWM 物理座標、MSS 針對單 Pane 裁切取樣、不做 OCR、不讀 Host token 或其他應用視窗；僅在確定測試 Shell 為英文輸入狀態時逐字輸入測量。第一窗格 **16 個樣本 p50 92.05ms、p95 124.39ms**；第二窗格 **24 個樣本 p50 74.94ms、p95 92.41ms**。桌面截圖 median **16.48ms**，量測包含輸入送出、Native/Host、GPUI 重繪與取樣抖動，不是 GPU 精確 present latency，亦未與 Classic 同場比較。非同步候選窗及命令欄換行可影響畫面像素判定；工具改以可變 ASCII glyph、主視窗最大面積、最多每 10 字重置測試欄位，仍需重複多輪與不同 Windows IME 狀態後才能推廣。

**持續輸出測試的假陽性與根因修復。**初版 `--native-load-seconds` 的完成標記 `LOAD_DONE_...` 直接寫在提交至 `cmd.exe` 的 PowerShell 命令中；cmd 的輸入回顯讓測試在發送命令後提早讀到標記，誤稱「30 秒負載完成」，而 GPUI 截圖只顯示較早輸出。**此先前 30 秒完成結論無效，不代表已證實 Native 解析落後。**修正為在 PowerShell 內把 `'LOAD_'+'DONE_...'` 動態串接，保證提交命令字串中沒有完整完成標記；另對真正耗時設最低 `seconds*0.8` 門檻。新增純函式測試驗證完成標記不會在輸入命令回顯中出現，避免再有假陽性。

**修正後真正持續輸出 GREEN。**在隔離 Host 真實 crash/restart 後持續兩個原 PTY 各輸出每秒 40 行，20 秒負載的驗收總耗時 **26.4 秒**（含 CLI 啟動／查詢），Host end cursors `[111116, 323849]`，獨立 Native debug-only 游標紀錄最終也是 `[111116, 323849]`，串流 `active=2` 且事件佇列清空；Native RSS **95.8→97.0 MiB（+1.2 MiB）**，測試期間 Native process CPU time **2.14 秒**。再次改成 10 秒負載，真正耗時 **12.3 秒**，Native CPU **1.19 秒**、RSS **93.3→96.8 MiB（+3.5 MiB）**，最後以新 `native_visible_cursors_from_diagnostic()` 比對 Host 的新鮮授權讀取游標，兩邊為 `[56457, 268831]`、串流 `active=2`，全部追上且 **14/14 隔離測試 PASS**。正式驗收條件新增「已存在的兩個可見 PTY 的 Native 游標不得小於 Host 最終游標」，不再只看 Shell 完成標記。儀表僅 `debug_assertions` + `ALERA_GPUI_ISOLATED_STREAM_DIAGNOSTICS` opt-in，每秒輸出數值、訂閱數和 cursor，不輸出 Host token／命令／PTY 內容；release 不含偵錯列印。

**目前 Gate**：同時雙 Pane / Host crash / 輸入路由 / 忽略逾期 IME 提交與短時負載的記錄更完整，但 Native 仍 `supported=false`。剩餘的是真正超過 30–60 秒的穩態負載多輪比較、量測不同 TUI（全螢幕互動／ANSI 變化）、跨 Windows TSF 候選組字階段的系統級驗收、Classic 對照 p50/p95 與 Native release 版效能；不合併 `main`、不推送／發布，不改 Classic 或正式 Host。

## 2026-11-09 延長驗收：30/60 秒 ANSI TUI、雙 Pane 可視更新及背景輸出延遲

在既有隔離 Windows Host／Worker／GPUI 測試架構加入：

- `--native-load-style ansi`：PowerShell 使用 `[char]27` 從*測試 PTY 內部*產生 ANSI 控制序列，進入／離開 alternate screen、隱藏／恢復 cursor、游標定位到 18 行、SGR 彩色文字、清除行尾。測試仍是合成 TUI，並非真實第三方全螢幕編輯器。原 `plain` 持續輸出維持不變。
- `--native-load-target secondary`：只對隔離的第二個 Host PTY 注入受限長度 TUI，保留第一個 PTY 的 `cmd.exe` 互動權限，以實測「右側負載／左側正常鍵盤輸入」。只對真正送負載的 PTY 查核完成 sentinel，不誤稱兩個 PTY 都送了壓力輸出；最終的 Native／Host 輸出 cursor 仍對**兩個可見窗格**逐一比對。
- `native_terminal_visible_activity_probe.py`：只允許目前前景且啟動參數包含隔離 smoke profile 的 GPUI PID；DWM 確認 Native 實體視窗，MSS 各取左右 Pane 的內容區域；18 組每 500ms 像素差異，過濾游標閃爍等小變化，不存圖片、畫面文字或 Host token。這是「兩個窗格實際有更新」的偵測，**不是 FPS 或持續零延遲的證明**。只有在 shell 確實輸出的時間窗執行才有效。
- `test_terminal_load_probes.py`：7 個純函式回歸測試，涵蓋 plain/ANSI 送出的 shell 命令永遠不含完整完成 sentinel、ANSI enter/exit 訊號、拒絕未知負載模式、兩個訂閱／游標驗證、裁切不重疊、像素差異與 percentile。

### 實測（全部為隔離 Host 實際 crash/restart 後、左右 GPUI 同時顯示原 Worker PTY）

| 工作負載 | 實際負載完成時間 | Native RSS 峰值增加 | Native CPU 累積時間 | Host／Native cursor | 契約 |
|---|---:|---:|---:|---|---|
| 雙 PTY、30 秒 ANSI/alternate-screen | 35.0 秒 | +0.7 MiB | 2.42 秒 | `[35153,243844]` 一致 | 14/14 PASS |
| 雙 PTY、60 秒 ANSI/alternate-screen | 61.6 秒 | +0.4 MiB | 4.23 秒 | `[57470,270422]` 一致 | 14/14 PASS |
| 雙 PTY、60 秒 plain 大量輸出 | 61.6 秒 | +0.7 MiB | 5.39 秒 | `[325932,538439]` 一致 | 14/14 PASS |
| 第二 PTY、45 秒 ANSI（第一 PTY 保持互動） | 45.4 秒 | +0.0 MiB | 1.97 秒 | `[2427,256350]` 一致 | 14/14 PASS |

**畫面確實更新**：兩窗格同時執行 60 秒 ANSI 的實際中段，18 個相鄰截圖間隔中左 **8/18**、右 **11/18** 次有顯著內容差異；60 秒 plain 中左 **10/18**、右 **8/18** 次有顯著差異。因負載本身每 900ms 一輪、截圖每 500ms，此更新率為預期；不能以之推算 FPS。所有端到端測試須看到完整（非 cmd 輸入回顯）的 sentinel、至少 80% 配置時長，以及最終兩個原 VT cursor 都趕上 Host，才算 PASS；沒有發現 Worker 身分更換、訂閱斷線或測試窗口崩潰。

**單窗格忙碌時另一個正常輸入的可見延遲（同一測試 Native PID、同一 Windows 畫面）**：右側 45 秒 ANSI TUI 運作期間，左側 20 個英文字鍵輸入樣本 **p50 92.14ms／p95 94.94ms**，截圖 median **16.46ms**；右側負載結束後，同窗格／同環境 20 個樣本 **p50 92.20ms／p95 93.17ms**，截圖 median **16.68ms**。本次未觀察到可分辨的忙碌窗格拖慢互動窗格；量測是鍵盤事件到第一個視覺差異，**不代表每個鍵都已由 Host 確認、不是 GPU 精確 present 時間**，且樣本數小、需 Classic 同場對照與高頻真實 TUI 重複測量。

**更新後仍未關 Gate**：`supported=false`，不合併 `main`、不推送／發布。不再缺「60 秒雙 PTY 合成 ANSI/文字負載」與「忙碌時另一窗格輸入基準」，但仍缺真實高頻全螢幕 TUI app、Windows TSF 注音候選半組字取消的全面系統測試、Classic 同場延遲及 release build 的效能與回歸。保留原 `.gpui-smoke-20261108/` 未追蹤實驗資料，不清除、不加入提交。

## 2026-11-09：真實全螢幕 less.exe 互動與 Host 重啟後恢復驗收

在原有隔離 Host／CLI／Native 測試工具加入 opt-in `--native-real-tui-pager`，只接受安全的絕對路徑與唯讀 `less.exe`，實測使用 Git for Windows 的 `C:/Program Files/Git/usr/bin/less.exe`。這是**真實第三方全螢幕應用**，不是 PowerShell 合成 ANSI/TUI 輸出。

- 每次在隨機 TemporaryDirectory 的測試 Workspace 產生唯一 240 行文字檔；透過測試 Host 既有 PTY 啟動 pager，等待 Host 真正輸出第一行；輸入 `G` 跳檔尾，確認最後行出現在 Host 原始輸出且 cursor 增加；輸入 `g` 回檔首並要求新輸出；輸入 `q` 後由**同一個 Shell**建立另一個不屬於 pager 的檔案，確保不是指令回顯造成假陽性。測試不修改原檔、不呼叫使用者正式 Host。
- 初輪真正執行 `less.exe` 的三階段輸出已發生，Shell 也已恢復；但隔離 Smoke 最後因**驗收工具**將 `active=1`（單一可見 viewport）且 `tabs=[(cursor,true),(0,true)]`（第二個仍存在的背景 Tab）誤判為兩個可見窗格而 RED。新 `native_primary_cursor_from_diagnostic` 明確要求單一活躍串流與第一個 Host Tab running，允許背景 Tab 仍存在；兩窗格原 `native_visible_cursors_from_diagnostic` 的嚴格檢核維持。新增正反向回歸。
- 修正後 **Python 10/10 PASS**；不 crash 的真實 Windows Host／CLI／GPUI **13/13 PASS**：pager 首頁新輸出 1929 bytes、`G` 至最後行新增 1284 bytes、`g` 回首頁新增 1240 bytes，原 Shell 回應成功，Host／Native 最終游標 **7618／7618**，且 Native 子程序未結束。
- 使用**真正隔離 Host 強制退出／重啟**再跑相同真實 TUI：**14/14 PASS**。Host PID 更換、原兩個 Worker／Shell 未更換；後續 Native／Host 最終游標 **14854／14854**，說明 Host 恢復後仍能開啟、導航、退出全螢幕 pager，未用新 Shell 偽造恢復。
- 重複執行指令（對照實際可執行檔路徑調整）：
  `python desktop_gpui/tests/terminal_stream_isolated_smoke.py D:/c/t/debug/alera.exe --native-app D:/c/t/debug/alera-gpui-desktop.exe --native-host-restart --native-real-tui-pager "C:/Program Files/Git/usr/bin/less.exe" --hold-seconds 3`。
- 證據邊界：此輪驗的是**第一個可見 Terminal**，Host 事件和 Native 消費進度，不是每個按鍵的 GPU Present 時戳；也不是兩個真實 TUI 同時高頻重繪、TSF 注音未完成組字切窗、Classic／Native Release 同場比較。以上仍待驗收，Native `supported=false` 不變；不合併 main、不推送或發布，不修改 Classic、正式 Host，也不清除／提交 `.gpui-smoke-20261108/`。
