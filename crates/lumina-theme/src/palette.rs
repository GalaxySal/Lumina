use egui::Color32;
use lumina_store::Accent;

/// The colours of one theme. Nothing here is pure white or pure black: the
/// light theme is ink on paper, the dark theme is warm charcoal, not blue-grey.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Palette {
    /// The page: the main background.
    pub paper: Color32,
    /// The toolbar and other surfaces that sit above the page.
    pub raised: Color32,
    /// The inside of text fields.
    pub field: Color32,
    /// Running text.
    pub ink: Color32,
    /// Secondary text: notes, labels, placeholders.
    pub ink_muted: Color32,
    /// Hairlines and outlines.
    pub rule: Color32,
    /// The one accent: focus, selection, the primary action.
    pub accent: Color32,
    /// Text placed on top of the accent.
    pub on_accent: Color32,
    /// Problems the user needs to know about.
    pub danger: Color32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub palette: Palette,
}

impl Theme {
    pub fn new(dark: bool, accent: Accent) -> Self {
        let palette = if dark {
            Palette {
                paper: Color32::from_rgb(0x1F, 0x1B, 0x18),
                raised: Color32::from_rgb(0x2A, 0x25, 0x20),
                field: Color32::from_rgb(0x17, 0x14, 0x11),
                ink: Color32::from_rgb(0xED, 0xE4, 0xD3),
                ink_muted: Color32::from_rgb(0xA9, 0x9E, 0x8E),
                rule: Color32::from_rgb(0x45, 0x3D, 0x35),
                accent: accent_color(accent, true),
                on_accent: Color32::from_rgb(0x1F, 0x1B, 0x18),
                danger: Color32::from_rgb(0xE8, 0x8B, 0x7D),
            }
        } else {
            Palette {
                paper: Color32::from_rgb(0xF4, 0xEF, 0xE4),
                raised: Color32::from_rgb(0xEA, 0xE3, 0xD3),
                field: Color32::from_rgb(0xFB, 0xF8, 0xF1),
                ink: Color32::from_rgb(0x2A, 0x24, 0x20),
                ink_muted: Color32::from_rgb(0x66, 0x5D, 0x53),
                rule: Color32::from_rgb(0xCF, 0xC4, 0xB0),
                accent: accent_color(accent, false),
                on_accent: Color32::from_rgb(0xFB, 0xF8, 0xF1),
                danger: Color32::from_rgb(0x9B, 0x2C, 0x1F),
            }
        };
        Self { dark, palette }
    }
}

/// The accent as it appears on the light or the dark theme. The dark variants
/// are lighter so they keep their contrast against charcoal.
pub fn accent_color(accent: Accent, dark: bool) -> Color32 {
    match (accent, dark) {
        (Accent::Brick, false) => Color32::from_rgb(0xA3, 0x40, 0x29),
        (Accent::Brick, true) => Color32::from_rgb(0xE2, 0x8D, 0x6F),
        (Accent::Indigo, false) => Color32::from_rgb(0x2F, 0x4A, 0x7A),
        (Accent::Indigo, true) => Color32::from_rgb(0x9F, 0xB6, 0xE2),
        (Accent::Olive, false) => Color32::from_rgb(0x56, 0x61, 0x22),
        (Accent::Olive, true) => Color32::from_rgb(0xBC, 0xC7, 0x7C),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG 2 relative luminance of an sRGB colour.
    fn luminance(color: Color32) -> f64 {
        let channel = |value: u8| {
            let value = f64::from(value) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r()) + 0.7152 * channel(color.g()) + 0.0722 * channel(color.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn themes() -> impl Iterator<Item = Theme> {
        [false, true]
            .into_iter()
            .flat_map(|dark| Accent::ALL.map(|accent| Theme::new(dark, accent)))
    }

    #[test]
    fn text_meets_wcag_aa_on_every_surface_it_sits_on() {
        for theme in themes() {
            let p = theme.palette;
            for (text, surface, what) in [
                (p.ink, p.paper, "ink on paper"),
                (p.ink, p.raised, "ink on raised"),
                (p.ink, p.field, "ink in field"),
                (p.ink_muted, p.paper, "muted on paper"),
                (p.ink_muted, p.raised, "muted on raised"),
                (p.ink_muted, p.field, "muted in field"),
                (p.accent, p.paper, "accent on paper"),
                (p.accent, p.raised, "accent on raised"),
                (p.on_accent, p.accent, "text on accent"),
                (p.danger, p.paper, "danger on paper"),
            ] {
                let ratio = contrast(text, surface);
                assert!(ratio >= 4.5, "{what} is {ratio:.2}:1 in {theme:?}");
            }
        }
    }

    #[test]
    fn outlines_are_visible_against_their_surface() {
        for theme in themes() {
            let p = theme.palette;
            // Focus outlines are drawn in the accent and must reach 3:1.
            assert!(contrast(p.accent, p.field) >= 3.0, "{theme:?}");
            assert!(contrast(p.accent, p.raised) >= 3.0, "{theme:?}");
        }
    }

    #[test]
    fn nothing_is_pure_white_or_pure_black() {
        for theme in themes() {
            let p = theme.palette;
            for color in [
                p.paper,
                p.raised,
                p.field,
                p.ink,
                p.ink_muted,
                p.rule,
                p.accent,
                p.on_accent,
                p.danger,
            ] {
                assert_ne!(color, Color32::WHITE);
                assert_ne!(color, Color32::BLACK);
            }
        }
    }
}
