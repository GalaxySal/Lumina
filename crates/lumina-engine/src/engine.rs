use std::rc::Rc;
use std::sync::Arc;

use dpi::PhysicalSize;
use servo::{EventLoopWaker, RenderingContext, Servo, ServoBuilder, WindowRenderingContext};
use url::Url;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use winit::window::Window;

use crate::error::EngineError;
use crate::view::{PageEvent, PageView};

/// One running instance of the web engine, drawing into one window.
///
/// Pages are painted off screen and then copied into the window, so that the
/// embedder can draw its own interface around and over them using the same
/// graphics context (see [`Engine::gl`]).
pub struct Engine {
    servo: Servo,
    window_context: Rc<WindowRenderingContext>,
}

impl Engine {
    /// Starts the engine and binds it to `window`.
    ///
    /// `wake` is called, possibly from another thread, whenever the engine has
    /// work waiting. The embedder must respond by calling [`Engine::pump`] on
    /// the thread that owns the engine.
    pub fn new(
        window: &Window,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, EngineError> {
        // Servo's network stack needs a process-wide TLS provider. Installing
        // fails only when one is already installed, which is exactly the state
        // we want, so that outcome is deliberately ignored.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let display_handle = window
            .display_handle()
            .map_err(EngineError::DisplayHandle)?;
        let window_handle = window.window_handle().map_err(EngineError::WindowHandle)?;

        let window_context = Rc::new(
            WindowRenderingContext::new(
                display_handle,
                window_handle,
                non_empty(window.inner_size()),
            )
            .map_err(graphics_error)?,
        );
        window_context.make_current().map_err(graphics_error)?;

        let servo = ServoBuilder::default()
            .event_loop_waker(Box::new(Waker(Arc::new(wake))))
            .build();
        servo.setup_logging();

        Ok(Self {
            servo,
            window_context,
        })
    }

    /// Lets the engine process whatever work it has queued. Page events are
    /// delivered to their callbacks from inside this call.
    pub fn pump(&self) {
        self.servo.spin_event_loop();
    }

    /// Opens `url` in a new page view.
    ///
    /// `on_event` is called on the engine's thread, from inside
    /// [`Engine::pump`].
    pub fn open(
        &self,
        url: Url,
        scale_factor: f64,
        on_event: impl Fn(PageEvent) + 'static,
    ) -> PageView {
        PageView::new(
            &self.servo,
            self.window_context.clone(),
            url,
            scale_factor,
            Box::new(on_event),
        )
    }

    /// The window's OpenGL context, for the embedder to draw its interface
    /// with. Call [`Engine::begin_frame`] before drawing.
    pub fn gl(&self) -> Arc<glow::Context> {
        self.window_context.glow_gl_api()
    }

    /// Tells the engine the window's drawable area changed size.
    pub fn resize(&self, size: PhysicalSize<u32>) {
        self.window_context.resize(non_empty(size));
    }

    /// Makes the window the target of drawing commands. Everything drawn
    /// between this and [`Engine::present`] makes up one frame.
    pub fn begin_frame(&self) -> Result<(), EngineError> {
        self.window_context.make_current().map_err(graphics_error)?;
        self.window_context.prepare_for_rendering();
        Ok(())
    }

    /// Shows the finished frame in the window.
    pub fn present(&self) {
        self.window_context.present();
    }
}

fn graphics_error(error: impl std::fmt::Debug) -> EngineError {
    EngineError::RenderingContext(format!("{error:?}"))
}

/// A zero-sized surface cannot be created; a minimised or not yet configured
/// window reports one.
pub(crate) fn non_empty(size: PhysicalSize<u32>) -> PhysicalSize<u32> {
    PhysicalSize::new(size.width.max(1), size.height.max(1))
}

#[derive(Clone)]
struct Waker(Arc<dyn Fn() + Send + Sync>);

impl EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn EventLoopWaker> {
        Box::new(self.clone())
    }

    fn wake(&self) {
        (self.0)();
    }
}
