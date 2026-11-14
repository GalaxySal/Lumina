use std::cell::Cell;
use std::rc::Rc;

use dpi::PhysicalSize;
use euclid::Scale;
use euclid::default::{Point2D, Rect, Size2D};
use servo::{
    CreateNewWebViewRequest, Cursor, DevicePoint, EmbedderControl, EmbedderControlId, LoadStatus,
    Modifiers, OffscreenRenderingContext, PixelFormat, RenderingContext, Servo, Theme, WebView,
    WebViewBuilder, WebViewDelegate, WindowRenderingContext,
};
use url::Url;
use winit::event::WindowEvent;
use winit::window::CursorIcon;

use crate::engine::non_empty;
use crate::input;

/// How far along the current page load is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoadState {
    /// A load has begun; nothing of the new page is available yet.
    Started,
    /// The page is being built and can already be shown.
    Interactive,
    /// The page and everything it needs have loaded.
    Complete,
    /// The load was stopped before it finished.
    Stopped,
}

/// Something the embedder may want to react to, reported by a [`PageView`].
pub enum PageEvent {
    /// A new frame is ready; the embedder should redraw the window.
    FrameReady,
    TitleChanged(Option<String>),
    UrlChanged(Url),
    LoadChanged(LoadState),
    /// The page's place in its back/forward history changed.
    HistoryChanged {
        can_go_back: bool,
        can_go_forward: bool,
    },
    /// The text a browser shows in its status line changed: the address of
    /// the link under the pointer, or nothing when there is none.
    StatusChanged(Option<String>),
    /// The page's site icon changed, or it no longer has one.
    FaviconChanged(Option<Favicon>),
    /// The page asked for a new page to be opened, for instance through a
    /// link that opens in a new window. Dropping the request refuses it.
    NewPageRequested(PendingPage),
    /// The pointer should take this shape while over the page.
    CursorChanged(CursorIcon),
    /// An editable field took focus. The embedder should turn the system
    /// input method on and place its candidate window next to the field.
    /// The area is relative to the page.
    TextInputFocused(Area),
    /// The editable field lost focus; the input method should be turned off.
    TextInputBlurred,
    /// The page's content process failed. The view is no longer usable until
    /// it is reloaded.
    Crashed {
        reason: String,
    },
}

/// A site's icon as plain pixels: `width * height` RGBA values, row by row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Favicon {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Favicon {
    /// Converts an icon from the engine's pixel formats. Formats without
    /// colour, and data of the wrong length, yield nothing.
    fn from_pixels(width: u32, height: u32, format: PixelFormat, data: &[u8]) -> Option<Self> {
        let pixels = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?;
        if pixels == 0 || data.len() != pixels.checked_mul(4)? {
            return None;
        }
        let rgba = match format {
            PixelFormat::RGBA8 => data.to_vec(),
            PixelFormat::BGRA8 => data
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|[blue, green, red, alpha]| [*red, *green, *blue, *alpha])
                .collect(),
            PixelFormat::K8 | PixelFormat::KA8 | PixelFormat::RGB8 => return None,
        };
        Some(Self {
            width,
            height,
            rgba,
        })
    }
}

/// A page that web content asked to have opened and that is waiting for the
/// embedder to give it a place. Dropping it refuses the request.
pub struct PendingPage {
    request: CreateNewWebViewRequest,
    window_context: Rc<WindowRenderingContext>,
}

impl PendingPage {
    /// Opens the requested page. `on_event` is called as for `Engine::open`.
    pub fn open(self, scale_factor: f64, on_event: impl Fn(PageEvent) + 'static) -> PageView {
        let context = Rc::new(
            self.window_context
                .offscreen_context(self.window_context.size()),
        );
        PageView::build(
            self.request.builder(context.clone()),
            context,
            self.window_context,
            scale_factor,
            Box::new(on_event),
        )
    }
}

/// A rectangle in physical pixels, measured from a top-left corner.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Area {
    /// Whether the point (`x`, `y`) lies inside this area.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (left, top) = (f64::from(self.x), f64::from(self.y));
        x >= left
            && y >= top
            && x < left + f64::from(self.width)
            && y < top + f64::from(self.height)
    }
}

/// A single web page being displayed in part of the window.
pub struct PageView {
    webview: WebView,
    context: Rc<OffscreenRenderingContext>,
    /// Where in the window the page is shown.
    area: Cell<Area>,
    /// The pointer's position relative to the page.
    pointer: Cell<DevicePoint>,
    modifiers: Cell<Modifiers>,
    /// Whether an input method composition is in progress.
    composing: Cell<bool>,
    /// Whether the engine has been told the page is on screen.
    visible: Cell<bool>,
}

impl PageView {
    pub(crate) fn new(
        servo: &Servo,
        window_context: Rc<WindowRenderingContext>,
        url: Url,
        scale_factor: f64,
        on_event: Box<dyn Fn(PageEvent)>,
    ) -> Self {
        let context = Rc::new(window_context.offscreen_context(window_context.size()));
        Self::build(
            WebViewBuilder::new(servo, context.clone()).url(url),
            context,
            window_context,
            scale_factor,
            on_event,
        )
    }

