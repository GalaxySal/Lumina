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

fn config_dir_from(variable: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let absolute = |name: &str| {
        variable(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = absolute("XDG_CONFIG_HOME").or_else(|| Some(absolute("HOME")?.join(".config")))?;
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
    fn nothing_to_go_on_gives_nothing() {
        assert_eq!(with(&[]), None);
        assert_eq!(with(&[("HOME", "relative")]), None);
    }
}
