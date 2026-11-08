//! Reusable GPUI building blocks following Classic's design-system tokens.
//! Use GPUI's own anchored()/deferred() for native layout/overlay primitives,
//! rather than one-off absolute-positioned rows or fake in-panel menus.
//!
//! Existing modal / icon implementations are re-exported here so new
//! screens can import one Native component library.

pub(super) use crate::native_modal_ui::AleraNativeModal;
pub(super) use crate::native_ui_icons::{icon, icon_button, UiIcon};

use alera_gpui_desktop::visual_tokens::{ACTIVE, BORDER, ELEVATED, MUTED, TEXT};
use gpui::{div, point, prelude::*, px, rgb, ElementId, IntoElement, SharedString, Stateful};
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::IconName;

pub(super) fn menu_surface(
    width: f32,
    title: Option<impl Into<SharedString>>,
    content: impl IntoElement,
) -> Stateful<gpui::Div> {
    div()
        .id("alera-popup-surface")
        .w(px(width))
        .max_h(px(380.0))
        .overflow_y_scroll()
        .rounded_md()
        .border_1()
        .border_color(rgb(BORDER))
        .bg(rgb(ELEVATED))
        .p_1()
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| {
            cx.stop_propagation();
        })
        .children(title.map(|title| {
            div()
                .px_2()
                .py_2()
                .border_b_1()
                .border_color(rgb(BORDER))
                .text_xs()
                .text_color(rgb(MUTED))
                .child(title.into())
        }))
        .child(content)
}

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

pub(super) fn menu_separator() -> gpui::Div {
    div().mx_2().my_1().h(px(1.0)).bg(rgb(BORDER))
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

pub(super) fn popup_at(x: f32, y: f32, content: impl IntoElement) -> gpui::Deferred {
    gpui::deferred(
        gpui::anchored()
            .position(point(px(x), px(y)))
            .snap_to_window()
            .child(content),
    )
    .with_priority(100)
}
