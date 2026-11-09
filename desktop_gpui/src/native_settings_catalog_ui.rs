//! Classic Settings catalog presentation on the GPUI Kit workbench.
//! This layer is deliberately inert: real Native controls remain in
//! native_settings_ui.rs, and catalog previews never touch preferences/Host.
use super::*;
use alera_gpui_desktop::native_i18n::{tr, UiLocale};
use alera_gpui_desktop::settings_menu_catalog::{PreviewControl, PreviewRow, SettingsSection};

fn render_preview_control(kind: PreviewControl, locale: UiLocale) -> gpui::AnyElement {
    match kind {
        PreviewControl::Toggle => div()
            .w(px(38.))
            .h(px(22.))
            .rounded_full()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(ACTIVE))
            .flex()
            .items_center()
            .px_1()
            .child(div().size(px(14.)).rounded_full().bg(rgb(MUTED)))
            .into_any_element(),
        PreviewControl::Select => div()
            .w(px(150.))
            .h(px(30.))
            .px_3()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(TOOLBAR))
            .rounded_md()
            .flex()
            .items_center()
            .justify_between()
            .text_xs()
            .text_color(rgb(MUTED))
            .child(tr(locale, "Select…"))
            .child("⌄")
            .into_any_element(),
        PreviewControl::Input => div()
            .w(px(150.))
            .h(px(30.))
            .px_3()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(TOOLBAR))
            .rounded_md()
            .flex()
            .items_center()
            .text_xs()
            .text_color(rgb(MUTED))
            .child("—")
            .into_any_element(),
        PreviewControl::Action => div()
            .h(px(30.))
            .px_3()
            .border_1()
            .border_color(rgb(BORDER))
            .bg(rgb(TOOLBAR))
            .rounded_md()
            .flex()
            .items_center()
            .text_xs()
            .text_color(rgb(MUTED))
            .child(tr(locale, "Open"))
            .into_any_element(),
    }
}

fn render_preview_row(row: &PreviewRow, row_index: usize, locale: UiLocale) -> gpui::AnyElement {
    div()
        .id(("native-setting-preview-row", row_index))
        .px_4()
        .py_3()
        .flex()
        .items_center()
        .gap_4()
        .border_b_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(TEXT))
                        .child(tr(locale, row.title)),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(MUTED))
                        .child(tr(locale, row.detail)),
                ),
        )
        .child(render_preview_control(row.control, locale))
        .into_any_element()
}

impl Workbench {
    /// All Classic Settings categories are visible, including those without
    /// a Native preference backing yet. No row action is attached here.
    pub(super) fn render_settings_catalog_preview(
        &self,
        selected: &'static SettingsSection,
        locale: UiLocale,
    ) -> gpui::AnyElement {
        div()
            .id("native-settings-catalog-groups")
            .flex()
            .flex_col()
            .gap_3()
            .children(
                selected
                    .groups
                    .iter()
                    .enumerate()
                    .map(|(group_idx, group)| {
                        div()
                            .id(("native-settings-catalog-group", group_idx))
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(BORDER))
                            .bg(rgb(PANEL))
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .px_4()
                                    .py_3()
                                    .text_sm()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .child(tr(locale, group.title)),
                            )
                            .children(group.rows.iter().enumerate().map(|(row_idx, row)| {
                                render_preview_row(row, group_idx * 100 + row_idx, locale)
                            }))
                    }),
            )
            .into_any_element()
    }

    /// Exactly the five Classic AppLanguage values. Selection is persisted
    /// through Rust Host CAS and updates Classic on its next change event.
    pub(super) fn render_settings_language_preview(
        &self,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let selected = self.settings_menu_language.as_str();
        div()
            .id("native-settings-language-preview")
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_2()
            .border_b_1()
            .border_color(rgb(BORDER))
            .children(
                [
                    ("system", "System"),
                    ("english", "EN"),
                    ("traditionalChinese", "繁中"),
                    ("simplifiedChinese", "简中"),
                    ("japanese", "日本語"),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (preference, label))| {
                    div()
                        .id(("native-settings-locale-option", index))
                        .cursor_pointer()
                        .rounded_md()
                        .px_2()
                        .py_1()
                        .text_xs()
                        .text_color(rgb(if selected == preference { TEXT } else { MUTED }))
                        .bg(rgb(if selected == preference {
                            ACTIVE
                        } else {
                            PANEL
                        }))
                        .hover(|style| style.bg(rgb(ACTIVE)))
                        .child(label)
                        .on_click(cx.listener(move |view, _, _, cx| {
                            view.change_shared_desktop_setting(
                                "general.language",
                                serde_json::json!(preference),
                                cx,
                            );
                        }))
                }),
            )
            .into_any_element()
    }
}
