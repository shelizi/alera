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
