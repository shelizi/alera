//! Shared Native translation boundary.
//! Texts are taken from the Classic Alera localization vocabulary by
//! tool/generate_settings_locales.py. New pages can reuse tr() and UiLocale.
//!
//! Locale is passed explicitly to avoid global-state cross-window leaks;
//! when GPUI Kit owns more of the surface, synchronize component-level locale
//! separately and notify/repaint the owning view.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum UiLocale {
    #[default]
    English,
    TraditionalChinese,
    SimplifiedChinese,
    Japanese,
}

impl UiLocale {
    pub fn from_preference(raw: &str) -> Self {
        match raw {
            "english" => Self::English,
            "traditionalChinese" => Self::TraditionalChinese,
            "simplifiedChinese" => Self::SimplifiedChinese,
            "japanese" => Self::Japanese,
            "system" => Self::from_tag(&system_locale_name()),
            _ => Self::English,
        }
    }

    pub const ALL: [Self; 4] = [
        Self::English,
        Self::TraditionalChinese,
        Self::SimplifiedChinese,
        Self::Japanese,
    ];

    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::TraditionalChinese => "zh-TW",
            Self::SimplifiedChinese => "zh-CN",
            Self::Japanese => "ja",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::TraditionalChinese => "繁體中文",
            Self::SimplifiedChinese => "简体中文",
            Self::Japanese => "日本語",
        }
    }

    pub fn from_tag(raw: &str) -> Self {
        let tag = raw
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .replace('_', "-");
        let tag = tag.to_ascii_lowercase();
        match tag.as_str() {
            "zh-tw" | "zh-hant" | "zh-hant-tw" | "zh-hk" | "zh-mo" => Self::TraditionalChinese,
            "zh-cn" | "zh-hans" | "zh-hans-cn" | "zh-sg" | "zh" => Self::SimplifiedChinese,
            "ja" | "ja-jp" => Self::Japanese,
            _ => Self::English,
        }
    }
}

/// Resolve the same OS language as Classic for the persisted 'system' mode.
fn system_locale_name() -> String {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultLocaleName(name: *mut u16, count: i32) -> i32;
        }
        let mut buffer = [0u16; 85];
        let count = unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), buffer.len() as i32) };
        if count > 1 {
            return String::from_utf16_lossy(&buffer[..count as usize - 1]);
        }
    }
    std::env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_else(|| "en".into())
}

/// Translation of a Classic source string, with English as final fallback.
/// Callers should pass locale from the view state, not mutate global locale.
pub fn tr(locale: UiLocale, source: &str) -> String {
    rust_i18n::t!(source, locale = locale.tag()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_settings_terms_translate_across_all_supported_locales() {
        assert_eq!(tr(UiLocale::English, "Settings"), "Settings");
        assert_eq!(tr(UiLocale::TraditionalChinese, "Settings"), "設定");
        assert_eq!(tr(UiLocale::SimplifiedChinese, "Settings"), "设置");
        assert_eq!(tr(UiLocale::Japanese, "Settings"), "設定");
        assert_eq!(tr(UiLocale::TraditionalChinese, "Application"), "應用程式");
        assert_eq!(tr(UiLocale::Japanese, "Terminal"), "ターミナル");
        assert_eq!(
            tr(UiLocale::TraditionalChinese, "No matching settings."),
            "沒有符合的設定。"
        );
        assert_eq!(
            tr(
                UiLocale::Japanese,
                "UI preview only · controls are not connected to settings"
            ),
            "UI プレビューのみ · 設定への連携は未実装です"
        );
        assert_eq!(
            tr(UiLocale::TraditionalChinese, "Connected Native Controls"),
            "已連接的 Native 設定"
        );
    }

    #[test]
    fn locale_variants_and_unknown_sources_fall_back_safely() {
        assert_eq!(
            UiLocale::from_preference("traditionalChinese"),
            UiLocale::TraditionalChinese
        );
        assert_eq!(UiLocale::from_preference("japanese"), UiLocale::Japanese);
        assert_eq!(UiLocale::from_preference("unknown"), UiLocale::English);
        assert_eq!(
            UiLocale::from_tag("zh_Hant_TW"),
            UiLocale::TraditionalChinese
        );
        assert_eq!(
            UiLocale::from_tag("zh_CN.UTF-8"),
            UiLocale::SimplifiedChinese
        );
        assert_eq!(UiLocale::from_tag("ja_JP"), UiLocale::Japanese);
        assert_eq!(UiLocale::from_tag("de-DE"), UiLocale::English);
        assert_eq!(
            tr(UiLocale::TraditionalChinese, "Alera-unknown-key-927"),
            "Alera-unknown-key-927"
        );
    }
}
