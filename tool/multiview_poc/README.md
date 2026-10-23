# Alera shared-engine multi-window PoC

**Windows 目前預設啟動方案及強制回退操作：** [DEFAULT_WINDOWS_SHARED_ENGINE.md](DEFAULT_WINDOWS_SHARED_ENGINE.md)。Windows 一般啟動使用共用 Engine 管理 Editor 視窗；若要完整回退至原多程序行為，先 Quit 再用 `--alera-legacy-detached-tabs` 啟動。其他分頁類型仍保留舊流程。以下為歷史 PoC 與後續驗證材料。

Windows Flutter standalone spike. See [DEEP_TEST_RESULTS.md](DEEP_TEST_RESULTS.md) for the latest 200-cycle, Release/AOT, plugin, DPI, focus and window-lifecycle evidence; [POC_NOTES.md](POC_NOTES.md) retains the first PoC's results. Do not merge the PoC runner into the production Alera runner without compatibility tests.

**Second-stage handoff tests:** [TAB_HANDOFF_INTEGRATION_RESULTS.md](TAB_HANDOFF_INTEGRATION_RESULTS.md) covers a view-ID-scoped window adapter, 20 native child detach/dock cycles, preserved unsaved Editor state and persistent test shell PID, actual Windows WM_CLOSE observer recovery, and Release/AOT measurements. This remains a synthetic editor/piped terminal PoC, not Alera production integration.
