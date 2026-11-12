//! Every word Lumina shows, in each language it speaks.
//!
//! Strings live in one table per language rather than scattered through the
//! interface code, and the table is a plain struct: a missing translation is
//! a compile error, never a blank label at run time.

use lumina_store::Language;

/// A language the interface is available in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lang {
    Turkish,
    English,
}

impl Lang {
    /// The language to use, given the user's setting and the desktop's locale
    /// variables (looked up through `variable`).
    pub fn resolve(setting: Language, variable: impl Fn(&str) -> Option<String>) -> Self {
        match setting {
            Language::Turkish => Self::Turkish,
            Language::English => Self::English,
            Language::System => {
                // The order POSIX gives these variables.
                let locale = ["LC_ALL", "LC_MESSAGES", "LANG"]
                    .into_iter()
                    .find_map(|name| variable(name).filter(|value| !value.is_empty()));
                match locale {
                    Some(locale) if locale.to_ascii_lowercase().starts_with("tr") => Self::Turkish,
                    _ => Self::English,
                }
            }
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Self::Turkish => &TURKISH,
            Self::English => &ENGLISH,
        }
    }
}

pub struct Strings {
    pub back: &'static str,
    pub forward: &'static str,
    pub reload: &'static str,
    pub settings: &'static str,
    pub address_hint: &'static str,
    pub address_label: &'static str,

    pub settings_title: &'static str,
    pub return_to_page: &'static str,
    pub search_engine: &'static str,
    pub search_engine_note: &'static str,
    pub start_page: &'static str,
    pub start_with_search_engine: &'static str,
    pub start_with_address: &'static str,
    pub start_address_hint: &'static str,
    pub start_address_invalid: &'static str,
    pub appearance: &'static str,
    pub appearance_system: &'static str,
    pub appearance_light: &'static str,
    pub appearance_dark: &'static str,
    pub accent: &'static str,
    pub accent_brick: &'static str,
    pub accent_indigo: &'static str,
    pub accent_olive: &'static str,
    pub language: &'static str,
    pub language_system: &'static str,
    pub stored_in: &'static str,
    pub not_stored: &'static str,
    pub save_failed: &'static str,

    pub problem_unreadable: &'static str,
    pub problem_syntax: &'static str,
    pub problem_value: &'static str,
    pub line: &'static str,

    pub welcome: &'static str,
    pub welcome_lead: &'static str,
    pub welcome_caveat: &'static str,
    pub begin: &'static str,

    pub page_crashed: &'static str,
    pub page_crashed_note: &'static str,
}

const TURKISH: Strings = Strings {
    back: "Geri",
    forward: "İleri",
    reload: "Yenile",
    settings: "Ayarlar",
    address_hint: "Ara ya da adres yaz",
    address_label: "Adres",

    settings_title: "Ayarlar",
    return_to_page: "Sayfaya dön",
    search_engine: "Arama motoru",
    search_engine_note: "Adres çubuğuna yazdığınız, adres olmayan her şey burada aranır.",
    start_page: "Açılış sayfası",
    start_with_search_engine: "Arama motorunun ana sayfası",
    start_with_address: "Belirlediğim adres",
    start_address_hint: "https://…",
    start_address_invalid: "Bu, açılabilecek bir adres değil. Şimdilik arama motoru açılacak.",
    appearance: "Görünüm",
    appearance_system: "Sistemle aynı",
    appearance_light: "Açık",
    appearance_dark: "Koyu",
    accent: "Vurgu rengi",
    accent_brick: "Kiremit",
    accent_indigo: "Çivit",
    accent_olive: "Zeytin",
    language: "Dil",
    language_system: "Sistemle aynı",
    stored_in: "Ayarlarınız bu dosyada saklanır:",
    not_stored: "Ayar klasörü bulunamadığı için seçimleriniz bu oturumdan sonra kaybolacak.",
    save_failed: "Ayarlar kaydedilemedi:",

    problem_unreadable: "Ayar dosyası okunamadı; varsayılanlar kullanılıyor.",
    problem_syntax: "Ayar dosyası bozuk olduğu için hiçbir ayar okunamadı; varsayılanlar kullanılıyor.",
    problem_value: "Bu ayarın değeri anlaşılamadı, varsayılanı kullanılıyor:",
    line: "satır",

    welcome: "Lumina'ya hoş geldiniz",
    welcome_lead: "Başlamadan önce birkaç tercih. Hepsini sonradan Ayarlar'dan değiştirebilirsiniz.",
    welcome_caveat: "Lumina kendi yolunu çizen genç bir tarayıcı. Bazı siteler henüz düzgün \
                     görünmeyebilir; Netflix gibi korumalı yayın hizmetleri çalışmaz.",
    begin: "Başla",

    page_crashed: "Bu sayfa çalışmayı durdurdu",
    page_crashed_note: "Yeniden yüklemeyi deneyebilirsiniz.",
};

