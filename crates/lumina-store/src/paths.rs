use std::ffi::OsString;
use std::path::PathBuf;

/// The directory Lumina keeps its configuration in, following the XDG base
/// directory specification: `$XDG_CONFIG_HOME/lumina`, or `~/.config/lumina`
/// when that variable is unset or not an absolute path.
///
/// Returns `None` when neither location can be determined, in which case
/// settings cannot be saved.
pub fn config_dir() -> Option<PathBuf> {
    config_dir_from(|name| std::env::var_os(name))
}

/// The directory Lumina keeps what it remembers between runs in, such as the
/// open tabs: `$XDG_STATE_HOME/lumina`, or `~/.local/state/lumina`.
pub fn state_dir() -> Option<PathBuf> {
    dir_from(
        |name| std::env::var_os(name),
        "XDG_STATE_HOME",
        ".local/state",
    )
}

fn config_dir_from(variable: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    dir_from(variable, "XDG_CONFIG_HOME", ".config")
}

fn dir_from(
    variable: impl Fn(&str) -> Option<OsString>,
    xdg_variable: &str,
    home_fallback: &str,
) -> Option<PathBuf> {
    let absolute = |name: &str| {
        variable(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = absolute(xdg_variable).or_else(|| Some(absolute("HOME")?.join(home_fallback)))?;
    Some(base.join("lumina"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(pairs: &'static [(&'static str, &'static str)]) -> Option<PathBuf> {
        config_dir_from(|name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        })
    }

    #[test]
    fn xdg_config_home_wins() {
        assert_eq!(
            with(&[("XDG_CONFIG_HOME", "/data/config"), ("HOME", "/home/ayşe")]),
            Some(PathBuf::from("/data/config/lumina"))
        );
    }

    #[test]
    fn home_is_the_fallback() {
        assert_eq!(
            with(&[("HOME", "/home/ayşe")]),
            Some(PathBuf::from("/home/ayşe/.config/lumina"))
        );
    }

    #[test]
    fn a_relative_xdg_path_is_ignored_as_the_specification_requires() {
        assert_eq!(
            with(&[("XDG_CONFIG_HOME", "config"), ("HOME", "/home/ayşe")]),
            Some(PathBuf::from("/home/ayşe/.config/lumina"))
        );
    }

    #[test]
    fn state_has_its_own_place() {
        let variable = |name: &str| (name == "HOME").then(|| OsString::from("/home/ayşe"));
        assert_eq!(
            dir_from(variable, "XDG_STATE_HOME", ".local/state"),
            Some(PathBuf::from("/home/ayşe/.local/state/lumina"))
        );
    }

    #[test]
    fn nothing_to_go_on_gives_nothing() {
        assert_eq!(with(&[]), None);
        assert_eq!(with(&[("HOME", "relative")]), None);
    }
}
