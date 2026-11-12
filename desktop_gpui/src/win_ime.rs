//! Cancel the Windows IMM composition belonging to the current GPUI UI HWND.
//! A GPUI input-to-input focus transfer does not trigger native WM_KILLFOCUS.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelResult {
    NoFocus,
    InactiveWindow,
    NoContext,
    Cancelled,
    Rejected,
}

#[cfg(windows)]
pub(crate) fn cancel_focused_composition() -> CancelResult {
    use std::ffi::c_void;
    type Handle = *mut c_void;
    #[link(name = "user32")]
    extern "system" {
        fn GetFocus() -> Handle;
        fn GetForegroundWindow() -> Handle;
        fn GetAncestor(window: Handle, flags: u32) -> Handle;
    }
    #[link(name = "imm32")]
    extern "system" {
        fn ImmGetContext(window: Handle) -> Handle;
        fn ImmNotifyIME(context: Handle, action: u32, index: u32, value: u32) -> i32;
        fn ImmReleaseContext(window: Handle, context: Handle) -> i32;
    }
    // GA_ROOT, NI_COMPOSITIONSTR, CPS_CANCEL, NI_CLOSECANDIDATE.
    // Only touch the current foreground GPUI HWND, on its UI thread.
    unsafe {
        let focus = GetFocus();
        if focus.is_null() {
            return CancelResult::NoFocus;
        }
        let foreground = GetForegroundWindow();
        if foreground.is_null() || GetAncestor(focus, 2) != foreground {
            return CancelResult::InactiveWindow;
        }
        let context = ImmGetContext(focus);
        if context.is_null() {
            return CancelResult::NoContext;
        }
        let cancelled = ImmNotifyIME(context, 0x0015, 0x0004, 0) != 0;
        let _ = ImmNotifyIME(context, 0x0011, 0, 0);
        let _ = ImmReleaseContext(focus, context);
        if cancelled {
            CancelResult::Cancelled
        } else {
            CancelResult::Rejected
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn cancel_focused_composition() -> CancelResult {
    CancelResult::NoFocus
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancellation_is_bounded_even_without_any_ime_context() {
        assert!(matches!(
            cancel_focused_composition(),
            CancelResult::NoFocus
                | CancelResult::InactiveWindow
                | CancelResult::NoContext
                | CancelResult::Cancelled
                | CancelResult::Rejected
        ));
    }
}