    fn build(
        builder: WebViewBuilder,
        context: Rc<OffscreenRenderingContext>,
        window_context: Rc<WindowRenderingContext>,
        scale_factor: f64,
        on_event: Box<dyn Fn(PageEvent)>,
    ) -> Self {
        let webview = builder
            .hidpi_scale_factor(Scale::new(scale_factor as f32))
            .delegate(Rc::new(Delegate {
                on_event,
                input_method: Cell::new(None),
                window_context,
            }))
            .build();

        Self {
            webview,
            context,
            area: Cell::new(Area::default()),
            pointer: Cell::new(DevicePoint::default()),
            modifiers: Cell::new(Modifiers::empty()),
            composing: Cell::new(false),
            visible: Cell::new(true),
        }
    }

    /// Places the page in the window. The page is laid out again if its size
    /// changed.
    pub fn set_area(&self, area: Area) {
        let previous = self.area.replace(area);
        if (previous.width, previous.height) != (area.width, area.height) {
            self.webview.resize(non_empty(PhysicalSize::new(
                area.width.max(0).unsigned_abs(),
                area.height.max(0).unsigned_abs(),
            )));
        }
    }

    /// Where in the window the page is shown.
    pub fn area(&self) -> Area {
        self.area.get()
    }

    /// Renders the current state of the page off screen. Call when a
    /// [`PageEvent::FrameReady`] has arrived; follow with [`PageView::blit`].
    pub fn paint(&self) {
        self.webview.paint();
    }

    /// Copies the most recently painted picture of the page into its area of
    /// the window. `window_height` is the window's height in physical pixels.
    /// Must be called between `Engine::begin_frame` and `Engine::present`.
    pub fn blit(&self, gl: &glow::Context, window_height: u32) {
        let Some(copy) = self.context.render_to_parent_callback() else {
            return;
        };
        let area = self.area.get();
        // OpenGL measures from the bottom-left corner.
        let window_height = i32::try_from(window_height).unwrap_or(i32::MAX);
        let bottom = window_height
            .saturating_sub(area.y)
            .saturating_sub(area.height);
        copy(
            gl,
            Rect::new(
                Point2D::new(area.x, bottom),
                Size2D::new(area.width, area.height),
            ),
        );
    }

    pub fn load(&self, url: Url) {
        self.webview.load(url);
    }

    pub fn reload(&self) {
        self.webview.reload();
    }

    pub fn go_back(&self) {
        if self.webview.can_go_back() {
            self.webview.go_back(1);
        }
    }

    pub fn go_forward(&self) {
        if self.webview.can_go_forward() {
            self.webview.go_forward(1);
        }
    }

    pub fn title(&self) -> Option<String> {
        self.webview.page_title()
    }

    pub fn url(&self) -> Option<Url> {
        self.webview.url()
    }

    /// Tells the page whether the user prefers a dark or a light appearance,
    /// which pages can follow through `prefers-color-scheme`.
    pub fn set_dark(&self, dark: bool) {
        self.webview
            .notify_theme_change(if dark { Theme::Dark } else { Theme::Light });
    }

    /// Tells the engine whether the page is on screen. A page that is not
    /// stops being drawn, so its animations no longer cost anything.
    ///
    /// The engine treats being asked about a page it does not know as a bug
    /// and panics; that is contained here so it can never take Lumina down.
    pub fn set_visible(&self, visible: bool) {
        if self.visible.replace(visible) == visible {
            return;
        }
        let webview = &self.webview;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if visible {
                webview.show();
            } else {
                webview.hide();
            }
        }));
        if outcome.is_err() {
            tracing::error!(visible, "the engine failed to change a page's visibility");
        }
    }

    /// Tells the page whether it is what the user is typing into.
    pub fn set_focused(&self, focused: bool) {
        self.webview.set_focused(focused);
    }

    /// Feeds a window event to the page: scale changes, pointer, wheel,
    /// keyboard and input method events. The embedder decides which events
    /// are meant for the page; pointer positions are window coordinates.
    pub fn handle_window_event(&self, event: &WindowEvent) {
        match event {
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.webview
                    .set_hidpi_scale_factor(Scale::new(*scale_factor as f32));
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers.set(input::modifiers(modifiers.state()));
            }
            WindowEvent::CursorMoved { position, .. } => {
                let area = self.area.get();
                self.pointer.set(DevicePoint::new(
                    (position.x - f64::from(area.x)) as f32,
                    (position.y - f64::from(area.y)) as f32,
                ));
                self.webview
                    .notify_input_event(input::pointer_moved(self.pointer.get()));
            }
            WindowEvent::CursorLeft { .. } => self.pointer_left(),
            WindowEvent::MouseInput { state, button, .. } => {
                self.webview.notify_input_event(input::mouse_button(
                    *state,
                    *button,
                    self.pointer.get(),
                ));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.webview
                    .notify_input_event(input::wheel(*delta, self.pointer.get()));
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.webview.notify_input_event(input::keyboard(
                    event,
                    self.modifiers.get(),
                    self.composing.get(),
                ));
            }
            WindowEvent::Ime(ime) => {
                let (events, composing) = input::composition(self.composing.get(), ime);
                self.composing.set(composing);
                for event in events {
                    self.webview.notify_input_event(event);
                }
            }
            _ => {}
        }
    }

    /// Tells the page the pointer is no longer over it, so hover effects end.
    pub fn pointer_left(&self) {
        self.webview.notify_input_event(input::pointer_left());
    }
}

