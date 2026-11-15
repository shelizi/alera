# Alera GPUI S04 — Settings i18n：Classic 字典／Runtime Host 語言／原生操作介面

日期：2026-11-13（Asia/Taipei）
整合分支：`integrate/gpui-central-diff-into-main-20261112`；起始 HEAD `93df9dc11`。
狀態：**Settings 字典與可見操作文字核心已接線；實際 Windows 雙 GUI 滑鼠／鍵盤切換待驗；`supported=false`。**

## 現況盤點

- 原生 Settings 的 Classic Catalog 16 類導航／分組／項目名稱與說明已有 `tr(locale, ...)`，語言 `general.language` 經 Rust Runtime Host CAS 儲存並於 `install_shared_desktop_snapshot` 改寫 `settings_menu_language`、`settings_menu_locale`，同步更新搜尋框提示。
- 原 Settings 操作列仍有英文硬編碼：不可用提示、Host 讀取中／離線、Projects／Quota action 文字、唯讀安全設定啟用狀態、封存天數、Quick Open 排除目錄提示／按鈕、S01 新增的 Settings 能力統計。
- 舊字典產生器只擷取 `settings_menu_catalog.rs`，沒有掃描真正 `native_settings_catalog_ui.rs` 的 `tr()` 字串；故即使 UI 以 `tr` 包裹，仍可能回退英文。

## 修改內容

1. `native_settings_catalog_ui.rs`：所有上述日常操作文字改由本地 `UiLocale` 的 `tr()` 產生，Host 安全唯讀狀態使用翻譯過的 `Enabled／Disabled`、`days (0 disables)`、`Rust Host` 與「請在 Classic 修改」提示；Projects／Provider Quotas 按鈕及說明同樣翻譯。
2. `settings_support_summary`：先對 `Editable／Actions／Read-only／Not available` 各自翻譯，再帶入實際可用／只讀／未提供的動態計數，切換繁中與英文時不留下混用標籤或硬編英文。
3. `tool/generate_settings_locales.py`：維持原 Classic 詞彙來源，另掃描 Settings UI 中 `tr(locale, "...")`／`tr(self.settings_menu_locale, "...")` 的具體字串，以及動態分支會使用的語彙；避免依賴不可審核的手改產出檔。
4. `locales/settings_overrides.json`：新增主要 Host、Actions、Quota、Read-only、安全狀態、Directory 排除等 **19 個 UI/操作鍵** 的繁中／簡中／日文翻譯，同時補齊五個原本無翻譯的 Classic Catalog 項目（History Sync、Agent Status/Finished、Tray Badge）。`locales/ui.yml` 由產生器更新，未直接編輯生成檔。

## TDD 與翻譯完整度

- **RED**：新增 `native_i18n::tests::s04_shared_settings_operational_labels_have_real_translations`，起始測試因 `Not available in GPUI yet` 仍回英文而失敗。補字典及 UI 接線後 **GREEN 1/1 PASS**（十個關鍵英文／繁中用語，英文原樣）。
- **Native GREEN**：`native_settings_catalog_ui::tests::s04_settings_capability_summary_follows_host_locale` **1/1 PASS**，確保 `Editable: 6...` 與「可編輯設定: 6...」隨 locale 重新組合，而非固定 UI 字串。
- `python tool/generate_settings_locales.py --check` 檢查來源與生成的 `locales/ui.yml` 一致。
- 翻譯覆蓋：**繁中 238/238、簡中 238/238、日文 238/238；0 英文 fallback**。這指的是本次 Settings Catalog＋明列 Native Settings UI 詞彙，不代表整個 Alera GPUI 所有畫面已翻譯。
- **Windows 全量回歸**：GPUI Library **572 PASS／0 FAIL／1 ignored**；Native Binary **44/44 PASS**；Windows EXE Build、`cargo fmt --check`、locale generator `--check`、`git diff --check` 全部 **PASS**。隔離 Host 原有 ignored 測試不冒充 PASS。

## 選項儲存與 GUI 待驗

- 原有語言 selector `system／english／traditionalChinese／simplifiedChinese／japanese` 仍使用 `change_shared_desktop_setting("general.language",...)`，由 Rust Host CAS→Snapshot readback 驅動 UI locale，不再增設 UI-only JSON 設定來源。
- S03 已以真隔離 Runtime Host 驗證 `general.language` 兩方向 CAS 與重啟持久化；S04 新測試驗證翻譯鍵與 Native 狀態組合，但**未實際以 Classic＋GPUI 兩個 GUI 視窗點選語言或重啟截圖**，不宣稱 GUI DONE。
- 設定頁其餘非日常功能仍可能有未包 `tr` 的長段英文提示、特別是 Agent Profiles 的高階編輯區；此包只保證被盤點的 Settings Catalog／日常操作文字，不能推論全站零英文。後續 i18n 回歸或 U 系列 UI 收斂可再補。

## 安全與下一項

- 不寫入正式使用者 Settings 或 Classic Runtime Host，不合 `main`，不 Push／部署／Release；原有受保護 smoke 工作樹與 target 不清除。
- **S04 核心 INTEGRATED／GUI 待驗 → NOW H01 Agent Hook 四態狀態顯示**。`supported=false` 不變。
