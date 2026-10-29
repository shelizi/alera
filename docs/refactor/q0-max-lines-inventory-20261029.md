# Q0 — Max-lines ratchet 分類及降低計畫（2026-10-29）

> **第二批更新**：`refactor/tdd-quality-20261029` 已以 TDD 將 `ai_assist_registry.dart` 由約 616 行拆為 466 行（Codex/Copilot/Cursor 獨立 agent adapter），`--list-oversized` 檔案 167 → 166、未獲 baseline 許可者 134 → **133**。下表仍是原始盤點快照，總分布需在每輪重新量測，沒有放寬 baseline。

本文件與 `tdd-progress-20261029.md` 一起追蹤，僅代表本工作樹當下靜態掃描，非整個產品的缺陷數。

## 重現步驟
```powershell
dart tool/quality/check_max_lines.dart --list-oversized
dart tool/quality/check_max_lines.dart
```
門檻：500 行；`--list-oversized` 顯示所有超過門檻者，`check_max_lines` 只對未獲 baseline 容許或超過 baseline 的檔案失敗。嚴禁把 167 個大檔直接改基準達成綠燈。

## 2026-10-29 inventory

| 分類 | 大於 500 行數量 | 建議 |
| --- | ---: | --- |
| Desktop source（不含 localization） | 45 | 以 ownership/可測性的實際收益選擇重構；先 Terminal、Git diff、Explorer |
| Rust source | 50 | 按 request capability／store transaction／native worker 分工，不打散單一 pool 或 actor |
| tests（Desktop/Mobile） | 57 | 測試資料與 scenario helper 可搬遷；保留完整契約與回歸斷言 |
| Localization | 7 | 資料表/文字資源非核心高耦合；審核後允許結構性豁免，不應機械拆字串 |
| 其他（如 integration/tool） | 8 | 逐項 review |
| **總計** | **167** | 33 baseline、19 grew、115 new；**134 違反既存 ratchet** |

最長的高價值目標：
- `lib/src/features/workbench/presentation/terminal_xterm_worker.dart` 2,643 行：Parser/terminal worker 互動、buffer 生命週期，先加高壓恢復測試。
- `rust/alera-core/src/runtime/store.rs` 2,139 行：依 domain transaction 分層，不額外新建 SqlitePool。
- `rust/src/api/agent_runtime_overlay.rs` 1,900 行：native runtime overlay 的 owner 與模型觀察。
- `rust/src/api/workspace_files.rs` 1,836 行：路徑安全/檔案 mutation 資源邊界。
- `lib/src/features/workbench/presentation/workspace_git_diff_surface_editable.dart` 1,827 行：Diff projection/cache 依效能 profiling 下沉 Rust。
- `rust/alera-cli/src/terminal_host/server/mobile_workspace_file_requests.rs` 1,605 行：file requests 能力分離，跨語言 fixture。
- `lib/src/features/workbench/presentation/workspace_explorer.dart` 1,286 行：將 FileService I/O 與 UI tree projection 分離。

## 執行規則

1. 當下仍為 **P0 未完成**，不能因其它 guard 通過就宣稱所有 CI checks 綠燈。
2. 每次小批 TDD 新增項目應 **不增加新的未獲豁免 >500 行檔案**，且原 baseline 行數不得增長；依品質指標選實際的重構收益。
3. 進入 Q0 專責批次時，將 134 offending files 明確分成必須拆的行為邏輯、長測試 scenario、純資料表與只需訂好 ignore policy 的生成內容；逐項記錄理由。
4. **不得一鍵 `--write-baseline` 以掩蓋已知 134 個問題**。若產品需要短期恢復 CI，須先提供 review 可讀的例外政策與固定期限；本輪不做這個產品決策。
5. 每移出一段有狀態/IO 責任，新增直接 unit tests，保持值、排序、error、cancellation、性能同等或更好。

## 下批最小任務
- [ ] 在 `terminal_xterm_worker.dart` 辨認可獨立測試的 worker parsing/buffer slice，先建立確定性 red test。
- [ ] 長測試獨立 helper/shard 只移搬測資，不改驗證意涵。
- [ ] 量測過大的 editor/diff 檔案是否 CPU bottleneck；有證據再 Rust 化。
