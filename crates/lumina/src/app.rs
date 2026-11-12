use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use egui::ViewportId;
use lumina_engine::{Area, Engine, LoadState, PageEvent, PageView};
use lumina_store::{Appearance, LoadedSettings, Settings};
use lumina_theme::Theme;
use lumina_ui::{Action, Chrome, Lang, PageState};
use url::Url;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, KeyEvent, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

const APP_NAME: &str = "Lumina";

/// Sent to the event loop when the engine has work waiting.
#[derive(Debug)]
pub struct Wake;

pub struct App {
    start_url: Url,
    /// Taken when the browser starts.
    loaded: Option<LoadedSettings>,
    config_dir: Option<PathBuf>,
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

/// What is known about the page being shown.
#[derive(Default)]
struct PageStatus {
    url: Option<Url>,
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

/// Field order is drop order: the page, the interface painter and the engine
/// must be gone before the window whose surface they draw into.
struct Browser {
    view: PageView,
    painter: egui_glow::Painter,
    engine: Engine,
    egui: egui_winit::State,
    chrome: Chrome,
    settings: Settings,
    config_dir: Option<PathBuf>,
    theme: Theme,
    lang: Lang,
    status: PageStatus,
    /// Page events, queued by the engine's callback until they are handled.
    events: Rc<RefCell<Vec<PageEvent>>>,
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
        start_url: Url,
        loaded: LoadedSettings,
        config_dir: Option<PathBuf>,
        proxy: EventLoopProxy<Wake>,
    ) -> Self {
        Self {
            start_url,
            loaded: Some(loaded),
            config_dir,
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

        let events = Rc::new(RefCell::new(Vec::new()));
        let view = engine.open(self.start_url.clone(), window.scale_factor(), {
            let events = events.clone();
            let window = window.clone();
            move |event| {
                // The queue is only ever borrowed for the length of a push or
                // a drain, never across a call into the engine, so this
                // cannot fail; if it somehow did, losing one event is better
                // than stopping.
                if let Ok(mut queue) = events.try_borrow_mut() {
                    queue.push(event);
                }
                window.request_redraw();
            }
        });
        view.set_dark(theme.dark);

        let settings_file = self
            .config_dir
            .as_ref()
            .map(|dir| dir.join("settings.toml"));
        let chrome = Chrome::new(&settings, problems, settings_file);
        let lang = lang_for(&settings);

        Ok(Browser {
            view,
            painter,
            engine,
            egui,
            chrome,
            settings,
            config_dir: self.config_dir.clone(),
            theme,
            lang,
            status: PageStatus {
                url: Some(self.start_url.clone()),
                loading: true,
                ..PageStatus::default()
            },
            events,
            modifiers: ModifiersState::empty(),
            pointer: PhysicalPosition::new(-1.0, -1.0),
            pointer_over_page: false,
            page_has_keyboard: false,
            window_focused: true,
            window,
        })
    }
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
                browser.painter.destroy();
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => browser.frame(event_loop),
            other => browser.on_window_event(&other),
        }
    }
}

impl Browser {
    /// Lets the engine work, then deals with whatever it reported.
    fn pump(&mut self) {
        self.engine.pump();
        let events: Vec<PageEvent> = match self.events.try_borrow_mut() {
            Ok(mut queue) => queue.drain(..).collect(),
            Err(_) => return,
        };
        for event in events {
            self.on_page_event(event);
        }
    }

    fn on_page_event(&mut self, event: PageEvent) {
        match event {
            PageEvent::FrameReady => self.status.stale = true,
            PageEvent::TitleChanged(title) => {
                self.window.set_title(&window_title(title.as_deref()));
            }
            PageEvent::UrlChanged(url) => {
                tracing::info!(%url, "navigated");
                self.status.url = Some(url);
            }
            PageEvent::LoadChanged(state) => {
                self.status.loading = matches!(state, LoadState::Started | LoadState::Interactive);
                if state == LoadState::Started {
                    self.status.crashed = false;
                }
            }
            PageEvent::HistoryChanged {
                can_go_back,
                can_go_forward,
            } => {
                self.status.can_go_back = can_go_back;
                self.status.can_go_forward = can_go_forward;
            }
            PageEvent::CursorChanged(icon) => {
                self.status.cursor = icon;
                if self.pointer_over_page {
                    self.window.set_cursor(icon);
                }
            }
            PageEvent::TextInputFocused(area) => {
                self.status.text_input = Some(area);
                self.apply_page_input_method();
            }
            PageEvent::TextInputBlurred => {
                self.status.text_input = None;
                // Lumina's own text fields manage the input method themselves.
                if self.page_has_keyboard {
                    self.window.set_ime_allowed(false);
                }
            }
            PageEvent::Crashed { reason } => {
                tracing::error!(%reason, "the page stopped working");
                self.status.crashed = true;
                self.status.loading = false;
            }
        }
        self.window.request_redraw();
    }

