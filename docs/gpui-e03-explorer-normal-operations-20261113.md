# GPUI E03 — Explorer 日常檔案操作整合與驗證

日期：2026-11-13（Asia/Taipei）
工作樹：`integrate/gpui-central-diff-into-main-20261112`
狀態：**正常功能程式已接線／Windows Rust 測試與 EXE Build PASS；Windows GUI 滑鼠操作及 OS 外部拖出仍待驗收；`supported=false`。**

## 正常使用缺口與修正

Explorer 原本已具備檔案／資料夾 New、Rename、Copy、Cut、Paste、Duplicate、Move、內部群組拖曳及從 Windows File Manager 拖入檔案等流程，無須重造。修正兩組會影響一般使用的阻擋：

1. Explorer `Ctrl+C / Ctrl+X / Ctrl+V` 的本地剪貼簿 Paste 原本只要有任何 Dirty Editor 即全面拒絕。現移除無關的全域 Dirty 限制：`Copy` 對磁碟來源進行複製，允許無關 Editor 有未儲存內容；`Cut` 必須在即將移動的來源檔案／資料夾沒有被任何主／副窗格 Editor 開啟時才能貼上。檔案已在 Editor 開啟則拒絕且不消耗 clipboard、不移動來源。
2. Explorer 右鍵 `Copy into folder`、`Duplicate` 原本也會因主 Editor 有未儲存內容遭整體拒絕；現允許複製磁碟資料。右鍵 `Rename`／`Move` 則檢查來源是否屬於主／副窗格的任何開啟檔案或其父資料夾，只有會破壞現有 Editor 路徑的操作才拒絕，其餘無關 Dirty Buffer 不阻塞。

安全行為：沒有改變 overwrite／Git metadata／workspace boundary 限制，Cut 失敗保留原始檔與 Clipboard；未經明確使用者操作不會自動改寫任何檔案。Copy/Duplicate 複製的是磁碟已存版本，而非某個未儲存的 Editor 草稿。

## TDD 與 Windows 驗證

- `explorer_ops::tests::e03_explorer_copy_can_paste_with_dirty_editor_while_cut_protects_open_tabs` **1/1 PASS**：在 tempfile Workspace 建立檔案／資料夾，測試 Copy 可與開啟檔案清單共存；Cut 開啟來源或包含開啟子檔的資料夾被拒且 Clipboard 保留；移除該來源的 Editor 持有狀態後成功搬移。
- `explorer_ops::tests::`（含新案例）**23/23 PASS**；`explorer_selection::tests::` **7/7 PASS**；`explorer_drag::tests::` **3/3 PASS**，涵蓋常用檔案操作、多選、拖曳與外部檔案匯入。
- GPUI Native Binary **36/36 PASS**；Windows 原生 `cargo check --bin alera-gpui-desktop` **PASS**；Windows `cargo build --bin alera-gpui-desktop` **PASS**；`cargo fmt` 與 `git diff --check` 通過。
- 僅使用 tempfile 測試資料；沒有修改正式 Host、登入 Provider、使用者檔案或受保護的 smoke fixture。

## 尚未完成的 GUI／跨程式操作

- Windows Explorer 實際右鍵／快捷鍵、多選／拖曳及實際變更後樹狀列表同步尚未做圖形操作驗收；本文件不聲稱 Windows GUI PASS。
- 本地 Explorer Copy/Cut Clipboard 與 Windows OS Clipboard 的雙向傳遞仍待規格與操作驗收；GPUI 內部拖曳與外部拖入已有程式，**GPUI 拖出檔案到其他程式**尚未驗證有原生 OS file-drop 支援。兩者列入後續功能缺口，不誤標 E03 100% Classic parity。
- 基本正常功能已整合進 GPUI integration；原隊列按功能完整性優先移到 **E04：Quick Open／Explorer 篩選與內容搜尋**，GUI parity 缺口另追，不阻塞日常功能開發。仍 `supported=false`、不合 `main`／Push／部署／Release。
