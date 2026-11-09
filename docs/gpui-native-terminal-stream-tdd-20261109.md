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
- 仍欠真正隔離 Host 的完整端對端連線／輸入／推播／backpressure 測試，以及新 Native executable 的實際鍵盤、IME、TUI 與 p50/p95 延遲測試。CLI Sidecar 還未搭配 Native 預覽程式更新部署。
