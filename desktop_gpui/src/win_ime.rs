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

/// Best-effort IMM-compatible indicator for the focused GPUI window.
/// Only a confirmed open IME in native conversion mode may suppress the
/// first ASCII phonetic KeyDown. Unknown/English modes preserve PTY typing.
fn imm_native_text_mode(open: bool, conversion: Option<u32>) -> bool {
    const IME_CMODE_NATIVE: u32 = 0x0001;
    const IME_CMODE_NOCONVERSION: u32 = 0x0100;
    open && conversion
        .is_some_and(|mode| mode & IME_CMODE_NATIVE != 0 && mode & IME_CMODE_NOCONVERSION == 0)
}

#[cfg(windows)]
pub(crate) fn focused_native_text_mode() -> bool {
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
        fn ImmGetOpenStatus(context: Handle) -> i32;
        fn ImmGetConversionStatus(context: Handle, conversion: *mut u32, sentence: *mut u32)
            -> i32;
        fn ImmReleaseContext(window: Handle, context: Handle) -> i32;
    }
    // Query only: never switch keyboard layout or alter unrelated windows.
    unsafe {
        let focus = GetFocus();
        let foreground = GetForegroundWindow();
        if focus.is_null() || foreground.is_null() || GetAncestor(focus, 2) != foreground {
            return false;
        }
        let context = ImmGetContext(focus);
        if context.is_null() {
            return false;
        }
        let open = ImmGetOpenStatus(context) != 0;
        let mut mode = 0u32;
        let conversion = if ImmGetConversionStatus(context, &mut mode, std::ptr::null_mut()) != 0 {
            Some(mode)
        } else {
            None
        };
        let _ = ImmReleaseContext(focus, context);
        imm_native_text_mode(open, conversion)
    }
}

#[cfg(not(windows))]
pub(crate) fn focused_native_text_mode() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_mode_only_uses_confirmed_open_conversion() {
        assert!(imm_native_text_mode(true, Some(0x0001)));
        assert!(!imm_native_text_mode(true, Some(0x0000)));
        assert!(!imm_native_text_mode(false, Some(0x0001)));
        assert!(!imm_native_text_mode(true, None));
        assert!(!imm_native_text_mode(true, Some(0x0101)));
    }

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
