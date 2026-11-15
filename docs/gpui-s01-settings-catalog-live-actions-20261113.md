# Alera GPUI S01 — Classic Settings 逐列能力盤點與常用入口接線

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；本輪起始 HEAD：`0eb9977d8`。
狀態：**S01 核心路由／分類已實作與專項驗證；真正 Windows GPUI GUI 及隔離 Host CAS 操作另待驗；`supported=false`。**

## 本輪找到的功能缺口

1. Settings 的 **Editor → Two-row tabs** 原本在實際 Classic 預覽目錄裡沒有 `shared_setting_row_path` 對應，因而落入 `Not available in GPUI yet`，即使 Native 舊設定畫面與共用 Rust Core 均已經支援 `editor.twoRowTabs`。新增路由到核准的 `editor.twoRowTabs` Host 路徑，使用原有 `native_settings_bool_row`（Runtime Host CAS→snapshot readback），不是再寫舊 Native JSON，也不複製 Flutter 設定。
2. Settings 的 **Quotas → Provider Quotas** 原本同樣顯示不可操作，但 Native 已有完整 provider/account 額度總覽。新增真正 **Open Quota Overview** 按鈕；離開 Settings modal，開啟既有額度面板、清除先前帳號的細節聚焦、向 Runtime Host 發出正常刷新；Provider 失聯時遵守既有錯誤與 stale 顯示。
3. 原 Settings catalog 的所有其它預覽列沒有共同能力標記。增加 `SettingsRowSupport`（`Editable`／`OperationalAction`／`ReadOnly`／`Unavailable`）和唯一的 `SettingsOperationalAction` 路由（目前只有 Manage Projects、Open Quota Overview），並為各 Settings 分類顯示能力計數。分類不授予權限；實際 Host-backed 開關仍必須 Host snapshot 已安裝、CAS 成功才會更新。

## 16 個 Classic Settings 分類的本次查核

| 類別 | 本輪可確認的 GPUI 行為 |
| --- | --- |
| Configuration Sync | Catalog 列表仍是未接線，不冒充上傳／合併入口 |
| Account | Identity／Mobile Push／Ownership 仍是未提供的預覽項 |
| Application | Git Fetch、間隔、Tray／Dock／Badge 設定有共用 Host 控制；App Language 使用全域 Host selector；Project／Workspace 移除確認和 Auto-Archive 只讀，不開放 Agent 修改；其餘未接線項清楚提示 |
| Agents | Codex／Claude Hooks 走獨立版本化 Runtime Host，Tab Titles／Status／Finished Notifications 走已核准共享設定；Skills 預覽尚未接線 |
| Quotas | **Provider Quotas 真正開啟現有額度總覽**；Claude Profiles、Credential Environment 仍標記未提供，不假裝具備憑證管理 |
| AI Assist、AI Dictation | 目前仍為 Catalog 預覽列，無相應可寫 Native handler |
| Text Actions | 預覽列，未冒充已接線的文字動作編輯器 |
| Editor | **Two-row tabs 新接通**，Syntax Theme、Tab Size、Autosave／Delay、Quick Open 排除目錄已有 Host-backed 控制；Language Intelligence、External Editors 尚待 |
| Terminal | 字型、字體大小、行高、Cursor、Theme、Opacity、TUI Scroll、Copy On Select 等原有 Host-backed 控制；PowerShell 7 執行檔與 Scrollback 預覽尚未接線 |
| Keyboard | 搜尋與重設 Shortcut 的 Catalog 行仍未接線 |
| Projects | Manage Projects 開真原生 Project/Worktree 管理入口；Per-project Settings 列目前無獨立 Handler |
| Mobile Devices、Remote Hosts | Catalog 項目未接線；不得宣稱已支援配對／SSH 修改 |
| Agent Profiles | Native 完整 Host Agent Profile 管理器已在 Settings 區另一部分；Catalog 中同名 Preview Row 尚未指向獨立操作，不能將它計成另一個按鈕 |
| History Sync | 無 Catalog 預覽列；其 Host 功能由既有獨立 Settings 子頁承擔，勿將零列等同功能缺失 |

## TDD 與已有共用 Host 契約

- **TDD RED**：先要求 `SettingsRowSupport`／`settings_row_support` 與兩項新能力契約，原始程式因缺型別／函式以 E0425／E0433 拒編譯。
- **GREEN Catalog**：`s01_every_visible_classic_row_has_one_grounded_capability` 確認 16 分類每一可見列恰有一個能力、只有 2 個真正 action、3 個安全唯讀項；Editor 6 個可編輯項中包含 Two-row tabs。
- **GREEN Shared Rust Core**：擴充 `all_connected_rows_resolve_to_the_one_rust_core_allowlist`，確認 Two-row tabs 在核准 `cli_writable` Host 路徑，所有被標 Editable 的項目必須有真正的 shared setting 或已確認 Hook／Language handler。
- **GREEN Shared Snapshot**：既有 `shared_host_snapshot_validates_quota_pin_field_without_erasing_native_only_data`，驗證 `editor.twoRowTabs=true` 的 Rust Host snapshot 正確套用到 Native 設定，以及非法字串值不部分覆蓋設定。
- **Host availability**：既有 `installed_snapshot_is_not_writable_when_runtime_host_disconnects` 確認 Host 斷線或尚未載入時 Shared Settings 不會假裝可儲存。
- **全量 Windows 回歸**：GPUI Library **569 PASS／0 FAIL／1 ignored**；Native Binary **43/43 PASS**；Windows EXE Build、`cargo fmt --check`、`git diff --check` **PASS**。隔離 Host 測試並未冒充通過。

## 尚待正常操作驗收

- 在隔離 Runtime Host 設定資料進行 `editor.twoRowTabs false → true → restart/readback → false` 的真 CAS 操作，並由 GPUI Settings 開關核對；這一輪的 Rust 契約與 Host snapshot 單測 **不等於**已取得獨立 Host CAS 實測證據。
- Windows 真 GUI 點 Editor Two-row tabs；進 Quotas→Provider Quotas，確認關 Settings 後真正開額度總覽、刷新、錯誤狀態正常。切換 Settings 類別時能力統計與「未提供」列需符合預期。
- 若要讓 Auto Archive、移除確認、PowerShell 7 路徑、其它 Classic Catalog 功能可寫，必須先完成 Rust Host 明確的設定契約與使用者確認規則；不透過 Agent 或 Native 本地 JSON 繞過保護。

## 安全與下一項

- 沒有對正式 Classic Runtime Host 設定寫入；本輪只修改 GPUI 整合分支程式與回歸，既有 `.gpui-center-diff-smoke/` 與 `desktop_gpui/target/` 保留。
- **S01 核心 INTEGRATED／待 GUI 與真 Host CAS 驗收；主線 NOW S02**：盤點 Classic／GPUI 仍重複或落在 Native-only 的真正共用設定，統一遷移到 Rust Core／Host。S03 再做雙 GUI 同時開啟及重啟同步；S04 跟進設定 i18n。
- `main` 不修改、不 Push／部署／Release，`supported=false` 維持。
