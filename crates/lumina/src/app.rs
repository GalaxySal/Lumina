use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use egui::{ColorImage, TextureHandle, TextureOptions, ViewportId};
use lumina_engine::{Area, Engine, Favicon, LoadState, PageEvent, PageView, PendingPage};
use lumina_store::{Accent, Appearance, LoadedSettings, SavedGroup, SavedTab, Session, Settings};
use lumina_theme::Theme;
use lumina_ui::{Action, Chrome, GroupInfo, Lang, PageState, TabInfo, View};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, MouseButton, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::tabs::{TabSet, label, worth_saving};

const APP_NAME: &str = "Lumina";

/// Site icons larger than this on a side are ignored: they come from the web
/// and a tab only ever shows them small.
const FAVICON_LIMIT: u32 = 256;

/// How long a tab may sit out of sight before its page is let go to free
/// memory. The tab stays, and loads again when it is next looked at.
const DISCARD_AFTER: Duration = Duration::from_secs(30 * 60);

/// Sent to the event loop when the engine has work waiting.
#[derive(Debug)]
pub struct Wake;

/// Where Lumina keeps things on disk. Either may be unknown.
#[derive(Clone, Debug)]
pub struct Paths {
    /// Settings.
    pub config: Option<PathBuf>,
    /// What is remembered between runs, such as the open tabs.
    pub state: Option<PathBuf>,
}

pub struct App {
    /// The address asked for on the command line, if any.
    requested: Option<Url>,
    /// Taken when the browser starts.
    loaded: Option<LoadedSettings>,
    paths: Paths,
    proxy: EventLoopProxy<Wake>,
    state: State,
}

enum State {
    /// The window system has not let us create a window yet.
    Starting,
    Running(Box<Browser>),
    /// Start-up failed; the reason is reported once the event loop returns.
    Failed(anyhow::Error),
}

/// One tab: its page, if it has one yet, and what is known about it.
#[derive(Default)]
struct Tab {
    /// `None` for a new tab, and for a restored tab not yet looked at.
    view: Option<PageView>,
    url: Option<Url>,
    title: Option<String>,
    /// What the tab is called in the tab strip.
    label: Option<String>,
    pinned: bool,
    /// The id of the group the tab belongs to.
    group: Option<u32>,
    /// The address of the link under the pointer, as the page reports it.
    status: Option<String>,
    /// When the tab's page was last on screen.
    last_shown: Option<Instant>,
    favicon: Option<TextureHandle>,
    loading: bool,
    can_go_back: bool,
    can_go_forward: bool,
    crashed: bool,
    /// The engine has a newer picture of the page than the one last painted.
    stale: bool,
    /// The page has been painted at least once, so there is something to show.
    painted: bool,
    cursor: CursorIcon,
    /// The editable field the page wants an input method for, if any,
    /// relative to the page.
    text_input: Option<Area>,
}

impl Tab {
    /// A tab that will show `url` once it is given a page.
    fn at(url: Url, title: Option<String>) -> Self {
        let mut tab = Self {
            url: Some(url),
            title,
            ..Self::default()
        };
        tab.relabel();
        tab
    }

    fn relabel(&mut self) {
        self.label = label(self.title.as_deref(), self.url.as_ref());
    }

    /// The tab as it is remembered once closed: a plain tab at its address.
    fn saved(&self) -> Option<SavedTab> {
        let url = self.url.as_ref().filter(|url| worth_saving(url))?;
        Some(SavedTab::new(
            url.clone(),
            self.title.clone().unwrap_or_default(),
        ))
    }
}

/// A named, coloured set of tabs.
struct Group {
    id: u32,
    name: String,
    color: Accent,
    /// Whether the group's tabs are folded away.
    collapsed: bool,
}

/// Two pages shown side by side: the tab in front and one other.
#[derive(Clone, Copy)]
struct Split {
    /// The tab shown beside the one in front.
    other: u64,
    /// Whether the tab in front is the one on the right.
    front_on_right: bool,
}

type EventQueue = Rc<RefCell<Vec<(u64, PageEvent)>>>;

/// Field order is drop order: the pages, the interface painter and the engine
/// must be gone before the window whose surface they draw into.
struct Browser {
    tabs: TabSet<Tab>,
    groups: Vec<Group>,
    next_group: u32,
    split: Option<Split>,
    /// Where the second page is shown when two are side by side.
    other_area: Area,
    /// A press was used by Lumina itself (to open a link in a new tab), so
    /// the release that follows is not the page's either.
    swallow_release: bool,
    painter: egui_glow::Painter,
    engine: Engine,
    egui: egui_winit::State,
    chrome: Chrome,
    settings: Settings,
    paths: Paths,
    theme: Theme,
    lang: Lang,
    /// Page events with the id of the tab they belong to, queued by the
    /// engine's callbacks until they are handled.
    events: EventQueue,
    /// Where the page is shown in the window, as of the last frame.
    page_area: Area,
    /// When the remembered tabs last stopped matching the open ones.
    session_stale_since: Option<Instant>,
    modifiers: ModifiersState,
    pointer: PhysicalPosition<f64>,
    pointer_over_page: bool,
    /// Whether typing goes to the page rather than to Lumina's interface.
    page_has_keyboard: bool,
    window_focused: bool,
    window: Rc<Window>,
}

impl App {
    pub fn new(
        requested: Option<Url>,
        loaded: LoadedSettings,
        paths: Paths,
        proxy: EventLoopProxy<Wake>,
    ) -> Self {
        Self {
            requested,
            loaded: Some(loaded),
            paths,
            proxy,
            state: State::Starting,
        }
    }