/// Translates Servo's per-webview callbacks into [`PageEvent`]s.
struct Delegate {
    on_event: Box<dyn Fn(PageEvent)>,
    /// The input method request currently being shown, if any.
    input_method: Cell<Option<EmbedderControlId>>,
    /// The window new pages are opened in.
    window_context: Rc<WindowRenderingContext>,
}

impl WebViewDelegate for Delegate {
    fn notify_status_text_changed(&self, _: WebView, status: Option<String>) {
        (self.on_event)(PageEvent::StatusChanged(status));
    }

    fn notify_favicon_changed(&self, webview: WebView) {
        let favicon = webview.favicon().and_then(|image| {
            Favicon::from_pixels(image.width, image.height, image.format, image.data())
        });
        (self.on_event)(PageEvent::FaviconChanged(favicon));
    }

    fn request_create_new(&self, _: WebView, request: CreateNewWebViewRequest) {
        (self.on_event)(PageEvent::NewPageRequested(PendingPage {
            request,
            window_context: self.window_context.clone(),
        }));
    }

    fn notify_new_frame_ready(&self, _: WebView) {
        (self.on_event)(PageEvent::FrameReady);
    }

    fn notify_page_title_changed(&self, _: WebView, title: Option<String>) {
        (self.on_event)(PageEvent::TitleChanged(title));
    }

    fn notify_url_changed(&self, _: WebView, url: Url) {
        (self.on_event)(PageEvent::UrlChanged(url));
    }

    fn notify_load_status_changed(&self, _: WebView, status: LoadStatus) {
        (self.on_event)(PageEvent::LoadChanged(match status {
            LoadStatus::Started => LoadState::Started,
            LoadStatus::HeadParsed => LoadState::Interactive,
            LoadStatus::Complete => LoadState::Complete,
            LoadStatus::Stopped => LoadState::Stopped,
        }));
    }

    fn notify_history_changed(&self, _: WebView, entries: Vec<Url>, current: usize) {
        (self.on_event)(PageEvent::HistoryChanged {
            can_go_back: current > 0,
            can_go_forward: current.saturating_add(1) < entries.len(),
        });
    }

    fn notify_cursor_changed(&self, _: WebView, cursor: Cursor) {
        (self.on_event)(PageEvent::CursorChanged(input::cursor_icon(cursor)));
    }

    fn show_embedder_control(&self, _: WebView, control: EmbedderControl) {
        // Other controls (pickers, dialogs, menus) are not presented yet;
        // dropping the request gives the page the engine's default answer.
        if let EmbedderControl::InputMethod(input_method) = control {
            let position = input_method.position();
            self.input_method.set(Some(input_method.id()));
            (self.on_event)(PageEvent::TextInputFocused(Area {
                x: position.min.x,
                y: position.min.y,
                width: position.width(),
                height: position.height(),
            }));
        }
    }

    fn hide_embedder_control(&self, _: WebView, control_id: EmbedderControlId) {
        if self.input_method.get() == Some(control_id) {
            self.input_method.set(None);
            (self.on_event)(PageEvent::TextInputBlurred);
        }
    }

    fn notify_crashed(&self, _: WebView, reason: String, backtrace: Option<String>) {
        tracing::error!(%reason, backtrace = backtrace.as_deref().unwrap_or("none"), "page crashed");
        (self.on_event)(PageEvent::Crashed { reason });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favicons_are_converted_to_rgba() {
        let rgba = [1, 2, 3, 255, 4, 5, 6, 128];
        assert_eq!(
            Favicon::from_pixels(2, 1, PixelFormat::RGBA8, &rgba)
                .unwrap()
                .rgba,
            rgba
        );
        assert_eq!(
            Favicon::from_pixels(2, 1, PixelFormat::BGRA8, &rgba)
                .unwrap()
                .rgba,
            [3, 2, 1, 255, 6, 5, 4, 128]
        );
    }

    #[test]
    fn malformed_or_colourless_favicons_are_dropped() {
        assert_eq!(
            Favicon::from_pixels(2, 2, PixelFormat::RGBA8, &[0; 8]),
            None
        );
        assert_eq!(Favicon::from_pixels(0, 0, PixelFormat::RGBA8, &[]), None);
        assert_eq!(Favicon::from_pixels(1, 1, PixelFormat::K8, &[0; 4]), None);
    }

    #[test]
    fn an_area_contains_its_inside_and_top_left_edge_only() {
        let area = Area {
            x: 0,
            y: 46,
            width: 800,
            height: 554,
        };
        assert!(area.contains(0.0, 46.0));
        assert!(area.contains(799.9, 599.9));
        assert!(!area.contains(400.0, 45.9));
        assert!(!area.contains(800.0, 300.0));
        assert!(!area.contains(400.0, 600.0));
    }
}
