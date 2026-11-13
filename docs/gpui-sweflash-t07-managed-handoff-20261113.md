# GPUI T07 — Managed Model／Persona 安全編輯通道

日期：2026-11-13（Asia/Taipei）
工作分支：`feat/gpui-sweflash-t07-managed-20261113`
Base：`f6d09cdd8326e2d6522ffde849805e79e018a382`
目前狀態：**CLI Managed 後端 4/4＋隔離 Host 1/1 PASS；GPUI Managed UI 已接線、Native 專項 2/2 與 Binary 回歸 34/34 PASS；Windows GUI 實際操作待驗收。**

## 需求／實作

- 以 `agent-profile update --edit-managed-selection-stdin --expected-revision=N` 傳入小型 JSON，例如 `{"model":"new-model","persona":"reviewer"}`，而非把 Managed JSON 暴露在 argv。
- 只允許 `model`／`persona`，`null` 明確清空；**Classic Persona 在 Rust Host managedConfig 實際對應 `agent`**，API 將 UI 的 `persona` 映射成 `agent`，不可直接寫入未受 Host 支援的 `persona` key；拒絕其他金鑰、型別、控制字元、空字串與超長資料。每個選項上限 256 bytes、整體 stdin 4 KiB。
- 必須選擇既存 Managed Profile；透過 shared Rust AgentDescriptor 的 `model_override`／`supports_persona` 判定是否可調整。
- 用 Host 回傳的原始 managedConfig 物件合併指定欄位，**不重建或丟失其他未知欄位／安全選項**，保留 CLI 既有 `ensure_expected_revision`／`ensure_risk_confirmation`／Host revision CAS。不可自動帶入 `--confirm-reduced-protections`。
- `--edit-managed-selection-stdin` 和全量 Managed config、Command／Prompt 編輯、其他 Profile 欄位異動相互排斥，要求明確 revision。
- 新增 CLI tests：既有風險控制保留、只有支援的 adapter 能修改 Persona／Model、清空、違規欄位／過長輸入拒絕；另有隔離 Host conformance 測試含 CAS 舊版拒絕與獨立讀回。

## GPUI Native UI 接線（本輪）

- `agent_profiles.rs` 只解析 Host Show 的 ID、revision、Model、Persona（Rust `managedConfig.agent`）。其餘 Managed Config、Permission、Sandbox、Prompt、憑證不進入 Native UI state 或共享設定。
- `agent_runtime_bridge.rs` 新增明確的 `load_agent_profile_managed_selection` 與 `update_agent_profile_managed_selection`；只以 `--edit-managed-selection-stdin` 傳有界 Model/Persona JSON，argv 不帶模型或 Persona 名稱；Host 回傳須推進 revision，另以 Show 獨立讀回確定值一致。
- `main.rs` 設定窗只儲存瞬時選取欄位，切換 Profile、Host catalog 修改 revision／選取失效／Host 斷線便清除輸入與快照，不寫入持久化設定。
- `native_settings_ui.rs` 依共用 Rust Agent Descriptor 的 `model_override`／`supports_persona` 顯示真正支援的欄位，提供 Load / Save / Clear Model / Clear Persona；IME composition 期間拒絕儲存，清空原有欄位必須明確按 Clear，CLI 保留所有未知或安全相關 Managed 選項。
- `managed_choice_discovery.rs` 參照 Classic 現有唯讀 CLI 探索命令接上可點擊的 Model／Persona 選項：Model 支援 Codex、Cursor、Antigravity、Grok、OpenCode／OpenCode2；Persona 支援 Antigravity、OpenCode／OpenCode2。使用者需明確點選 Refresh，才能從已安裝的 CLI 探索。命令與參數固定，不接任意 shell 輸入，30 秒逾時、1 MiB 輸出上限、128 筆選項與格式過濾；未知來源、未安裝或探索失敗仍可手動輸入精確 ID。Claude 等目前沒有相容唯讀列表命令的 Agent，不虛構動態清單；Classic 的某些靜態預設選項仍未全面在 GPUI 呈現。

## 驗證進展

- CLI 專項原始四項測試 **4/4 PASS**，確認原設定保留、Adapter 支援限制與旗標衝突。
- GPUI Managed Host 投影與 Bridge 定向 Rust **2/2 PASS**，Native Binary 回歸 **34/34 PASS**；Windows 原生 `cargo check --bin alera-gpui-desktop` 與 Debug EXE 編譯 **PASS**。
- Classic 相容 Model／Persona 探索定向 Rust parser **3/3 PASS**；該版 Native Binary 回歸 **34/34 PASS**、Windows 原生 `cargo check --bin alera-gpui-desktop` 與 Debug EXE 完整編譯 **PASS**。
- 第一輪隔離 Host conformance 在測試 Profile 建立時被 Host 拒絕，原因先後是測試使用不合法的 `customFlag` 和 `persona` Managed key；已修正測試 fixture 並依 Classic/Host 契約將 Persona 變更對應到 `agent`。更正後 **CLI 4/4 PASS、隔離 Host 1/1 PASS**；Host 實測模型／Persona 寫入、Permission Mode 保留、舊 revision 拒絕、清空及獨立 Show 讀回。

## 驗收界線

- Subagent SWE-2 Flash High 僅完成程式碼閱讀，執行逾時沒有生成修改；本輪由主控依原計畫補最小 Host API。
- 所有程式修改均在獨立 feature worktree。Cargo 使用既有 Windows 外部 target 快取，由主控執行。
- **GPUI Model／Persona 編輯 UI、Host 讀回與指定 Agent 的唯讀動態探索選單已接上，但沒有連接真實 Provider 的探索正向結果或 Windows GUI 滑鼠／鍵盤／注音實測**。某些 Classic 靜態清單和所有 Managed 安全進階選項仍待對齊，不能宣稱整體 T07 DONE。
- 已核對 CLI／Host 整合測試結果，並保留降低沙盒／核准防護的既有確認門檻；沒有修改真實憑證、正式 Runtime／PTY，也不合併 main、Push、部署、Release。原主追蹤 `NOW=T07`、`supported=false`。
