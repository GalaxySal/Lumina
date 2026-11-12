//! Lumina's design tokens: palettes, type, spacing and motion.
//!
//! Interface code reads every colour, size and duration from here and never
//! writes a literal of its own, so that changing the theme changes everything.

mod fonts;
mod palette;
mod style;

pub use fonts::{install_fonts, mono, sans, sans_medium, serif};
pub use palette::{Palette, Theme, accent_color};
pub use style::apply;

/// Distances, in logical pixels, on a 4-pixel grid.
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 20.0;
    pub const XL: f32 = 32.0;
    pub const XXL: f32 = 56.0;
}

/// Type sizes, in logical pixels.
pub mod size {
    /// Fine print: notes under a choice, status text.
    pub const SMALL: f32 = 13.5;
    /// Running text and controls.
    pub const BODY: f32 = 15.5;
    /// Addresses and other literal text.
    pub const MONO: f32 = 13.5;
    /// Section headings.
    pub const HEADING: f32 = 20.0;
    /// Page titles.
    pub const TITLE: f32 = 34.0;
    /// The greeting on the first-run screen.
    pub const DISPLAY: f32 = 46.0;
}

/// Corner rounding. Deliberately slight: paper has corners.
pub const RADIUS: u8 = 3;

/// Width of rules and outlines.
pub const HAIRLINE: f32 = 1.0;

/// Width of the focus outline, which must always be visible.
pub const FOCUS_WIDTH: f32 = 2.0;

/// How long a state change takes to animate, in seconds.
pub const MOTION: f32 = 0.14;

/// The widest a column of running text should be, for comfortable reading.
pub const MEASURE: f32 = 620.0;
