//! UI-agnostic popup state. Only one transient menu may be open at once.
//! Position is in GPUI window coordinates; GPUI anchored() handles fitting.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePopupKind {
    ExplorerContext,
    ExplorerVisibility,
    GitMore,
    GitBranch,
    TerminalAttach,
    TerminalSize,
    MixedTabContext,
    TerminalTabContext,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NativePopup {
    pub kind: NativePopupKind,
    pub x: f32,
    pub y: f32,
    pub selected: usize,
}

#[derive(Clone, Debug, Default)]
pub struct NativePopupState {
    popup: Option<NativePopup>,
}

impl NativePopupState {
    pub fn active(&self) -> Option<NativePopup> {
        self.popup
    }

    pub fn is_open(&self, kind: NativePopupKind) -> bool {
        self.popup.is_some_and(|popup| popup.kind == kind)
    }

    pub fn show(&mut self, kind: NativePopupKind, x: f32, y: f32) {
        self.popup = Some(NativePopup {
            kind,
            x: if x.is_finite() { x.max(0.0) } else { 0.0 },
            y: if y.is_finite() { y.max(0.0) } else { 0.0 },
            selected: 0,
        });
    }

    pub fn toggle(&mut self, kind: NativePopupKind, x: f32, y: f32) -> bool {
        if self.is_open(kind) {
            self.dismiss();
            false
        } else {
            self.show(kind, x, y);
            true
        }
    }

    pub fn dismiss(&mut self) {
        self.popup = None;
    }

    pub fn navigate(&mut self, delta: isize, count: usize) -> Option<usize> {
        let popup = self.popup.as_mut()?;
        if count == 0 {
            return None;
        }
        popup.selected = (popup.selected as isize + delta).rem_euclid(count as isize) as usize;
        Some(popup.selected)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_menu_can_be_active_and_same_trigger_toggles_closed() {
        let mut state = NativePopupState::default();
        assert!(state.toggle(NativePopupKind::ExplorerVisibility, 120.0, 45.0));
        assert!(state.is_open(NativePopupKind::ExplorerVisibility));
        assert!(state.toggle(NativePopupKind::GitMore, 900.0, 80.0));
        assert!(!state.is_open(NativePopupKind::ExplorerVisibility));
        assert!(state.is_open(NativePopupKind::GitMore));
        assert!(!state.toggle(NativePopupKind::GitMore, 900.0, 80.0));
        assert!(state.active().is_none());
    }

    #[test]
    fn menu_keyboard_selection_wraps_and_dismissal_invalidates_it() {
        let mut state = NativePopupState::default();
        state.show(NativePopupKind::ExplorerContext, f32::NAN, -9.0);
        assert_eq!(
            (state.active().unwrap().x, state.active().unwrap().y),
            (0.0, 0.0)
        );
        assert_eq!(state.navigate(-1, 4), Some(3));
        assert_eq!(state.navigate(1, 4), Some(0));
        assert_eq!(state.navigate(1, 0), None);
        state.dismiss();
        assert_eq!(state.navigate(1, 4), None);
    }
}
