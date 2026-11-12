//! The user's settings and the file they are kept in.
//!
//! The file is plain TOML so it can be read and edited by hand. Reading is
//! forgiving: a value Lumina does not understand falls back to its default
//! and is reported as a [`Problem`], and the rest of the file still applies.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use toml::{Table, Value};
use url::Url;

use crate::search::SearchEngine;

const FILE_NAME: &str = "settings.toml";

/// Whether the interface is light or dark.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Appearance {
    /// Follow the desktop's preference.
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.id() == id)
    }
}

/// The single accent colour used for links, focus and selection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Accent {
    /// A muted, earthy red.
    #[default]
    Brick,
    /// A deep ink blue.
    Indigo,
    /// A dark, yellowish green.
    Olive,
}

impl Accent {
    pub const ALL: [Self; 3] = [Self::Brick, Self::Indigo, Self::Olive];

    pub fn id(self) -> &'static str {
        match self {
            Self::Brick => "brick",
            Self::Indigo => "indigo",
            Self::Olive => "olive",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.id() == id)
    }
}

/// The language of Lumina's own interface.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Language {
    /// Follow the desktop's language.
    #[default]
    System,
    Turkish,
    English,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::System, Self::Turkish, Self::English];

    pub fn id(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Turkish => "tr",
            Self::English => "en",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|value| value.id() == id)
    }
}

/// What Lumina opens when it starts without being given an address.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum StartPage {
    /// The front page of the chosen search engine.
    #[default]
    SearchEngine,
    /// An address of the user's choosing.
    Custom(Url),
}

const START_PAGE_SEARCH_ENGINE: &str = "search-engine";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Settings {
    /// False until the user has been through the first-run screen.
    pub setup_complete: bool,
    pub search_engine: SearchEngine,
    pub appearance: Appearance,
    pub accent: Accent,
    pub language: Language,
    pub start_page: StartPage,
}

impl Settings {
    /// The address to open when Lumina starts without one.
    pub fn start_url(&self) -> Result<Url, url::ParseError> {
        match &self.start_page {
            StartPage::SearchEngine => Url::parse(self.search_engine.home()),
            StartPage::Custom(url) => Ok(url.clone()),
        }
    }

    fn to_toml(&self) -> String {
        let mut table = Table::new();
        let mut set = |key: &str, value: Value| {
            table.insert(key.to_owned(), value);
        };
        set("setup_complete", Value::Boolean(self.setup_complete));
        set(
            "search_engine",
            Value::String(self.search_engine.id().into()),
        );
        set("appearance", Value::String(self.appearance.id().into()));
        set("accent", Value::String(self.accent.id().into()));
        set("language", Value::String(self.language.id().into()));
        set(
            "start_page",
            Value::String(match &self.start_page {
                StartPage::SearchEngine => START_PAGE_SEARCH_ENGINE.to_owned(),
                StartPage::Custom(url) => url.to_string(),
            }),
        );
        table.to_string()
    }
}

/// Something in the settings file that could not be used.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Problem {
    /// The file exists but could not be read at all.
    Unreadable { detail: String },
    /// The file is not valid TOML, so none of it could be applied.
    Syntax { line: Option<usize>, detail: String },
    /// One setting has a value Lumina does not understand; its default is
    /// used instead.
    InvalidValue {
        key: &'static str,
        line: Option<usize>,
    },
}

/// The outcome of reading the settings file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSettings {
    pub settings: Settings,
    /// Empty when the file was read cleanly or does not exist yet.
    pub problems: Vec<Problem>,
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not create the settings folder {path}: {source}")]
    CreateDir { path: PathBuf, source: io::Error },
    #[error("could not write the settings file {path}: {source}")]
    Write { path: PathBuf, source: io::Error },
}

