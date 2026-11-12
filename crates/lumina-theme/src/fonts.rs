//! The typefaces Lumina ships with. They are compiled into the binary so the
//! interface looks the same on every machine, whatever fonts are installed.
//!
//! All three are under the SIL Open Font License; the licence texts sit next
//! to the font files in `assets/fonts`.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

const NEWSREADER: &[u8] = include_bytes!("../../../assets/fonts/Newsreader.ttf");
const ALEGREYA_SANS: &[u8] = include_bytes!("../../../assets/fonts/AlegreyaSans-Regular.ttf");
const ALEGREYA_SANS_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/AlegreyaSans-Medium.ttf");
const PLEX_MONO: &[u8] = include_bytes!("../../../assets/fonts/IBMPlexMono-Regular.ttf");

const SERIF: &str = "newsreader";
const SANS: &str = "alegreya-sans";
const SANS_MEDIUM: &str = "alegreya-sans-medium";
const MONO: &str = "plex-mono";

/// Newsreader: titles and headings.
pub fn serif(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SERIF.into()))
}

/// Alegreya Sans: running text and controls.
pub fn sans(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}

/// Alegreya Sans Medium: labels that need a little more weight.
pub fn sans_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SANS_MEDIUM.into()))
}

/// IBM Plex Mono: addresses and other text shown exactly as it is.
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

/// Registers Lumina's typefaces with `ctx`. Call once, before the first frame.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::empty();
    for (name, bytes) in [
        (SERIF, NEWSREADER),
        (SANS, ALEGREYA_SANS),
        (SANS_MEDIUM, ALEGREYA_SANS_MEDIUM),
        (MONO, PLEX_MONO),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    // Each family falls back to the others, so a character missing from one
    // face (Plex Mono has Cyrillic, Alegreya Sans has Greek) is still drawn.
    let family = |names: [&str; 4]| names.map(str::to_owned).to_vec();
    fonts.families.insert(
        FontFamily::Proportional,
        family([SANS, SERIF, MONO, SANS_MEDIUM]),
    );
    fonts.families.insert(
        FontFamily::Monospace,
        family([MONO, SANS, SERIF, SANS_MEDIUM]),
    );
    fonts.families.insert(
        FontFamily::Name(SERIF.into()),
        family([SERIF, SANS, MONO, SANS_MEDIUM]),
    );
    fonts.families.insert(
        FontFamily::Name(SANS_MEDIUM.into()),
        family([SANS_MEDIUM, SANS, SERIF, MONO]),
    );

    ctx.set_fonts(fonts);
}
