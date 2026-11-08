//! Shared Native/GPUI counterpart of Classic's AleraDialog and
//! AleraDialogHeader. Callers own the content and dismissal action; this
//! component owns the dimmed backdrop, bounded frame and consistent header.
//! Reuse it for settings, confirmation dialogs and future pickers.

use alera_gpui_desktop::native_modal::NativeModalDimensions;
use alera_gpui_desktop::visual_tokens::{BORDER, TEXT, TOOLBAR};
use gpui::{div, prelude::*, px, rgb, App, MouseButton, MouseDownEvent, Window};

pub(super) struct AleraNativeModal {
    dimensions: NativeModalDimensions,
}

impl AleraNativeModal {
    pub(super) fn new(dimensions: NativeModalDimensions) -> Self {
        Self { dimensions }
    }

    /// The backdrop is a real top-layer pointer shield: a click outside
    /// dismisses, while the dialog frame stops bubbling so its controls work.
    pub(super) fn overlay(
        &self,
        content: impl IntoElement,
        dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id("alera-modal-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x000000ad))
            .on_mouse_down(MouseButton::Left, dismiss)
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(content)
    }

    pub(super) fn frame(&self, content: impl IntoElement) -> gpui::Stateful<gpui::Div> {
        div()
            .id("alera-modal-frame")
            .w(px(self.dimensions.width))
            .h(px(self.dimensions.height))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .rounded_lg()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(TOOLBAR))
            .text_color(rgb(TEXT))
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .child(content)
    }

    /// AleraDialogHeader parity: title and only one icon-only close affordance.
    pub(super) fn header(
        &self,
        title: impl Into<gpui::SharedString>,
        close_button: impl IntoElement,
    ) -> gpui::Div {
        div()
            .h(px(54.0))
            .flex()
            .items_center()
            .justify_between()
            .px_4()
            .border_b_1()
            .border_color(rgb(BORDER))
            .child(
                div()
                    .text_sm()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(title.into()),
            )
            .child(close_button)
    }
}
