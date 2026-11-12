//! Lumina's embedding layer over the Servo web engine.
//!
//! This crate is the only place in Lumina that names Servo types. Everything
//! above it talks in terms of [`Engine`], [`PageView`] and [`PageEvent`], so
//! the rest of the browser never depends on the engine's internals.

mod engine;
mod error;
mod input;
mod view;

pub use engine::Engine;
pub use error::EngineError;
pub use view::{Area, LoadState, PageEvent, PageView};
