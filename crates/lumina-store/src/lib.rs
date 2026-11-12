//! Lumina's persistent data. Today that is the user's settings; history,
//! bookmarks and sessions will live here too.

mod paths;
mod search;
mod settings;

pub use paths::config_dir;
pub use search::{SearchEngine, resolve};
pub use settings::{
    Accent, Appearance, Language, LoadedSettings, Problem, Settings, SettingsError, StartPage,
    load, save,
};
