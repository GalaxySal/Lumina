//! Lumina's own interface: the toolbar and address bar, the settings screen
//! and the first-run screen.
//!
//! This crate draws and reports what the user asked for as [`Action`]s; it
//! never touches the web engine or the disk itself.

mod screens;
mod tabs;
mod text;
mod toolbar;
mod widgets;

use std::path::PathBuf;

use egui::Rect;
use lumina_store::{Accent, Problem, Settings, StartPage, TabLayout};
use lumina_theme::Theme;
use url::Url;

pub use tabs::{GroupInfo, TabInfo};
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
    /// The tab has no page yet; Lumina's new-tab page is shown in its place.
    pub blank: bool,
    /// The address of the link under the pointer, to be shown at the foot of
    /// the page so the user can see where it leads before following it.
    pub status: Option<&'a str>,
}

/// Everything one frame of the interface is drawn from.
#[derive(Clone, Copy)]
pub struct View<'a> {
    pub theme: &'a Theme,
    pub settings: &'a Settings,
    pub lang: Lang,
    pub tabs: &'a [TabInfo<'a>],
    pub groups: &'a [GroupInfo<'a>],
    /// Index into `tabs` of the tab in front.
    pub active: usize,
    /// When two pages are shown side by side: whether the tab in front is
    /// the one on the right.
    pub split: Option<bool>,
    /// The page in the tab in front.
    pub page: PageState<'a>,
}

/// Something the user asked for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Open(Url),
    Back,
    Forward,
    Reload,
    NewTab,
    /// Close the tab with this id.
    CloseTab(u64),
    /// Bring the tab with this id to the front.
    SelectTab(u64),
    /// Move the tab at position `from` to position `to`.
    MoveTab {
        from: usize,
        to: usize,
    },
    /// Pin the tab with this id, or unpin it.
    TogglePin(u64),
    /// Start a new group holding the tab with this id.
    NewGroup(u64),
    AddToGroup {
        tab: u64,
        group: u32,
    },
    /// Take the tab with this id out of its group.
    Ungroup(u64),
    /// Fold the group away, or unfold it.
    ToggleGroup(u32),
    RenameGroup(u32, String),
    RecolorGroup(u32, Accent),
    /// Take every tab out of the group.
    DissolveGroup(u32),
    /// Show the tab with this id beside the tab in front.
    SplitWith(u64),
    CloseSplit,
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
    /// Where the second page belongs when two are shown side by side.
    pub other_rect: Option<Rect>,
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
    /// The text in the new-tab page's search field.
    new_tab_query: String,
    /// The group whose name is being edited, and the text so far.
    group_rename: (u32, String),
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
            new_tab_query: String::new(),
            group_rename: (u32::MAX, String::new()),
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
    pub fn show(&mut self, ui: &mut egui::Ui, view: View<'_>) -> Output {
        let View {
            theme,
            settings,
            lang,
            tabs,
            groups,
            active,
            split,
            page,
        } = view;
        let mut output = Output::default();
        let strings = lang.strings();

        if self.screen == Screen::Setup {
            screens::setup(self, ui, theme, settings, strings, &mut output);
            return output;
        }

        let strip = tabs::Strip {
            theme,
            strings,
            tabs,
            groups,
            active,
            split: split.is_some(),
        };
        match settings.tab_layout {
            TabLayout::Top => tabs::top(ui, strip, &mut self.group_rename, &mut output),
            TabLayout::Side => tabs::side(ui, strip, &mut self.group_rename, &mut output),
        }
        toolbar::show(self, ui, theme, settings, strings, page, &mut output);

        match self.screen {
            Screen::Settings => screens::settings(self, ui, theme, settings, strings, &mut output),
            Screen::Page if page.blank => {
                screens::new_tab(self, ui, theme, settings, strings, &mut output);
            }
            Screen::Page if page.crashed => screens::crashed(ui, theme, strings, &mut output),
            Screen::Page | Screen::Setup => {
                // Left untouched so the engine's picture shows through.
                let whole = ui.available_rect_before_wrap();
                let (front, other) = screens::panes(ui, theme, whole, split);
                output.page_rect = Some(front);
                output.other_rect = other;
                if let Some(status) = page.status {
                    screens::status_line(ui, theme, whole, status);
                }
            }
        }
        output
    }
}