/// Reads the settings kept in `dir`. A missing file is not a problem: it
/// means Lumina has not been set up yet, and the defaults are returned.
pub fn load(dir: &Path) -> LoadedSettings {
    let text = match fs::read_to_string(dir.join(FILE_NAME)) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return LoadedSettings {
                settings: Settings::default(),
                problems: Vec::new(),
            };
        }
        Err(error) => {
            return LoadedSettings {
                settings: Settings::default(),
                problems: vec![Problem::Unreadable {
                    detail: error.to_string(),
                }],
            };
        }
    };
    parse(&text)
}

/// Writes `settings` to `dir`, creating it if needed. The file is replaced
/// in one step, so a crash or a full disk cannot leave it half written.
pub fn save(dir: &Path, settings: &Settings) -> Result<(), SettingsError> {
    fs::create_dir_all(dir).map_err(|source| SettingsError::CreateDir {
        path: dir.to_owned(),
        source,
    })?;
    let path = dir.join(FILE_NAME);
    let temporary = dir.join(format!("{FILE_NAME}.new"));
    let write = |source| SettingsError::Write {
        path: path.clone(),
        source,
    };
    fs::write(&temporary, settings.to_toml()).map_err(write)?;
    fs::rename(&temporary, &path).map_err(write)
}

fn parse(text: &str) -> LoadedSettings {
    let table = match text.parse::<Table>() {
        Ok(table) => table,
        Err(error) => {
            return LoadedSettings {
                settings: Settings::default(),
                problems: vec![Problem::Syntax {
                    line: error.span().map(|span| line_of(text, span.start)),
                    detail: error.message().to_owned(),
                }],
            };
        }
    };

    let defaults = Settings::default();
    let mut problems = Vec::new();
    let mut read = |key: &'static str, apply: &mut dyn FnMut(&Value) -> bool| {
        if let Some(value) = table.get(key)
            && !apply(value)
        {
            problems.push(Problem::InvalidValue {
                key,
                line: line_of_key(text, key),
            });
        }
    };

    let mut settings = defaults;
    read("setup_complete", &mut |value| {
        value
            .as_bool()
            .map(|flag| settings.setup_complete = flag)
            .is_some()
    });
    read("search_engine", &mut |value| {
        value
            .as_str()
            .and_then(SearchEngine::from_id)
            .map(|engine| settings.search_engine = engine)
            .is_some()
    });
    read("appearance", &mut |value| {
        value
            .as_str()
            .and_then(Appearance::from_id)
            .map(|appearance| settings.appearance = appearance)
            .is_some()
    });
    read("accent", &mut |value| {
        value
            .as_str()
            .and_then(Accent::from_id)
            .map(|accent| settings.accent = accent)
            .is_some()
    });
    read("language", &mut |value| {
        value
            .as_str()
            .and_then(Language::from_id)
            .map(|language| settings.language = language)
            .is_some()
    });
    read("start_page", &mut |value| {
        value
            .as_str()
            .and_then(start_page_from)
            .map(|start_page| settings.start_page = start_page)
            .is_some()
    });

    LoadedSettings { settings, problems }
}

fn start_page_from(text: &str) -> Option<StartPage> {
    if text == START_PAGE_SEARCH_ENGINE {
        return Some(StartPage::SearchEngine);
    }
    let url = Url::parse(text).ok()?;
    matches!(url.scheme(), "http" | "https" | "file" | "about").then_some(StartPage::Custom(url))
}

/// The 1-based line containing byte `offset`.
fn line_of(text: &str, offset: usize) -> usize {
    text.bytes()
        .take(offset)
        .filter(|byte| *byte == b'\n')
        .count()
        .saturating_add(1)
}

