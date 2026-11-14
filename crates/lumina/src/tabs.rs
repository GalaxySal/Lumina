//! The set of open tabs: their order, which one is in front, and the ones
//! closed recently enough to bring back. Nothing here knows about the engine
//! or the screen, so all of it can be tested directly.

use lumina_store::{SavedTab, Session};
use url::Url;

/// How many closed tabs are remembered for reopening.
const CLOSED_LIMIT: usize = 25;

/// An ordered set of tabs, each carrying a payload `T`, with one in front.
/// There is always at least one tab.
pub struct TabSet<T> {
    tabs: Vec<(u64, T)>,
    active: usize,
    next_id: u64,
    closed: Vec<SavedTab>,
}

impl<T> TabSet<T> {
    /// Starts with a single tab holding `first`.
    pub fn new(first: T) -> Self {
        Self {
            tabs: vec![(0, first)],
            active: 0,
            next_id: 1,
            closed: Vec::new(),
        }
    }

    /// Position of the tab in front.
    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active_id(&self) -> u64 {
        self.tabs.get(self.active).map_or(0, |(id, _)| *id)
    }

    pub fn active(&self) -> Option<&T> {
        self.tabs.get(self.active).map(|(_, tab)| tab)
    }

    pub fn active_mut(&mut self) -> Option<&mut T> {
        self.tabs.get_mut(self.active).map(|(_, tab)| tab)
    }

    pub fn get_mut(&mut self, id: u64) -> Option<&mut T> {
        self.tabs
            .iter_mut()
            .find(|(tab_id, _)| *tab_id == id)
            .map(|(_, tab)| tab)
    }

    pub fn iter(&self) -> impl Iterator<Item = (u64, &T)> {
        self.tabs.iter().map(|(id, tab)| (*id, tab))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.tabs.iter_mut().map(|(_, tab)| tab)
    }

    fn position(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|(tab_id, _)| *tab_id == id)
    }

    /// The id the next added tab will get. Needed by callers that must name
    /// the tab while building its payload.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Adds a tab at the end, or directly after the tab `after`, and returns
    /// its id. The new tab is not brought to the front.
    pub fn add(&mut self, tab: T, after: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let at = after
            .and_then(|after| self.position(after))
            .map_or(self.tabs.len(), |index| index.saturating_add(1));
        self.tabs.insert(at, (id, tab));
        if at <= self.active {
            self.active = self.active.saturating_add(1);
        }
        id
    }

    /// Brings the tab `id` to the front. Returns whether the front tab
    /// changed.
    pub fn select(&mut self, id: u64) -> bool {
        match self.position(id) {
            Some(index) if index != self.active => {
                self.active = index;
                true
            }
            _ => false,
        }
    }

    /// Brings the tab at `index` to the front; an index past the end selects
    /// the last tab.
    pub fn select_index(&mut self, index: usize) -> bool {
        let index = index.min(self.tabs.len().saturating_sub(1));
        let changed = index != self.active;
        self.active = index;
        changed
    }

    /// Brings the next tab to the front (or the previous one), wrapping round.
    pub fn cycle(&mut self, forward: bool) -> bool {
        let count = self.tabs.len();
        if count < 2 {
            return false;
        }
        self.active = if forward {
            self.active
                .saturating_add(1)
                .checked_rem(count)
                .unwrap_or(0)
        } else {
            self.active
                .checked_sub(1)
                .unwrap_or(count.saturating_sub(1))
        };
        true
    }

    /// Moves the tab at position `from` to position `to`. The tab in front
    /// stays in front wherever it ends up.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from == to || from >= self.tabs.len() || to >= self.tabs.len() {
            return false;
        }
        let front = self.active_id();
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = self.position(front).unwrap_or(0);
        true
    }

    /// Puts the tabs in the order their pins and groups call for, changing
    /// as little as possible: pinned tabs come first, and the members of a
    /// group sit together where its first member is. `key` gives whether a
    /// tab is pinned and which group it is in.
    pub fn arrange(&mut self, key: impl Fn(&T) -> (bool, Option<u32>)) {
        let front = self.active_id();
        let (pinned, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.tabs)
            .into_iter()
            .partition(|(_, tab)| key(tab).0);
        let mut arranged = pinned;
        let mut rest: Vec<Option<(u64, T)>> = rest.into_iter().map(Some).collect();
        for index in 0..rest.len() {
            let Some(entry) = rest.get_mut(index).and_then(Option::take) else {
                continue;
            };
            let group = key(&entry.1).1;
            arranged.push(entry);
            let Some(group) = group else {
                continue;
            };
            for later in rest.iter_mut().skip(index.saturating_add(1)) {
                if later
                    .as_ref()
                    .is_some_and(|(_, tab)| key(tab).1 == Some(group))
                    && let Some(entry) = later.take()
                {
                    arranged.push(entry);
                }
            }
        }
        self.tabs = arranged;
        self.active = self.position(front).unwrap_or(0);
    }

    /// Closes the tab `id` and returns its payload. `remember` is what to
    /// keep so the tab can be reopened, if it is worth keeping. When the last
    /// tab is closed, `replacement` takes its place.
    pub fn close(
        &mut self,
        id: u64,
        remember: impl FnOnce(&T) -> Option<SavedTab>,
        replacement: impl FnOnce() -> T,
    ) -> Option<T> {
        let index = self.position(id)?;
        let (_, tab) = self.tabs.remove(index);
        if let Some(saved) = remember(&tab) {
            self.closed.push(saved);
            if self.closed.len() > CLOSED_LIMIT {
                self.closed.remove(0);
            }
        }
        if self.tabs.is_empty() {
            self.add(replacement(), None);
            self.active = 0;
        } else if index < self.active {
            self.active = self.active.saturating_sub(1);
        } else {
            // Closing the front tab brings forward the one that took its
            // place, or the new last tab.
            self.active = self.active.min(self.tabs.len().saturating_sub(1));
        }
        Some(tab)
    }

    /// The most recently closed tab, removed from the list of closed tabs.
    pub fn take_closed(&mut self) -> Option<SavedTab> {
        self.closed.pop()
    }

    /// The tabs as they should be remembered for the next start. `describe`
    /// gives a tab's address and title, or `None` for a tab not worth
    /// restoring.
    pub fn session(&self, describe: impl Fn(&T) -> Option<SavedTab>) -> Session {
        let mut session = Session::default();
        for (index, (_, tab)) in self.tabs.iter().enumerate() {
            if let Some(saved) = describe(tab) {
                if index == self.active {
                    session.active = session.tabs.len();
                }
                session.tabs.push(saved);
            }
        }
        session
    }
}

