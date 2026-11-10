//! GPUI Kit owns all generic button, dropdown and modal presentation.
//! Keep only the Alera-specific styled Quick Open result row and small
//! adapters that preserve the Classic UX vocabulary.

pub(super) use crate::native_ui_icons::{icon, icon_button, UiIcon};

use alera_gpui_desktop::visual_tokens::{ACTIVE, TEXT};
use gpui::{div, prelude::*, px, rgb, ElementId, SharedString, Stateful};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::IconName;

/// A menu item is a full-width hit target. Active/selected items get a
/// tokenized check mark; callers attach the strongly typed action callback.
pub(super) fn menu_item(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    selected: bool,
    destructive: bool,
) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .w_full()
        .h(px(32.0))
        .px_2()
        .flex()
        .items_center()
        .gap_2()
        .cursor_pointer()
        .rounded_sm()
        .hover(|style| style.bg(rgb(ACTIVE)))
        .text_sm()
        .text_color(rgb(if destructive { 0xe38383 } else { TEXT }))
        .child(div().w(px(16.0)).child(if selected { "✓" } else { "" }))
        .child(label.into())
}

/// Compact dropdown/select trigger: a visible value and chevron, not a
/// standalone text label that merely happens to reveal another panel.
pub(super) fn select_trigger(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    expanded: bool,
) -> Button {
    let button = Button::new(id).label(label).icon(IconName::ChevronDown);
    if expanded {
        button.secondary()
    } else {
        button.outline()
    }
}

pub(super) fn action_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
    destructive: bool,
) -> Button {
    let button = Button::new(id).label(label);
    if destructive {
        button.danger()
    } else if primary {
        button.primary()
    } else {
        button.outline()
    }
}