const ENGLISH: Strings = Strings {
    back: "Back",
    forward: "Forward",
    reload: "Reload",
    settings: "Settings",
    address_hint: "Search or type an address",
    address_label: "Address",

    settings_title: "Settings",
    return_to_page: "Return to page",
    search_engine: "Search engine",
    search_engine_note: "Anything you type in the address bar that is not an address is searched for here.",
    start_page: "Start page",
    start_with_search_engine: "The search engine's front page",
    start_with_address: "An address I choose",
    start_address_hint: "https://…",
    start_address_invalid: "That is not an address Lumina can open. The search engine will open for now.",
    appearance: "Appearance",
    appearance_system: "Same as system",
    appearance_light: "Light",
    appearance_dark: "Dark",
    accent: "Accent colour",
    accent_brick: "Brick",
    accent_indigo: "Indigo",
    accent_olive: "Olive",
    language: "Language",
    language_system: "Same as system",
    stored_in: "Your settings are kept in this file:",
    not_stored: "No settings folder could be found, so your choices will be lost after this session.",
    save_failed: "Settings could not be saved:",

    problem_unreadable: "The settings file could not be read; defaults are in use.",
    problem_syntax: "The settings file is damaged, so no settings could be read; defaults are in use.",
    problem_value: "This setting's value was not understood, so its default is in use:",
    line: "line",

    welcome: "Welcome to Lumina",
    welcome_lead: "A few choices before you begin. You can change all of them later in Settings.",
    welcome_caveat: "Lumina is a young browser going its own way. Some sites may not look right \
                     yet, and protected streaming services such as Netflix do not work.",
    begin: "Begin",

    page_crashed: "This page stopped working",
    page_crashed_note: "You can try reloading it.",
};

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(setting: Language, pairs: &[(&str, &str)]) -> Lang {
        Lang::resolve(setting, |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_owned())
        })
    }

    #[test]
    fn an_explicit_choice_ignores_the_system() {
        assert_eq!(
            resolve(Language::English, &[("LANG", "tr_TR.UTF-8")]),
            Lang::English
        );
        assert_eq!(
            resolve(Language::Turkish, &[("LANG", "en_US.UTF-8")]),
            Lang::Turkish
        );
    }

    #[test]
    fn the_system_language_is_followed() {
        assert_eq!(
            resolve(Language::System, &[("LANG", "tr_TR.UTF-8")]),
            Lang::Turkish
        );
        assert_eq!(
            resolve(Language::System, &[("LANG", "de_DE.UTF-8")]),
            Lang::English
        );
        assert_eq!(resolve(Language::System, &[]), Lang::English);
    }

    #[test]
    fn lc_all_outranks_lang_and_empty_values_are_skipped() {
        assert_eq!(
            resolve(
                Language::System,
                &[("LC_ALL", "tr_TR.UTF-8"), ("LANG", "en_US.UTF-8")]
            ),
            Lang::Turkish
        );
        assert_eq!(
            resolve(Language::System, &[("LC_ALL", ""), ("LANG", "tr_TR.UTF-8")]),
            Lang::Turkish
        );
    }
}
