//! Lumina's own interface: the toolbar and address bar, the settings screen
//! and the first-run screen.
//!
//! This crate draws and reports what the user asked for as [`Action`]s; it
//! never touches the web engine or the disk itself.

mod screens;
mod text;
mod toolbar;
mod widgets;

use std::path::PathBuf;

use egui::Rect;
use lumina_store::{Problem, Settings, StartPage};
use lumina_theme::Theme;
use url::Url;

pub use text::Lang;

/// What the interface needs to know about the page being shown.
#[derive(Clone, Copy, Debug)]
pub struct PageState<'a> {
    pub url: Option<&'a Url>,
    pub loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    /// The page's content process failed and it must be reloaded.
    pub crashed: bool,
}

/// Something the user asked for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Open(Url),
    Back,
    Forward,
    Reload,
    /// Apply these settings now and keep them.
    ChangeSettings(Settings),
}

/// The result of drawing one frame.
#[derive(Debug, Default)]
pub struct Output {
    pub actions: Vec<Action>,
    /// Where the web page belongs, in logical pixels, or `None` while one of
    /// Lumina's own screens covers it.
    pub page_rect: Option<Rect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Screen {
    Page,
    Settings,
    /// The first-run screen.
    Setup,
}

/// The state of Lumina's interface between frames.
pub struct Chrome {
    screen: Screen,
    /// The text in the address bar.
    address: String,
    /// Set to move keyboard focus to the address bar on the next frame.
    focus_address: bool,
    /// The text in the "address I choose" start-page field.
    start_address: String,
    /// Whether the user picked a start address of their own, even if what
    /// they have typed so far is not yet a valid one.
    start_is_custom: bool,
    problems: Vec<Problem>,
    save_error: Option<String>,
    settings_file: Option<PathBuf>,
}

impl Chrome {
    /// `problems` are whatever went wrong reading the settings file and
    /// `settings_file` is where settings are kept, if anywhere.
    pub fn new(
        settings: &Settings,
        problems: Vec<Problem>,
        settings_file: Option<PathBuf>,
    ) -> Self {
        let (start_address, start_is_custom) = match &settings.start_page {
            StartPage::SearchEngine => (String::new(), false),
            StartPage::Custom(url) => (url.to_string(), true),
        };
        Self {
            screen: if settings.setup_complete {
                Screen::Page
            } else {
                Screen::Setup
            },
            address: String::new(),
            focus_address: false,
            start_address,
            start_is_custom,
            problems,
            save_error: None,
            settings_file,
        }
    }

    /// Whether the web page is what the user is looking at and typing into.
    pub fn showing_page(&self) -> bool {
        self.screen == Screen::Page
    }

    /// Moves keyboard focus to the address bar and selects its contents.
    pub fn focus_address(&mut self) {
        if self.screen != Screen::Setup {
            self.screen = Screen::Page;
            self.focus_address = true;
        }
    }

    /// Opens the settings screen, or closes it if it is open.
    pub fn toggle_settings(&mut self) {
        self.screen = match self.screen {
            Screen::Page => Screen::Settings,
            Screen::Settings => Screen::Page,
            Screen::Setup => Screen::Setup,
        };
    }

    /// Closes the settings screen if it is open. Returns whether it was.
    pub fn close_settings(&mut self) -> bool {
        let was_open = self.screen == Screen::Settings;
        if was_open {
            self.screen = Screen::Page;
        }
        was_open
    }

    /// Tells the interface whether the last attempt to save settings failed,
    /// so it can say so.
    pub fn set_save_error(&mut self, error: Option<String>) {
        self.save_error = error;
    }

    /// Draws the interface for this frame into `ui`, which must cover the
    /// whole window.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        settings: &Settings,
        lang: Lang,
        page: PageState<'_>,
    ) -> Output {
        let mut output = Output::default();
        let strings = lang.strings();

        if self.screen == Screen::Setup {
            screens::setup(self, ui, theme, settings, strings, &mut output);
            return output;
        }

        toolbar::show(self, ui, theme, settings, strings, page, &mut output);

        match self.screen {
            Screen::Settings => screens::settings(self, ui, theme, settings, strings, &mut output),
            Screen::Page if page.crashed => screens::crashed(ui, theme, strings, &mut output),
            Screen::Page | Screen::Setup => {
                // Left untouched so the engine's picture shows through.
                output.page_rect = Some(ui.available_rect_before_wrap());
            }
        }
        output
    }
}