/// The 1-based line on which top-level `key` is assigned, if it can be found.
fn line_of_key(text: &str, key: &str) -> Option<usize> {
    text.lines()
        .position(|line| {
            line.trim_start()
                .strip_prefix(key)
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        })
        .map(|index| index.saturating_add(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_means_defaults_and_no_complaints() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.settings, Settings::default());
        assert!(loaded.problems.is_empty());
        assert!(!loaded.settings.setup_complete);
    }

    #[test]
    fn what_is_saved_is_what_is_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            setup_complete: true,
            search_engine: SearchEngine::Brave,
            appearance: Appearance::Dark,
            accent: Accent::Olive,
            language: Language::Turkish,
            start_page: StartPage::Custom(Url::parse("https://example.com/giriş").unwrap()),
        };
        // The folder does not exist yet; saving must create it.
        let nested = dir.path().join("lumina");
        save(&nested, &settings).unwrap();

        let loaded = load(&nested);
        assert_eq!(loaded.settings, settings);
        assert!(loaded.problems.is_empty());
        assert!(!nested.join("settings.toml.new").exists());
    }

    #[test]
    fn saving_again_replaces_the_previous_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        save(dir.path(), &settings).unwrap();
        settings.search_engine = SearchEngine::Startpage;
        save(dir.path(), &settings).unwrap();
        assert_eq!(
            load(dir.path()).settings.search_engine,
            SearchEngine::Startpage
        );
    }

    #[test]
    fn one_bad_value_does_not_spoil_the_rest() {
        let loaded = parse(
            "search_engine = \"brave\"\n\
             appearance = \"neon\"\n\
             accent = \"indigo\"\n",
        );
        assert_eq!(loaded.settings.search_engine, SearchEngine::Brave);
        assert_eq!(loaded.settings.appearance, Appearance::System);
        assert_eq!(loaded.settings.accent, Accent::Indigo);
        assert_eq!(
            loaded.problems,
            [Problem::InvalidValue {
                key: "appearance",
                line: Some(2),
            }]
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_is_reported() {
        let loaded = parse("setup_complete = \"yes\"\nsearch_engine = 3\n");
        assert_eq!(loaded.settings, Settings::default());
        assert_eq!(loaded.problems.len(), 2);
    }

    #[test]
    fn broken_syntax_reports_the_line_and_keeps_defaults() {
        let loaded = parse("search_engine = \"brave\"\naccent = = \"olive\"\n");
        assert_eq!(loaded.settings, Settings::default());
        assert!(matches!(
            loaded.problems.as_slice(),
            [Problem::Syntax { line: Some(2), .. }]
        ));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let loaded = parse("from_the_future = true\nsearch_engine = \"bing\"\n");
        assert_eq!(loaded.settings.search_engine, SearchEngine::Bing);
        assert!(loaded.problems.is_empty());
    }

    #[test]
    fn a_start_page_must_be_an_address_a_browser_can_open() {
        assert_eq!(
            parse("start_page = \"search-engine\"").settings.start_page,
            StartPage::SearchEngine
        );
        assert_eq!(
            parse("start_page = \"https://example.com/\"")
                .settings
                .start_page,
            StartPage::Custom(Url::parse("https://example.com/").unwrap())
        );
        let loaded = parse("start_page = \"javascript:alert(1)\"");
        assert_eq!(loaded.settings.start_page, StartPage::SearchEngine);
        assert_eq!(loaded.problems.len(), 1);
    }

    #[test]
    fn the_start_address_follows_the_chosen_search_engine() {
        let mut settings = Settings {
            search_engine: SearchEngine::Brave,
            ..Settings::default()
        };
        assert_eq!(
            settings.start_url().unwrap().as_str(),
            "https://search.brave.com/"
        );
        settings.start_page = StartPage::Custom(Url::parse("https://example.org/").unwrap());
        assert_eq!(
            settings.start_url().unwrap().as_str(),
            "https://example.org/"
        );
    }

    #[test]
    fn an_unreadable_file_is_reported_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the file should be cannot be read as text.
        fs::create_dir(dir.path().join(FILE_NAME)).unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.settings, Settings::default());
        assert!(matches!(
            loaded.problems.as_slice(),
            [Problem::Unreadable { .. }]
        ));
    }
}
