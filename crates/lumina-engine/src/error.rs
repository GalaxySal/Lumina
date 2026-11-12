use winit::raw_window_handle::HandleError;

/// Everything that can go wrong while bringing the engine up.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("the display connection is not available: {0}")]
    DisplayHandle(#[source] HandleError),

    #[error("the window is not ready to be drawn into: {0}")]
    WindowHandle(#[source] HandleError),

    /// The graphics stack refused to give us a surface. The detail is the
    /// driver's own description, kept for logs.
    #[error("could not create a graphics surface for the window ({0})")]
    RenderingContext(String),
}