/// What to call a tab: the page's title if it has one, otherwise its address.
pub fn label(title: Option<&str>, url: Option<&Url>) -> Option<String> {
    match title.map(str::trim) {
        Some(title) if !title.is_empty() => Some(title.to_owned()),
        _ => url.map(|url| {
            let text = url.as_str();
            text.strip_prefix("https://")
                .or_else(|| text.strip_prefix("http://"))
                .unwrap_or(text)
                .trim_end_matches('/')
                .to_owned()
        }),
    }
}

/// Whether a tab at this address should be remembered between runs.
pub fn worth_saving(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "file")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(set: &TabSet<&'static str>) -> Vec<&'static str> {
        set.iter().map(|(_, name)| *name).collect()
    }

    fn set(names: &[&'static str]) -> TabSet<&'static str> {
        let mut names = names.iter();
        let mut set = TabSet::new(*names.next().unwrap());
        for name in names {
            set.add(name, None);
        }
        set
    }

    fn forget(_: &&'static str) -> Option<SavedTab> {
        None
    }

    fn remember(name: &&'static str) -> Option<SavedTab> {
        Some(SavedTab::new(
            Url::parse(&format!("https://{name}.example/")).unwrap(),
            *name,
        ))
    }

    #[test]
    fn a_new_tab_goes_to_the_end_or_right_after_its_opener() {
        let mut tabs = set(&["a", "b", "c"]);
        tabs.add("end", None);
        let opener = tabs.iter().find(|(_, name)| **name == "a").unwrap().0;
        tabs.add("child", Some(opener));
        assert_eq!(order(&tabs), ["a", "child", "b", "c", "end"]);
    }

    #[test]
    fn adding_before_the_front_tab_keeps_it_in_front() {
        let mut tabs = set(&["a", "b", "c"]);
        tabs.select_index(2);
        let opener = tabs.iter().next().unwrap().0;
        tabs.add("child", Some(opener));
        assert_eq!(tabs.active(), Some(&"c"));
    }

    #[test]
    fn closing_the_front_tab_brings_its_neighbour_forward() {
        let mut tabs = set(&["a", "b", "c"]);
        tabs.select_index(1);
        tabs.close(tabs.active_id(), forget, || "new");
        assert_eq!(order(&tabs), ["a", "c"]);
        assert_eq!(tabs.active(), Some(&"c"));
        tabs.close(tabs.active_id(), forget, || "new");
        assert_eq!(tabs.active(), Some(&"a"));
    }

    #[test]
    fn closing_a_tab_before_the_front_one_keeps_the_front_one() {
        let mut tabs = set(&["a", "b", "c"]);
        tabs.select_index(2);
        let first = tabs.iter().next().unwrap().0;
        tabs.close(first, forget, || "new");
        assert_eq!(tabs.active(), Some(&"c"));
    }

    #[test]
    fn closing_the_last_tab_leaves_a_fresh_one() {
        let mut tabs = set(&["only"]);
        tabs.close(tabs.active_id(), forget, || "fresh");
        assert_eq!(order(&tabs), ["fresh"]);
        assert_eq!(tabs.active(), Some(&"fresh"));
    }

    #[test]
    fn closed_tabs_come_back_most_recent_first() {
        let mut tabs = set(&["a", "b", "c"]);
        let ids: Vec<u64> = tabs.iter().map(|(id, _)| id).collect();
        tabs.close(ids[0], remember, || "new");
        tabs.close(ids[1], remember, || "new");
        assert_eq!(tabs.take_closed().unwrap().title, "b");
        assert_eq!(tabs.take_closed().unwrap().title, "a");
        assert!(tabs.take_closed().is_none());
    }

    #[test]
    fn only_so_many_closed_tabs_are_remembered() {
        let mut tabs = set(&["keep"]);
        for _ in 0..CLOSED_LIMIT.saturating_add(5) {
            let id = tabs.add("x", None);
            tabs.close(id, remember, || "new");
        }
        let count = std::iter::from_fn(|| tabs.take_closed()).count();
        assert_eq!(count, CLOSED_LIMIT);
    }

    #[test]
    fn moving_a_tab_keeps_the_front_tab_in_front() {
        let mut tabs = set(&["a", "b", "c", "d"]);
        tabs.select_index(1);
        assert!(tabs.move_tab(0, 3));
        assert_eq!(order(&tabs), ["b", "c", "d", "a"]);
        assert_eq!(tabs.active(), Some(&"b"));
        assert!(tabs.move_tab(0, 2));
        assert_eq!(order(&tabs), ["c", "d", "b", "a"]);
        assert_eq!(tabs.active(), Some(&"b"));
        assert!(!tabs.move_tab(1, 1));
        assert!(!tabs.move_tab(0, 9));
    }

    #[test]
    fn arranging_puts_pins_first_and_keeps_groups_together() {
        // The name says it all: "p…" is pinned, "g1…" is in group 1.
        let key = |name: &&'static str| {
            (
                name.starts_with('p'),
                name.strip_prefix('g')
                    .and_then(|rest| rest.chars().next())
                    .and_then(|digit| digit.to_digit(10)),
            )
        };
        let mut tabs = set(&["a", "g1x", "b", "p1", "g2x", "g1y", "c", "g2y", "p2"]);
        tabs.select_index(6);
        tabs.arrange(key);
        assert_eq!(
            order(&tabs),
            ["p1", "p2", "a", "g1x", "g1y", "b", "g2x", "g2y", "c"]
        );
        assert_eq!(tabs.active(), Some(&"c"));
        // Already in order: nothing moves.
        tabs.arrange(key);
        assert_eq!(
            order(&tabs),
            ["p1", "p2", "a", "g1x", "g1y", "b", "g2x", "g2y", "c"]
        );
    }

    #[test]
    fn cycling_wraps_round_both_ways() {
        let mut tabs = set(&["a", "b", "c"]);
        tabs.cycle(false);
        assert_eq!(tabs.active(), Some(&"c"));
        tabs.cycle(true);
        assert_eq!(tabs.active(), Some(&"a"));
        assert!(!set(&["only"]).cycle(true));
    }

    #[test]
    fn selecting_past_the_end_selects_the_last_tab() {
        let mut tabs = set(&["a", "b", "c"]);
        tabs.select_index(8);
        assert_eq!(tabs.active(), Some(&"c"));
    }

    #[test]
    fn a_session_skips_tabs_not_worth_restoring_and_tracks_the_front_one() {
        let mut tabs = set(&["a", "blank", "c"]);
        tabs.select_index(2);
        let session = tabs.session(|name| (*name != "blank").then(|| remember(name)).flatten());
        assert_eq!(session.tabs.len(), 2);
        assert_eq!(session.tabs[session.active].title, "c");
    }

    #[test]
    fn a_tab_is_called_by_its_title_or_failing_that_its_address() {
        let url = Url::parse("https://example.com/").unwrap();
        assert_eq!(label(Some("Örnek"), Some(&url)).unwrap(), "Örnek");
        assert_eq!(label(Some("  "), Some(&url)).unwrap(), "example.com");
        assert_eq!(label(None, Some(&url)).unwrap(), "example.com");
        assert_eq!(label(None, None), None);
    }
}