    /// The outcome of the whole run, for `main` to report.
    pub fn into_result(self) -> Result<()> {
        match self.state {
            State::Failed(error) => Err(error),
            State::Starting | State::Running(_) => Ok(()),
        }
    }

    fn start(&mut self, event_loop: &ActiveEventLoop) -> Result<Browser> {
        let window = Rc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(APP_NAME)
                        .with_inner_size(LogicalSize::new(1280.0, 800.0))
                        .with_min_inner_size(LogicalSize::new(420.0, 320.0)),
                )
                .context("could not open a window")?,
        );

        let proxy = self.proxy.clone();
        let engine = Engine::new(&window, move || {
            // Sending fails only once the event loop is gone, when there is
            // nothing left to wake.
            let _ = proxy.send_event(Wake);
        })
        .context("could not start the web engine")?;

        let painter = egui_glow::Painter::new(engine.gl(), "", None, false)
            .map_err(|error| anyhow!("could not prepare the interface for drawing: {error}"))?;

        let LoadedSettings { settings, problems } = self.loaded.take().unwrap_or(LoadedSettings {
            settings: Settings::default(),
            problems: Vec::new(),
        });

        let context = egui::Context::default();
        lumina_theme::install_fonts(&context);
        let theme = theme_for(&settings, &window);
        lumina_theme::apply(&context, &theme);
        let egui = egui_winit::State::new(
            context,
            ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(painter.max_texture_side()),
        );

        let (tabs, groups) = initial_tabs(&settings, &self.paths, self.requested.take());
        let next_group = u32::try_from(groups.len()).unwrap_or(u32::MAX);

        let settings_file = self
            .paths
            .config
            .as_ref()
            .map(|dir| dir.join("settings.toml"));
        let chrome = Chrome::new(&settings, problems, settings_file);
        let lang = lang_for(&settings);

        let mut browser = Browser {
            tabs,
            groups,
            next_group,
            split: None,
            other_area: Area::default(),
            swallow_release: false,
            painter,
            engine,
            egui,
            chrome,
            settings,
            paths: self.paths.clone(),
            theme,
            lang,
            events: Rc::new(RefCell::new(Vec::new())),
            page_area: Area::default(),
            session_stale_since: None,
            modifiers: ModifiersState::empty(),
            pointer: PhysicalPosition::new(-1.0, -1.0),
            pointer_over_page: false,
            page_has_keyboard: false,
            window_focused: true,
            window,
        };
        browser.wake_active_tab();
        Ok(browser)
    }
}

/// The tabs and groups to start with: the ones open last time, plus a tab
/// for an address given on the command line; or failing both, the start page.
fn initial_tabs(
    settings: &Settings,
    paths: &Paths,
    requested: Option<Url>,
) -> (TabSet<Tab>, Vec<Group>) {
    let session = match &paths.state {
        // Before the first-run screen has been completed there is nothing
        // worth restoring.
        Some(dir) if settings.setup_complete => lumina_store::load_session(dir),
        _ => Session::default(),
    };

    let groups: Vec<Group> = session
        .groups
        .into_iter()
        .zip(0_u32..)
        .map(|(group, id)| Group {
            id,
            name: group.name,
            color: group.color,
            collapsed: group.collapsed,
        })
        .collect();
    let restore = |saved: SavedTab| {
        let mut tab = Tab::at(
            saved.url,
            Some(saved.title).filter(|title| !title.is_empty()),
        );
        tab.pinned = saved.pinned;
        tab.group = saved
            .group
            .and_then(|index| groups.get(index))
            .map(|group| group.id);
        tab
    };

    let mut saved = session.tabs.into_iter();
    let Some(first) = saved.next() else {
        let url = requested.or_else(|| settings.start_url().ok());
        let tab = url.map_or_else(Tab::default, |url| Tab::at(url, None));
        return (TabSet::new(tab), Vec::new());
    };

    let mut tabs = TabSet::new(restore(first));
    for tab in saved {
        tabs.add(restore(tab), None);
    }
    tabs.select_index(session.active);
    tabs.arrange(|tab| (tab.pinned, tab.group));
    if let Some(url) = requested {
        let id = tabs.add(Tab::at(url, None), None);
        tabs.select(id);
    }
    (tabs, groups)
}

fn theme_for(settings: &Settings, window: &Window) -> Theme {
    let dark = match settings.appearance {
        Appearance::Light => false,
        Appearance::Dark => true,
        Appearance::System => window.theme() == Some(winit::window::Theme::Dark),
    };
    Theme::new(dark, settings.accent)
}

fn lang_for(settings: &Settings) -> Lang {
    Lang::resolve(settings.language, |name| std::env::var(name).ok())
}

