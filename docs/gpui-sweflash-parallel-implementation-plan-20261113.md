# Alera GPUI：SWE-2 Flash 平行實作計畫（2026-11-13）

原始主追蹤：docs/gpui-feature-completeness-tracker-20261112.md。
基線：integrate/gpui-central-diff-into-main-20261112，原 HEAD 469bd5d0d08be58b1694cff1425ead1cacf0aa5c。
**原定 NOW = T07；原順序 F1 → F2 → F3 → F4 → F5 → F6 不變。** 平行只意味開發準備／修正可以提前，不等於按錯順序整合或宣告完成。

## 執行者與資源

- 主整合者負責：唯一追蹤表、Classic 行為核對、衝突裁決、隔離 Windows GUI/Host 驗收、TDD 與按依賴順序 cherry-pick。
- 子代理使用本機已驗證的 Devin CLI 模型 **swe-2-flash-high**；工具支援 --model、--print、--prompt-file 和 --permission-mode。初始最多 **2 位 SWE Flash 同時寫碼**，另由整合者做審核。
- 兩位子代理各有獨立 Git worktree/branch，兩者皆從同一已提交的 integration 基線建立。每人只改其工作包、交付自己的 commit 與專屬交接文件。
- Windows 每次最多一個完整 Cargo 編譯／全套測試；子代理先跑窄範圍 RED→GREEN，再由整合者一次執行跨模組／GUI 驗收，避免共用 target 與 CPU 爭用。
- 不碰正式 Classic Host、使用者 PTY/Session、任何真實 Provider 憑證／私人設定、SSH/部署、主分支、已存在 WIP/鎖定 worktree 和受保護 fixture；不得跳過權限或使用 dangerous 模式。實驗版 supported=false。

## 第一波（平行分工）

| 工作流 | SWE Flash 任務 | 拆分／目標 | 要交付 |
| --- | --- | --- | --- |
| A / T07 — 優先整合 | **Command Profile 的 Command / Custom Prompt 日常編輯**；對齊 Classic；Host revision CAS，保留現有 Description、Quota Group、Managed fields；命令/提示限暫時編輯，不在 Native state 長期保存，不能在參數或日誌洩露認證 | 既有 agent profile 編輯 UI、Rust Host/CLI 的安全讀改寫；禁止修改真實登入資料 | 一個正常 RED→GREEN 測試、isolated Host 修改/讀回或清楚標記未驗收、獨立 Commit 和交接文件 |
| B / E01 — 提前研發，暫不整合 | **Explorer 單擊 preview → 雙擊正式開啟 → Ctrl+P 重開同檔**；維持相同 canonical file identity，防重複可編輯 Tab、Dirty buffer 保護；不同 Worktree 同名檔隔離 | 只動 Explorer、Open/Tab、Editor identity，不能碰 Profile、Quota、Runtime Host | 一個正常 RED→GREEN 測試，成功 preview/promote/open/dirty 的結果或明列尚缺 GUI，獨立 Commit 和交接文件 |

A 分支：feat/gpui-sweflash-t07-command-prompt-20261113；預備 worktree：.worktrees/gpui-sweflash-t07-command-prompt-20261113。
B 分支：feat/gpui-sweflash-e01-open-20261113；預備 worktree：.worktrees/gpui-sweflash-e01-open-20261113。

## 後續波次與原定交付順序

- T07-A2：Managed Profile Model／Persona／正常選項編輯；T07-A3：隔離無登入/過期環境的 Provider 錯誤提示。T07 日常正向 GUI 俱全後才能將 NOW 推進 E01。
- E01 完成後，按 E02（Edit/Find/Save）、E03（Explorer CRUD/drag）、E04（Quick Open/全文）、E05（跨 Worktree）、E06（雙 Pane Dirty/restart）依序整合；先行開發可以平行，但不得跳過集成 Gate。
- G01–G06 Git 可以提前只讀盤點或在別的 worktree 編碼，但必須等 Editor 主要流程完成才按序整合；接著 S01–S04 Settings/Rust Core、H01–H03 Hook、U01–U05 UI/UX（包括仍未完成的真 Provider 配額和 Classic/GPUI 雙視窗釘選）。
- 每批最多 2 個同時編碼工作包；有重疊的 main.rs 或 Rust Core 協定由整合者序列化 cherry-pick，禁止共同直接修改 integration tree。

## 子代理交付與 Review/Integration Gate

1. 各子代理啟動先確認 Git HEAD 與乾淨工作樹，只能寫自己的 worktree；嚴禁 main merge、Push、Release、部署或觸碰正式 Runtime/PTY。
2. 先從 Classic 取得實際行為契約，針對正常功能寫定向失敗測試，再最小實作，執行關聯回歸。不要為極端競態/故障壓測耗掉主要工期。
3. 結果要有：base/head commit、變更清單、測試指令與實際結果、GUI/Host/磁碟讀回證據、風險/阻擋、是否需整合者解衝突。子代理只能修改自己命名的交接文件，不直接修改總追蹤表。
4. 整合者先 code review、核對秘密與 Host owned 設定、執行定向測試和必要 Windows 正常 GUI，再由 T07→E01→E02… 順序將單獨 Commit 整合到原 integration 分支。
5. 未通過的子項維持 IN REVIEW/BLOCKED，不宣告 DONE；整合後立即更新原 tracker 的 ID、實測、Commit、下一個 NOW。只有另行授權才可能合 main、Push/Release；supported=false 不因測試綠燈自行改變。

## 狀態追蹤

| 工作包 | 分支 | 子代理狀態 | 提交／驗收 | 整合 |
| --- | --- | --- | --- | --- |
| T07 Command/Prompt | feat/gpui-sweflash-t07-command-prompt-20261113 | PREPARED | 待開始 | 先審後整合 |
| E01 Preview/Open | feat/gpui-sweflash-e01-open-20261113 | PREPARED | 待開始 | 等 T07 DONE |

PREPARED 並不代表子代理已啟動；必須有真實 CLI session 和輸出才可改成 RUNNING。只有經整合者驗收的正常 GUI 功能才能標 DONE。
