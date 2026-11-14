//! The open tabs, remembered between runs so the next start picks up where
//! the last one left off.

use std::fs;
use std::io;
use std::path::Path;

use toml::{Table, Value};
use url::Url;

use crate::settings::Accent;

const FILE_NAME: &str = "session.toml";

/// One tab as it is remembered: where it was and what it was called.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedTab {
    pub url: Url,
    pub title: String,
    pub pinned: bool,
    /// Index into [`Session::groups`] of the group the tab belongs to.
    pub group: Option<usize>,
}

impl SavedTab {
    /// A plain tab: not pinned and in no group.
    pub fn new(url: Url, title: impl Into<String>) -> Self {
        Self {
            url,
            title: title.into(),
            pinned: false,
            group: None,
        }
    }
}

/// A named, coloured set of tabs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedGroup {
    pub name: String,
    pub color: Accent,
    /// Whether the group's tabs are folded away.
    pub collapsed: bool,
}

/// The tabs that were open, in order, and which of them was in front.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Session {
    pub tabs: Vec<SavedTab>,
    pub groups: Vec<SavedGroup>,
    /// Index into `tabs` of the tab that was in front.
    pub active: usize,
}

impl Session {
    fn to_toml(&self) -> String {
        let tabs = self
            .tabs
            .iter()
            .map(|tab| {
                let mut entry = Table::new();
                entry.insert("url".to_owned(), Value::String(tab.url.to_string()));
                entry.insert("title".to_owned(), Value::String(tab.title.clone()));
                if tab.pinned {
                    entry.insert("pinned".to_owned(), Value::Boolean(true));
                }
                if let Some(group) = tab.group.and_then(|group| i64::try_from(group).ok()) {
                    entry.insert("group".to_owned(), Value::Integer(group));
                }
                Value::Table(entry)
            })
            .collect();
        let mut table = Table::new();
        table.insert(
            "active".to_owned(),
            Value::Integer(i64::try_from(self.active).unwrap_or(0)),
        );
        let groups = self
            .groups
            .iter()
            .map(|group| {
                let mut entry = Table::new();
                entry.insert("name".to_owned(), Value::String(group.name.clone()));
                entry.insert("color".to_owned(), Value::String(group.color.id().into()));
                entry.insert("collapsed".to_owned(), Value::Boolean(group.collapsed));
                Value::Table(entry)
            })
            .collect();
        table.insert("group".to_owned(), Value::Array(groups));
        table.insert("tab".to_owned(), Value::Array(tabs));
        table.to_string()
    }

    /// Reads a session, keeping every tab that is intact. A session file is a
    /// convenience, so damage costs the damaged tabs and nothing more.
    fn parse(text: &str) -> Self {
        let Ok(table) = text.parse::<Table>() else {
            return Self::default();
        };
        let groups: Vec<SavedGroup> = table
            .get("group")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .map(|entry| SavedGroup {
                        name: entry
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        color: entry
                            .get("color")
                            .and_then(Value::as_str)
                            .and_then(Accent::from_id)
                            .unwrap_or_default(),
                        collapsed: entry
                            .get("collapsed")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let tabs: Vec<SavedTab> = table
            .get("tab")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| {
                        let url = Url::parse(entry.get("url")?.as_str()?).ok()?;
                        restorable(&url).then(|| SavedTab {
                            url,
                            title: entry
                                .get("title")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_owned(),
                            pinned: entry
                                .get("pinned")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            // A tab naming a group that is not there is
                            // simply a tab in no group.
                            group: entry
                                .get("group")
                                .and_then(Value::as_integer)
                                .and_then(|group| usize::try_from(group).ok())
                                .filter(|group| *group < groups.len()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let active = table
            .get("active")
            .and_then(Value::as_integer)
            .and_then(|index| usize::try_from(index).ok())
            .filter(|index| *index < tabs.len())
            .unwrap_or(0);
        Self {
            tabs,
            groups,
            active,
        }
    }
}

/// Whether a remembered address is one Lumina should open by itself at start.
fn restorable(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "file")
}

/// Reads the session kept in `dir`. A missing, unreadable or damaged file
/// gives an empty session.
pub fn load_session(dir: &Path) -> Session {
    fs::read_to_string(dir.join(FILE_NAME))
        .map(|text| Session::parse(&text))
        .unwrap_or_default()
}

/// Writes `session` to `dir`, replacing the file in one step.
pub fn save_session(dir: &Path, session: &Session) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let temporary = dir.join(format!("{FILE_NAME}.new"));
    fs::write(&temporary, session.to_toml())?;
    fs::rename(&temporary, dir.join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(url: &str, title: &str) -> SavedTab {
        SavedTab::new(Url::parse(url).unwrap(), title)
    }

    #[test]
    fn what_is_saved_is_what_is_loaded() {
        let dir = tempfile::tempdir().unwrap();
        let session = Session {
            tabs: vec![
                tab("https://example.com/", "Örnek \"alan\""),
                tab("https://search.brave.com/search?q=çay", "çay"),
            ],
            groups: Vec::new(),
            active: 1,
        };
        save_session(dir.path(), &session).unwrap();
        assert_eq!(load_session(dir.path()), session);
    }

    #[test]
    fn pins_and_groups_are_remembered() {
        let dir = tempfile::tempdir().unwrap();
        let mut pinned = tab("https://example.com/", "a");
        pinned.pinned = true;
        let mut grouped = tab("https://example.org/", "b");
        grouped.group = Some(0);
        let session = Session {
            tabs: vec![pinned, grouped],
            groups: vec![SavedGroup {
                name: "İş".to_owned(),
                color: Accent::Olive,
                collapsed: true,
            }],
            active: 0,
        };
        save_session(dir.path(), &session).unwrap();
        assert_eq!(load_session(dir.path()), session);
    }

    #[test]
    fn a_tab_in_a_missing_group_is_in_no_group() {
        let session = Session::parse("[[tab]]\nurl = \"https://example.org/\"\ngroup = 4\n");
        assert_eq!(session.tabs.len(), 1);
        assert_eq!(session.tabs[0].group, None);
    }

    #[test]
    fn no_file_means_no_tabs() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_session(dir.path()), Session::default());
    }

    #[test]
    fn an_empty_session_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        save_session(dir.path(), &Session::default()).unwrap();
        assert_eq!(load_session(dir.path()), Session::default());
    }

    #[test]
    fn damaged_tabs_are_skipped_and_the_rest_kept() {
        let session = Session::parse(
            "active = 7\n\
             [[tab]]\nurl = \"not an address\"\n\
             [[tab]]\nurl = \"https://example.org/\"\n\
             [[tab]]\ntitle = \"no address at all\"\n\
             [[tab]]\nurl = \"javascript:alert(1)\"\n",
        );
        assert_eq!(session.tabs, [tab("https://example.org/", "")]);
        // The remembered front tab no longer exists, so the first is used.
        assert_eq!(session.active, 0);
    }

    #[test]
    fn nonsense_gives_an_empty_session() {
        assert_eq!(Session::parse("= = ="), Session::default());
        assert_eq!(Session::parse("tab = 3"), Session::default());
    }
}