impl ApplicationHandler<Wake> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !matches!(self.state, State::Starting) {
            return;
        }
        self.state = match self.start(event_loop) {
            Ok(browser) => State::Running(Box::new(browser)),
            Err(error) => {
                event_loop.exit();
                State::Failed(error)
            }
        };
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        // A repaint the interface asked for at a later time has come due.
        if matches!(cause, StartCause::ResumeTimeReached { .. })
            && let State::Running(browser) = &self.state
        {
            event_loop.set_control_flow(ControlFlow::Wait);
            browser.window.request_redraw();
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, _: Wake) {
        if let State::Running(browser) = &mut self.state {
            browser.pump();
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let State::Running(browser) = &mut self.state else {
            return;
        };
        browser.pump();

        match event {
            WindowEvent::CloseRequested => {
                browser.save_session();
                browser.painter.destroy();
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => browser.frame(event_loop),
            other => browser.on_window_event(&other),
        }
    }
}

impl Browser {
    fn active(&self) -> Option<&Tab> {
        self.tabs.active()
    }

    fn active_view(&self) -> Option<&PageView> {
        self.active().and_then(|tab| tab.view.as_ref())
    }

    /// Opens a page for `url` whose events are filed under tab `id`.
    fn open_view(&self, id: u64, url: Url) -> PageView {
        let view = self.engine.open(
            url,
            self.window.scale_factor(),
            event_sink(&self.events, &self.window, id),
        );
        self.prepare_view(&view);
        view
    }

    fn prepare_view(&self, view: &PageView) {
        view.set_dark(self.theme.dark);
        view.set_area(self.page_area);
    }

    /// Gives the tab `id` a page if it has an address but was never loaded
    /// (as restored tabs are until looked at) or was let go to free memory.
    fn ensure_view(&mut self, id: u64) {
        let pending = self
            .tabs
            .get_mut(id)
            .filter(|tab| tab.view.is_none())
            .and_then(|tab| tab.url.clone());
        if let Some(url) = pending {
            let view = self.open_view(id, url);
            if let Some(tab) = self.tabs.get_mut(id) {
                tab.view = Some(view);
                tab.loading = true;
            }
        }
        if let Some(tab) = self.tabs.get_mut(id) {
            // Whatever was painted for this tab may be for another size.
            tab.stale = tab.view.is_some();
        }
    }

    /// Makes the window reflect the tab that has just come to the front.
    fn wake_active_tab(&mut self) {
        self.ensure_view(self.tabs.active_id());
        self.tidy_split();
        let title = self.active().and_then(|tab| tab.title.clone());
        self.window.set_title(&window_title(title.as_deref()));
        self.pointer_over_page = false;
        self.window.set_cursor(CursorIcon::Default);
        self.page_has_keyboard = false;
        self.window.request_redraw();
    }

    /// Ends the side-by-side view if its second tab is gone or has become
    /// the tab in front.
    fn tidy_split(&mut self) {
        let front = self.tabs.active_id();
        if let Some(split) = self.split
            && (split.other == front || self.tabs.get_mut(split.other).is_none())
        {
            self.split = None;
        }
    }

    /// Takes the keyboard and pointer away from the tab in front, before
    /// another tab takes its place.
    fn rest_active_tab(&mut self) {
        if let Some(view) = self.active_view() {
            view.pointer_left();
            view.set_focused(false);
        }
        if self.page_has_keyboard {
            self.window.set_ime_allowed(false);
        }
    }

    fn new_tab(&mut self) {
        self.rest_active_tab();
        let id = self.tabs.add(Tab::default(), None);
        self.tabs.select(id);
        self.wake_active_tab();
        self.session_changed_now();
    }

    fn close_tab(&mut self, id: u64) {
        if id == self.tabs.active_id() {
            self.rest_active_tab();
        }
        // The closed tab's page is dropped here, which ends it in the engine.
        self.tabs.close(id, Tab::saved, Tab::default);
        self.drop_empty_groups();
        self.wake_active_tab();
        self.session_changed_now();
    }

    /// Forgets groups that no tab belongs to any more.
    fn drop_empty_groups(&mut self) {
        let tabs = &self.tabs;
        self.groups
            .retain(|group| tabs.iter().any(|(_, tab)| tab.group == Some(group.id)));
    }

    /// Puts the tabs in the order their pins and groups call for.
    fn rearrange(&mut self) {
        self.drop_empty_groups();
        self.tabs.arrange(|tab| (tab.pinned, tab.group));
        self.session_changed_now();
        self.window.request_redraw();
    }

    fn reopen_closed_tab(&mut self) {
        let Some(saved) = self.tabs.take_closed() else {
            return;
        };
        self.rest_active_tab();
        let title = Some(saved.title).filter(|title| !title.is_empty());
        let id = self.tabs.add(Tab::at(saved.url, title), None);
        self.tabs.select(id);
        self.wake_active_tab();
        self.session_changed_now();
    }

    /// Runs `change` on the tab set and, if it brought another tab to the
    /// front, hands the window over to that tab.
    fn switch_tab(&mut self, change: impl FnOnce(&mut TabSet<Tab>) -> bool) {
        let before = self.tabs.active_id();
        // The outgoing tab must be rested before the front tab changes.
        self.rest_active_tab();
        if change(&mut self.tabs) && self.tabs.active_id() != before {
            // Choosing the tab on the other side of a side-by-side view
            // moves the focus across; the two pages stay where they are.
            if let Some(split) = &mut self.split
                && split.other == self.tabs.active_id()
            {
                split.other = before;
                split.front_on_right = !split.front_on_right;
            }
            self.wake_active_tab();
            self.session_changed_now();
        } else if let Some(view) = self.active_view() {
            view.set_focused(self.page_has_keyboard && self.window_focused);
        }
        self.window.request_redraw();
    }

    /// Lets the engine work, then deals with whatever it reported.
    fn pump(&mut self) {
        self.engine.pump();
        let events: Vec<(u64, PageEvent)> = match self.events.try_borrow_mut() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => return,
        };
        for (id, event) in events {
            self.on_page_event(id, event);
        }
        self.save_session_if_due();
        self.discard_idle_pages();
    }

    fn on_page_event(&mut self, id: u64, event: PageEvent) {
        let in_front = id == self.tabs.active_id();

        // A page asking for a new page gets a tab of its own, right after it.
        if let PageEvent::NewPageRequested(pending) = event {
            self.open_requested_page(id, pending);
            return;
        }
        if let PageEvent::StatusChanged(status) = event {
            if let Some(tab) = self.tabs.get_mut(id) {
                tab.status = status.filter(|status| !status.trim().is_empty());
            }
            self.window.request_redraw();
            return;
        }
        if let PageEvent::FaviconChanged(favicon) = event {
            let texture = favicon.and_then(|favicon| self.favicon_texture(id, &favicon));
            if let Some(tab) = self.tabs.get_mut(id) {
                tab.favicon = texture;
            }
            self.window.request_redraw();
            return;
        }

        // Events for a tab that has since been closed are dropped.
        let Some(tab) = self.tabs.get_mut(id) else {
            return;
        };
        match event {
            PageEvent::FrameReady => tab.stale = true,
            PageEvent::TitleChanged(title) => {
                tab.title = title.filter(|title| !title.trim().is_empty());
                tab.relabel();
                if in_front {
                    let title = tab.title.clone();
                    self.window.set_title(&window_title(title.as_deref()));
                }
                self.session_changed();
            }
            PageEvent::UrlChanged(url) => {
                tracing::info!(%url, "navigated");
                tab.url = Some(url);
                tab.relabel();
                self.session_changed();
            }
            PageEvent::LoadChanged(state) => {
                tab.loading = matches!(state, LoadState::Started | LoadState::Interactive);
                if state == LoadState::Started {
                    tab.crashed = false;
                }
            }
            PageEvent::HistoryChanged {
                can_go_back,
                can_go_forward,
            } => {
                tab.can_go_back = can_go_back;
                tab.can_go_forward = can_go_forward;
            }
            PageEvent::CursorChanged(icon) => {
                tab.cursor = icon;
                if in_front && self.pointer_over_page {
                    self.window.set_cursor(icon);
                }
            }
            PageEvent::TextInputFocused(area) => {
                tab.text_input = Some(area);
                if in_front {
                    self.apply_page_input_method();
                }
            }
            PageEvent::TextInputBlurred => {
                tab.text_input = None;
                // Lumina's own text fields manage the input method themselves.
                if in_front && self.page_has_keyboard {
                    self.window.set_ime_allowed(false);
                }
            }
            PageEvent::Crashed { reason } => {
                tracing::error!(%reason, "a page stopped working");
                tab.crashed = true;
                tab.loading = false;
            }
            PageEvent::NewPageRequested(_)
            | PageEvent::FaviconChanged(_)
            | PageEvent::StatusChanged(_) => {}
        }
        self.window.request_redraw();
    }

    fn open_requested_page(&mut self, opener: u64, pending: PendingPage) {
        self.rest_active_tab();
        let id = self.tabs.next_id();
        let view = pending.open(
            self.window.scale_factor(),
            event_sink(&self.events, &self.window, id),
        );
        self.prepare_view(&view);
        let tab = Tab {
            view: Some(view),
            loading: true,
            // A page opened from a grouped tab belongs with it.
            group: self.tabs.get_mut(opener).and_then(|tab| tab.group),
            ..Tab::default()
        };
        let id = self.tabs.add(tab, Some(opener));
        self.tabs.select(id);
        self.wake_active_tab();
        self.session_changed_now();
    }

    /// Turns a site icon into something the interface can draw.
    fn favicon_texture(&self, id: u64, favicon: &Favicon) -> Option<TextureHandle> {
        if favicon.width > FAVICON_LIMIT || favicon.height > FAVICON_LIMIT {
            return None;
        }
        let size = [
            usize::try_from(favicon.width).ok()?,
            usize::try_from(favicon.height).ok()?,
        ];
        // The engine layer has already checked the data's length.
        let image = ColorImage::from_rgba_unmultiplied(size, &favicon.rgba);
        Some(self.egui.egui_ctx().load_texture(
            format!("favicon-{id}"),
            image,
            TextureOptions::LINEAR,
        ))
    }

    /// Notes that the remembered tabs are out of date; they are written soon.
    fn session_changed(&mut self) {
        self.session_stale_since.get_or_insert_with(Instant::now);
    }

    /// For changes the user made by hand (opening, closing, moving a tab):
    /// rare enough to write at once.
    fn session_changed_now(&mut self) {
        self.save_session();
    }

    fn save_session_if_due(&mut self) {
        if self
            .session_stale_since
            .is_some_and(|since| since.elapsed().as_secs() >= 2)
        {
            self.save_session();
        }
    }

    fn save_session(&mut self) {
        self.session_stale_since = None;
        if !self.settings.setup_complete {
            return;
        }
        let Some(dir) = &self.paths.state else {
            return;
        };
        let groups = &self.groups;
        let mut session = self.tabs.session(|tab| {
            let mut saved = tab.saved()?;
            saved.pinned = tab.pinned;
            saved.group = tab
                .group
                .and_then(|id| groups.iter().position(|group| group.id == id));
            Some(saved)
        });
        session.groups = groups
            .iter()
            .map(|group| SavedGroup {
                name: group.name.clone(),
                color: group.color,
                collapsed: group.collapsed,
            })
            .collect();
        if let Err(error) = lumina_store::save_session(dir, &session) {
            tracing::warn!(%error, "the open tabs could not be remembered");
        }
    }

    /// Lets go of the pages of tabs that have been out of sight for a long
    /// time. The tabs stay and load again when looked at. Pinned tabs and
    /// tabs still loading are left alone.
    fn discard_idle_pages(&mut self) {
        let front = self.tabs.active_id();
        let beside = self.split.map(|split| split.other);
        let ids: Vec<u64> = self
            .tabs
            .iter()
            .filter(|(id, tab)| {
                *id != front
                    && Some(*id) != beside
                    && tab.view.is_some()
                    && !tab.pinned
                    && !tab.loading
                    && tab.url.as_ref().is_some_and(worth_saving)
                    && tab
                        .last_shown
                        .is_some_and(|shown| shown.elapsed() >= DISCARD_AFTER)
            })
            .map(|(id, _)| id)
            .collect();
        for id in ids {
            if let Some(tab) = self.tabs.get_mut(id) {
                tracing::info!(tab = id, "letting go of a page that has been out of sight");
                tab.view = None;
                tab.painted = false;
                tab.stale = false;
                tab.text_input = None;
                tab.status = None;
                tab.can_go_back = false;
                tab.can_go_forward = false;
            }
        }
    }

    /// Turns the system input method on for the page's focused text field and
    /// places its candidate window beside the field.
    fn apply_page_input_method(&self) {
        let Some(field) = self.active().and_then(|tab| tab.text_input) else {
            return;
        };
        if !self.page_has_keyboard {
            return;
        }
        let page = self.page_area;
        self.window.set_ime_allowed(true);
        self.window.set_ime_cursor_area(
            PhysicalPosition::new(
                page.x.saturating_add(field.x),
                page.y.saturating_add(field.y),
            ),
            PhysicalSize::new(
                field.width.max(0).unsigned_abs(),
                field.height.max(0).unsigned_abs(),
            ),
        );
    }

    /// Passes a window event on to the page in front, if there is one.
    fn to_page(&self, event: &WindowEvent) {
        if let Some(view) = self.active_view() {
            view.handle_window_event(event);
        }
    }

    fn on_window_event(&mut self, event: &WindowEvent) {
        tracing::trace!(?event, "window event");
        match event {
            WindowEvent::Resized(size) => self.engine.resize(*size),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                // Both sides need to know which modifier keys are held.
                self.to_page(event);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                for tab in self.tabs.iter_mut() {
                    if let Some(view) = &tab.view {
                        view.handle_window_event(event);
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                self.window_focused = *focused;
                if let Some(view) = self.active_view() {
                    view.set_focused(*focused && self.page_has_keyboard);
                }
            }
            WindowEvent::ThemeChanged(_) => self.restyle(),
            WindowEvent::KeyboardInput { event: key, .. } => {
                if self.shortcut(key) {
                    self.window.request_redraw();
                    return;
                }
                if self.page_has_keyboard {
                    self.to_page(event);
                    return;
                }
            }
            WindowEvent::Ime(_) if self.page_has_keyboard => {
                self.to_page(event);
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = *position;
                self.track_pointer();
                if self.pointer_over_page {
                    self.to_page(event);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = PhysicalPosition::new(-1.0, -1.0);
                self.track_pointer();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pressed = *state == ElementState::Pressed;
                if !pressed && std::mem::take(&mut self.swallow_release) {
                    // The matching press opened a link in a new tab.
                } else if pressed && self.pointer_over_other_page() {
                    // A press on the other page of a side-by-side view moves
                    // the focus to it; the page then gets the press.
                    if let Some(split) = self.split {
                        self.switch_tab(|tabs| tabs.select(split.other));
                        self.track_pointer();
                        self.give_keyboard_to_page();
                        self.to_page(&WindowEvent::CursorMoved {
                            device_id: winit::event::DeviceId::dummy(),
                            position: self.pointer,
                        });
                        self.to_page(event);
                    }
                } else if self.pointer_over_page {
                    let new_tab = pressed
                        && (*button == MouseButton::Middle
                            || (*button == MouseButton::Left && self.modifiers.control_key()));
                    if new_tab && self.open_hovered_link_in_new_tab() {
                        self.swallow_release = true;
                    } else {
                        if pressed {
                            self.give_keyboard_to_page();
                        }
                        self.to_page(event);
                    }
                }
            }
            WindowEvent::MouseWheel { .. } if self.pointer_over_page => self.to_page(event),
            _ => {}
        }

        // Lumina's interface sees every event that was not exclusively the
        // page's, so its hover and focus state stay correct.
        let response = self.egui.on_window_event(&self.window, event);
        if response.repaint {
            self.window.request_redraw();
        }
    }

    /// Whether the tab in front is showing a web page the user can point at.
    fn page_is_showing(&self) -> bool {
        self.chrome.showing_page()
            && self
                .active()
                .is_some_and(|tab| tab.view.is_some() && !tab.crashed)
    }

    /// Whether the pointer is over the second page of a side-by-side view.
    fn pointer_over_other_page(&self) -> bool {
        self.split.is_some()
            && self.page_is_showing()
            && self.other_area.contains(self.pointer.x, self.pointer.y)
            && !self.egui.egui_ctx().egui_is_using_pointer()
    }

    /// Opens the link under the pointer in a new tab behind the current one.
    /// Returns whether there was a link to open.
    fn open_hovered_link_in_new_tab(&mut self) -> bool {
        let opener = self.tabs.active_id();
        let Some(url) = self
            .active()
            .and_then(|tab| tab.status.as_deref())
            .and_then(|status| Url::parse(status).ok())
            .filter(|url| matches!(url.scheme(), "http" | "https"))
        else {
            return false;
        };
        let mut tab = Tab::at(url, None);
        tab.group = self.active().and_then(|tab| tab.group);
        let id = self.tabs.add(tab, Some(opener));
        // It loads at once, out of sight.
        self.ensure_view(id);
        self.session_changed_now();
        self.window.request_redraw();
        true
    }

    /// Works out whether the pointer is over the page, and tells the page
    /// when it arrives or leaves.
    fn track_pointer(&mut self) {
        let over_page = self.page_is_showing()
            && self.page_area.contains(self.pointer.x, self.pointer.y)
            && !self.egui.egui_ctx().egui_is_using_pointer();
        if over_page == self.pointer_over_page {
            return;
        }
        self.pointer_over_page = over_page;
        if over_page {
            let cursor = self.active().map_or(CursorIcon::Default, |tab| tab.cursor);
            self.window.set_cursor(cursor);
        } else {
            if let Some(view) = self.active_view() {
                view.pointer_left();
            }
            self.window.set_cursor(CursorIcon::Default);
        }
    }

    /// Moves keyboard focus from Lumina's interface to the page.
    fn give_keyboard_to_page(&mut self) {
        self.egui.egui_ctx().memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
        self.set_page_has_keyboard(true);
    }

    fn set_page_has_keyboard(&mut self, has_keyboard: bool) {
        if self.page_has_keyboard == has_keyboard {
            return;
        }
        self.page_has_keyboard = has_keyboard;
        if let Some(view) = self.active_view() {
            view.set_focused(has_keyboard && self.window_focused);
        }
        if has_keyboard {
            self.apply_page_input_method();
        }
    }

    /// Handles the keys that belong to the browser rather than to the page.
    /// Returns whether `key` was one of them.
    fn shortcut(&mut self, key: &KeyEvent) -> bool {
        if key.state != ElementState::Pressed {
            return false;
        }
        let control = self.modifiers.control_key();
        let shift = self.modifiers.shift_key();
        let alt = self.modifiers.alt_key();
        let letter = match &key.logical_key {
            Key::Character(text) => Some(text.to_lowercase()),
            _ => None,
        };

        match (&key.logical_key, letter.as_deref()) {
            (_, Some("l")) if control => self.chrome.focus_address(),
            (_, Some("r")) if control => self.act(Action::Reload),
            (_, Some(",")) if control => self.chrome.toggle_settings(),
            (_, Some("t")) if control && shift => self.reopen_closed_tab(),
            (_, Some("t")) if control => self.new_tab(),
            (_, Some("w")) if control => self.close_tab(self.tabs.active_id()),
            (_, Some(digit)) if control && tab_number(digit).is_some() => {
                let index = tab_number(digit).unwrap_or(0);
                self.switch_tab(|tabs| tabs.select_index(index));
            }
            (Key::Named(NamedKey::Tab), _) if control => {
                self.switch_tab(|tabs| tabs.cycle(!shift));
            }
            (Key::Named(NamedKey::F5), _) => self.act(Action::Reload),
            (Key::Named(NamedKey::ArrowLeft), _) if alt => self.act(Action::Back),
            (Key::Named(NamedKey::ArrowRight), _) if alt => self.act(Action::Forward),
            (Key::Named(NamedKey::Escape), _) => return self.chrome.close_settings(),
            _ => return false,
        }
        true
    }

    fn act(&mut self, action: Action) {
        match action {
            Action::Open(url) => self.open_in_active_tab(url),
            Action::Back => {
                if let Some(view) = self.active_view() {
                    view.go_back();
                }
            }
            Action::Forward => {
                if let Some(view) = self.active_view() {
                    view.go_forward();
                }
            }
            Action::Reload => {
                if let Some(tab) = self.tabs.active_mut() {
                    tab.crashed = false;
                    if let Some(view) = &tab.view {
                        view.reload();
                    }
                }
            }
            Action::NewTab => self.new_tab(),
            Action::CloseTab(id) => self.close_tab(id),
            Action::SelectTab(id) => self.switch_tab(|tabs| tabs.select(id)),
            Action::MoveTab { from, to } => {
                if self.tabs.move_tab(from, to) {
                    self.rearrange();
                }
            }
            Action::TogglePin(id) => {
                if let Some(tab) = self.tabs.get_mut(id) {
                    tab.pinned = !tab.pinned;
                    if tab.pinned {
                        tab.group = None;
                    }
                }
                self.rearrange();
            }
            Action::NewGroup(id) => {
                let group = self.next_group;
                self.next_group = self.next_group.saturating_add(1);
                // Each new group takes the next colour round.
                let color = Accent::ALL
                    .into_iter()
                    .cycle()
                    .nth(self.groups.len())
                    .unwrap_or_default();
                self.groups.push(Group {
                    id: group,
                    name: String::new(),
                    color,
                    collapsed: false,
                });
                self.set_group(id, Some(group));
            }
            Action::AddToGroup { tab, group } => {
                if self.groups.iter().any(|known| known.id == group) {
                    self.set_group(tab, Some(group));
                }
            }
            Action::Ungroup(id) => self.set_group(id, None),
            Action::ToggleGroup(id) => {
                if let Some(group) = self.group_mut(id) {
                    group.collapsed = !group.collapsed;
                }
                self.session_changed_now();
            }
            Action::RenameGroup(id, name) => {
                if let Some(group) = self.group_mut(id) {
                    // Names come from the keyboard; keep them to a sane length.
                    group.name = name.chars().take(40).collect();
                }
                self.session_changed();
            }
            Action::RecolorGroup(id, color) => {
                if let Some(group) = self.group_mut(id) {
                    group.color = color;
                }
                self.session_changed_now();
            }
            Action::DissolveGroup(id) => {
                for tab in self.tabs.iter_mut() {
                    if tab.group == Some(id) {
                        tab.group = None;
                    }
                }
                self.rearrange();
            }
            Action::SplitWith(id) => {
                if id != self.tabs.active_id() && self.tabs.get_mut(id).is_some() {
                    self.ensure_view(id);
                    self.split = Some(Split {
                        other: id,
                        front_on_right: false,
                    });
                }
            }
            Action::CloseSplit => self.split = None,
            Action::ChangeSettings(settings) => {
                self.settings = settings;
                let error = match &self.paths.config {
                    Some(dir) => lumina_store::save(dir, &self.settings).err().map(|error| {
                        tracing::error!(%error, "settings could not be saved");
                        error.to_string()
                    }),
                    None => None,
                };
                self.chrome.set_save_error(error);
                self.restyle();
            }
        }
        self.window.request_redraw();
    }

    fn group_mut(&mut self, id: u32) -> Option<&mut Group> {
        self.groups.iter_mut().find(|group| group.id == id)
    }

    fn set_group(&mut self, tab: u64, group: Option<u32>) {
        if let Some(tab) = self.tabs.get_mut(tab) {
            tab.group = group;
            if group.is_some() {
                tab.pinned = false;
            }
        }
        self.rearrange();
    }

    fn open_in_active_tab(&mut self, url: Url) {
        let id = self.tabs.active_id();
        let has_view = self.active_view().is_some();
        let view = (!has_view).then(|| self.open_view(id, url.clone()));
        if let Some(tab) = self.tabs.active_mut() {
            tab.crashed = false;
            tab.loading = true;
            tab.url = Some(url.clone());
            tab.relabel();
            match view {
                Some(view) => tab.view = Some(view),
                None => {
                    if let Some(view) = &tab.view {
                        view.load(url);
                    }
                }
            }
        }
        self.give_keyboard_to_page();
        self.session_changed();
    }

    /// Re-derives the theme and language from the settings and the desktop.
    fn restyle(&mut self) {
        self.theme = theme_for(&self.settings, &self.window);
        self.lang = lang_for(&self.settings);
        lumina_theme::apply(self.egui.egui_ctx(), &self.theme);
        let dark = self.theme.dark;
        for tab in self.tabs.iter_mut() {
            if let Some(view) = &tab.view {
                view.set_dark(dark);
            }
        }
        self.window.request_redraw();
    }

    /// Tells the engine which pages are on screen, and paints the ones that
    /// are and have something new to show. `beside` is the second page of a
    /// side-by-side view.
    fn show_pages(&mut self, show_page: bool, beside: Option<u64>) {
        let front = self.tabs.active_id();
        let other_area = self.other_area;
        let now = Instant::now();
        let ids: Vec<u64> = self.tabs.iter().map(|(id, _)| id).collect();
        for id in ids {
            let shown = (show_page && id == front) || Some(id) == beside;
            let Some(tab) = self.tabs.get_mut(id) else {
                continue;
            };
            let Some(view) = &tab.view else {
                continue;
            };
            view.set_visible(shown);
            if !shown {
                // A page that has never been on screen starts waiting now.
                tab.last_shown.get_or_insert(now);
                continue;
            }
            tab.last_shown = Some(now);
            if Some(id) == beside {
                view.set_area(other_area);
            }
            if tab.stale {
                view.paint();
                tab.stale = false;
                tab.painted = true;
            }
        }
    }

    /// Draws one frame: Lumina's interface, with the page in the space the
    /// interface leaves for it.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let input = self.egui.take_egui_input(&self.window);
        let context = self.egui.egui_ctx().clone();

        let mut drawn = None;
        let mut output = {
            let infos: Vec<TabInfo<'_>> = self
                .tabs
                .iter()
                .map(|(id, tab)| TabInfo {
                    id,
                    title: tab.label.as_deref(),
                    loading: tab.loading,
                    favicon: tab.favicon.as_ref().map(TextureHandle::id),
                    pinned: tab.pinned,
                    group: tab.group,
                })
                .collect();
            let groups: Vec<GroupInfo<'_>> = self
                .groups
                .iter()
                .map(|group| GroupInfo {
                    id: group.id,
                    name: &group.name,
                    color: group.color,
                    collapsed: group.collapsed,
                })
                .collect();
            let front = self.tabs.active();
            let page = PageState {
                url: front.and_then(|tab| tab.url.as_ref()),
                loading: front.is_some_and(|tab| tab.loading),
                can_go_back: front.is_some_and(|tab| tab.can_go_back),
                can_go_forward: front.is_some_and(|tab| tab.can_go_forward),
                crashed: front.is_some_and(|tab| tab.crashed),
                blank: front.is_none_or(|tab| tab.view.is_none()),
                status: front
                    .filter(|_| self.pointer_over_page)
                    .and_then(|tab| tab.status.as_deref()),
            };
            let view = View {
                theme: &self.theme,
                settings: &self.settings,
                lang: self.lang,
                tabs: &infos,
                groups: &groups,
                active: self.tabs.active_index(),
                split: self.split.map(|split| split.front_on_right),
                page,
            };
            let chrome = &mut self.chrome;
            context.run_ui(input, |ui| {
                drawn = Some(chrome.show(ui, view));
            })
        };
        self.egui
            .handle_platform_output(&self.window, std::mem::take(&mut output.platform_output));
        let drawn = drawn.unwrap_or_default();
        let scale = output.pixels_per_point;

        let size = self.window.inner_size();
        let pixel = |value: f32| (value * scale).round() as i32;
        let area_of = |rect: egui::Rect| {
            let (left, top) = (pixel(rect.min.x), pixel(rect.min.y));
            Area {
                x: left,
                y: top,
                width: pixel(rect.max.x).saturating_sub(left),
                height: pixel(rect.max.y).saturating_sub(top),
            }
        };
        let show_page = match drawn.page_rect {
            Some(rect) if self.active_view().is_some() => {
                self.page_area = area_of(rect);
                if let Some(view) = self.active_view() {
                    view.set_area(self.page_area);
                }
                true
            }
            _ => false,
        };
        // The second page of a side-by-side view.
        let beside = self.split.map(|split| split.other).filter(|_| show_page);
        self.other_area = drawn
            .other_rect
            .filter(|_| beside.is_some())
            .map(area_of)
            .unwrap_or_default();

        // Typing goes to the page unless one of Lumina's own controls has
        // keyboard focus or no page is showing.
        let interface_focused = context.memory(|memory| memory.focused().is_some());
        self.set_page_has_keyboard(show_page && !interface_focused);

        self.track_pointer();
        if self.pointer_over_page {
            // The interface resets the cursor each frame; the page's wins
            // while the pointer is over it.
            let cursor = self.active().map_or(CursorIcon::Default, |tab| tab.cursor);
            self.window.set_cursor(cursor);
        }

        self.show_pages(show_page, beside);

        if size.width > 0 && size.height > 0 {
            match self.engine.begin_frame() {
                Ok(()) => {
                    let dimensions = [size.width, size.height];
                    let paper = self.theme.palette.paper;
                    self.painter.clear(
                        dimensions,
                        [
                            f32::from(paper.r()) / 255.0,
                            f32::from(paper.g()) / 255.0,
                            f32::from(paper.b()) / 255.0,
                            1.0,
                        ],
                    );
                    if show_page
                        && let Some(tab) = self.tabs.active()
                        && tab.painted
                        && let Some(view) = &tab.view
                    {
                        view.blit(self.painter.gl(), size.height);
                    }
                    if let Some(id) = beside
                        && let Some(tab) = self.tabs.get_mut(id)
                        && tab.painted
                        && let Some(view) = &tab.view
                    {
                        view.blit(self.painter.gl(), size.height);
                    }
                    let primitives = context.tessellate(std::mem::take(&mut output.shapes), scale);
                    self.painter.paint_and_update_textures(
                        dimensions,
                        scale,
                        &primitives,
                        &mut output.textures_delta,
                    );
                    self.engine.present();
                }
                Err(error) => tracing::error!(%error, "could not draw a frame"),
            }
        }

        for action in drawn.actions {
            self.act(action);
        }
        self.save_session_if_due();

        // Draw again when the interface asks to: at once for an animation in
        // progress, later for something like a blinking text cursor.
        if let Some(viewport) = output.viewport_output.get(&ViewportId::ROOT) {
            let delay = viewport.repaint_delay;
            if delay.is_zero() {
                self.window.request_redraw();
            } else if let Some(due) = Instant::now().checked_add(delay) {
                event_loop.set_control_flow(ControlFlow::WaitUntil(due));
            }
        }
    }
}

/// The callback a page reports its events through: each event is queued
/// under the id of the tab it belongs to and the window is asked to redraw.
fn event_sink(events: &EventQueue, window: &Rc<Window>, id: u64) -> impl Fn(PageEvent) + 'static {
    let events = events.clone();
    let window = window.clone();
    move |event| {
        // The queue is only ever borrowed for the length of a push or a
        // drain, never across a call into the engine, so this cannot fail;
        // if it somehow did, losing one event is better than stopping.
        if let Ok(mut queue) = events.try_borrow_mut() {
            queue.push((id, event));
        }
        window.request_redraw();
    }
}

/// The tab position a digit key asks for: 1 to 8 are the first eight tabs
/// and 9 is the last one, however many there are.
fn tab_number(key: &str) -> Option<usize> {
    match key.parse::<usize>().ok()? {
        9 => Some(usize::MAX),
        digit @ 1..=8 => digit.checked_sub(1),
        _ => None,
    }
}

fn window_title(page_title: Option<&str>) -> String {
    match page_title.map(str::trim) {
        Some(title) if !title.is_empty() => format!("{title} — {APP_NAME}"),
        _ => APP_NAME.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_title_falls_back_to_the_app_name() {
        assert_eq!(window_title(None), "Lumina");
        assert_eq!(window_title(Some("   ")), "Lumina");
        assert_eq!(window_title(Some("Servo")), "Servo — Lumina");
    }

    #[test]
    fn digit_keys_name_tab_positions() {
        assert_eq!(tab_number("1"), Some(0));
        assert_eq!(tab_number("8"), Some(7));
        assert_eq!(tab_number("9"), Some(usize::MAX));
        assert_eq!(tab_number("0"), None);
        assert_eq!(tab_number("a"), None);
    }
}
