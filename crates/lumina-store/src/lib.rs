//! Lumina's persistent data. Today that is the user's settings; history,
//! bookmarks and sessions will live here too.

mod paths;
mod search;
mod session;
mod settings;

pub use paths::{config_dir, state_dir};
pub use search::{SearchEngine, resolve};
pub use session::{SavedGroup, SavedTab, Session, load_session, save_session};
pub use settings::{
    Accent, AddressBar, Appearance, Language, LoadedSettings, Problem, Settings, SettingsError,
    StartPage, TabLayout, load, save,
};
