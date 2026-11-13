# GPUI T07 — Managed Model／Persona 安全編輯通道

日期：2026-11-13（Asia/Taipei）
工作分支：`feat/gpui-sweflash-t07-managed-20261113`
Base：`f6d09cdd8326e2d6522ffde849805e79e018a382`
目前狀態：**CLI 底層程式已驗證，4/4 定向測試與 1/1 隔離 Host 寫入／讀回通過；尚未接 GPUI Managed UI。**

## 需求／實作

- 以 `agent-profile update --edit-managed-selection-stdin --expected-revision=N` 傳入小型 JSON，例如 `{"model":"new-model","persona":"reviewer"}`，而非把 Managed JSON 暴露在 argv。
- 只允許 `model`／`persona`，`null` 明確清空；**Classic Persona 在 Rust Host managedConfig 實際對應 `agent`**，API 將 UI 的 `persona` 映射成 `agent`，不可直接寫入未受 Host 支援的 `persona` key；拒絕其他金鑰、型別、控制字元、空字串與超長資料。每個選項上限 256 bytes、整體 stdin 4 KiB。
- 必須選擇既存 Managed Profile；透過 shared Rust AgentDescriptor 的 `model_override`／`supports_persona` 判定是否可調整。
- 用 Host 回傳的原始 managedConfig 物件合併指定欄位，**不重建或丟失其他未知欄位／安全選項**，保留 CLI 既有 `ensure_expected_revision`／`ensure_risk_confirmation`／Host revision CAS。不可自動帶入 `--confirm-reduced-protections`。
- `--edit-managed-selection-stdin` 和全量 Managed config、Command／Prompt 編輯、其他 Profile 欄位異動相互排斥，要求明確 revision。
- 新增 CLI tests：既有風險控制保留、只有支援的 adapter 能修改 Persona／Model、清空、違規欄位／過長輸入拒絕；另有隔離 Host conformance 測試含 CAS 舊版拒絕與獨立讀回。

## 驗證進展

- CLI 專項原始四項測試 **4/4 PASS**，確認原設定保留、Adapter 支援限制與旗標衝突。
- 第一輪隔離 Host conformance 在測試 Profile 建立時被 Host 拒絕，原因先後是測試使用不合法的 `customFlag` 和 `persona` Managed key；已修正測試 fixture 並依 Classic/Host 契約將 Persona 變更對應到 `agent`。更正後 **CLI 4/4 PASS、隔離 Host 1/1 PASS**；Host 實測模型／Persona 寫入、Permission Mode 保留、舊 revision 拒絕、清空及獨立 Show 讀回。

## 驗收界線

- Subagent SWE-2 Flash High 僅完成程式碼閱讀，執行逾時沒有生成修改；本輪由主控依原計畫補最小 Host API。
- 所有程式修改均在獨立 feature worktree。Cargo 使用既有 Windows 外部 target 快取，由主控執行。
- **尚未接上 GPUI Model／Persona 輸入、選單及 Host 讀回 UI**，不能宣稱 T07 Managed 功能完整或 Windows GUI PASS。
- 已核對 CLI／Host 整合測試結果，並保留降低沙盒／核准防護的既有確認門檻；沒有修改真實憑證、正式 Runtime／PTY，也不合併 main、Push、部署、Release。原主追蹤 `NOW=T07`、`supported=false`。