    /// Turns the system input method on for the page's focused text field and
    /// places its candidate window beside the field.
    fn apply_page_input_method(&self) {
        let Some(field) = self.status.text_input else {
            return;
        };
        if !self.page_has_keyboard {
            return;
        }
        let page = self.view.area();
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

    fn on_window_event(&mut self, event: &WindowEvent) {
        tracing::trace!(?event, "window event");
        match event {
            WindowEvent::Resized(size) => self.engine.resize(*size),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
                // Both sides need to know which modifier keys are held.
                self.view.handle_window_event(event);
            }
            WindowEvent::ScaleFactorChanged { .. } => self.view.handle_window_event(event),
            WindowEvent::Focused(focused) => {
                self.window_focused = *focused;
                self.view.set_focused(*focused && self.page_has_keyboard);
            }
            WindowEvent::ThemeChanged(_) => self.restyle(),
            WindowEvent::KeyboardInput { event: key, .. } => {
                if self.shortcut(key) {
                    self.window.request_redraw();
                    return;
                }
                if self.page_has_keyboard {
                    self.view.handle_window_event(event);
                    return;
                }
            }
            WindowEvent::Ime(_) if self.page_has_keyboard => {
                self.view.handle_window_event(event);
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.pointer = *position;
                self.track_pointer();
                if self.pointer_over_page {
                    self.view.handle_window_event(event);
                }
            }
            WindowEvent::CursorLeft { .. } => {
                self.pointer = PhysicalPosition::new(-1.0, -1.0);
                self.track_pointer();
            }
            WindowEvent::MouseInput { state, .. } if self.pointer_over_page => {
                if *state == ElementState::Pressed {
                    self.give_keyboard_to_page();
                }
                self.view.handle_window_event(event);
            }
            WindowEvent::MouseWheel { .. } if self.pointer_over_page => {
                self.view.handle_window_event(event);
            }
            _ => {}
        }

        // Lumina's interface sees every event that was not exclusively the
        // page's, so its hover and focus state stay correct.
        let response = self.egui.on_window_event(&self.window, event);
        if response.repaint {
            self.window.request_redraw();
        }
    }

    /// Works out whether the pointer is over the page, and tells the page
    /// when it arrives or leaves.
    fn track_pointer(&mut self) {
        let over_page = self.chrome.showing_page()
            && !self.status.crashed
            && self.view.area().contains(self.pointer.x, self.pointer.y)
            && !self.egui.egui_ctx().egui_is_using_pointer();
        if over_page == self.pointer_over_page {
            return;
        }
        self.pointer_over_page = over_page;
        if over_page {
            self.window.set_cursor(self.status.cursor);
        } else {
            self.view.pointer_left();
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
        self.view.set_focused(has_keyboard && self.window_focused);
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
        let alt = self.modifiers.alt_key();
        let letter = match &key.logical_key {
            Key::Character(text) => Some(text.to_lowercase()),
            _ => None,
        };

        match (&key.logical_key, letter.as_deref()) {
            (_, Some("l")) if control => self.chrome.focus_address(),
            (_, Some("r")) if control => self.act(Action::Reload),
            (_, Some(",")) if control => self.chrome.toggle_settings(),
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
            Action::Open(url) => {
                self.status.crashed = false;
                self.status.loading = true;
                self.status.url = Some(url.clone());
                self.view.load(url);
                self.give_keyboard_to_page();
            }
            Action::Back => self.view.go_back(),
            Action::Forward => self.view.go_forward(),
            Action::Reload => {
                self.status.crashed = false;
                self.view.reload();
            }
            Action::ChangeSettings(settings) => {
                self.settings = settings;
                let error = match &self.config_dir {
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

    /// Re-derives the theme and language from the settings and the desktop.
    fn restyle(&mut self) {
        self.theme = theme_for(&self.settings, &self.window);
        self.lang = lang_for(&self.settings);
        lumina_theme::apply(self.egui.egui_ctx(), &self.theme);
        self.view.set_dark(self.theme.dark);
        self.window.request_redraw();
    }

    /// Draws one frame: Lumina's interface, with the page in the space the
    /// interface leaves for it.
    fn frame(&mut self, event_loop: &ActiveEventLoop) {
        let input = self.egui.take_egui_input(&self.window);
        let context = self.egui.egui_ctx().clone();

        let mut drawn = None;
        let mut output = {
            let page = PageState {
                url: self.status.url.as_ref(),
                loading: self.status.loading,
                can_go_back: self.status.can_go_back,
                can_go_forward: self.status.can_go_forward,
                crashed: self.status.crashed,
            };
            let (chrome, theme, settings, lang) =
                (&mut self.chrome, &self.theme, &self.settings, self.lang);
            context.run_ui(input, |ui| {
                drawn = Some(chrome.show(ui, theme, settings, lang, page));
            })
        };
        self.egui
            .handle_platform_output(&self.window, std::mem::take(&mut output.platform_output));
        let drawn = drawn.unwrap_or_default();
        let scale = output.pixels_per_point;

        // Typing goes to the page unless one of Lumina's own controls has
        // keyboard focus.
        let interface_focused = context.memory(|memory| memory.focused().is_some());
        self.set_page_has_keyboard(self.chrome.showing_page() && !interface_focused);

        let size = self.window.inner_size();
        let show_page = match drawn.page_rect {
            Some(rect) => {
                let pixel = |value: f32| (value * scale).round() as i32;
                let (left, top) = (pixel(rect.min.x), pixel(rect.min.y));
                self.view.set_area(Area {
                    x: left,
                    y: top,
                    width: pixel(rect.max.x).saturating_sub(left),
                    height: pixel(rect.max.y).saturating_sub(top),
                });
                true
            }
            None => false,
        };
        self.track_pointer();
        if self.pointer_over_page {
            // The interface resets the cursor each frame; the page's wins
            // while the pointer is over it.
            self.window.set_cursor(self.status.cursor);
        }

        if show_page && self.status.stale {
            self.view.paint();
            self.status.stale = false;
            self.status.painted = true;
        }

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
                    if show_page && self.status.painted {
                        self.view.blit(self.painter.gl(), size.height);
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
}
