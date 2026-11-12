//! Search engines, and deciding what the user meant by the text they typed:
//! an address to visit or words to search for.

use std::net::IpAddr;

use url::Url;

/// The search engines Lumina knows how to query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SearchEngine {
    /// The default: it does not profile the people who use it.
    #[default]
    DuckDuckGo,
    Brave,
    Startpage,
    Google,
    Bing,
    Yandex,
}

impl SearchEngine {
    pub const ALL: [Self; 6] = [
        Self::DuckDuckGo,
        Self::Brave,
        Self::Startpage,
        Self::Google,
        Self::Bing,
        Self::Yandex,
    ];

    /// The name shown to the user.
    pub fn name(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "DuckDuckGo",
            Self::Brave => "Brave Search",
            Self::Startpage => "Startpage",
            Self::Google => "Google",
            Self::Bing => "Bing",
            Self::Yandex => "Yandex",
        }
    }

    /// The short name used in the settings file and on the command line.
    pub fn id(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "duckduckgo",
            Self::Brave => "brave",
            Self::Startpage => "startpage",
            Self::Google => "google",
            Self::Bing => "bing",
            Self::Yandex => "yandex",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|engine| engine.id().eq_ignore_ascii_case(id.trim()))
    }

    /// The engine's front page, opened when Lumina starts without an address.
    pub fn home(self) -> &'static str {
        match self {
            Self::DuckDuckGo => "https://duckduckgo.com/",
            Self::Brave => "https://search.brave.com/",
            Self::Startpage => "https://www.startpage.com/",
            Self::Google => "https://www.google.com/",
            Self::Bing => "https://www.bing.com/",
            Self::Yandex => "https://yandex.com/",
        }
    }

    /// The results page address and the name of its query parameter.
    fn endpoint(self) -> (&'static str, &'static str) {
        match self {
            Self::DuckDuckGo => ("https://duckduckgo.com/", "q"),
            Self::Brave => ("https://search.brave.com/search", "q"),
            Self::Startpage => ("https://www.startpage.com/sp/search", "query"),
            Self::Google => ("https://www.google.com/search", "q"),
            Self::Bing => ("https://www.bing.com/search", "q"),
            Self::Yandex => ("https://yandex.com/search/", "text"),
        }
    }

    /// The address of the results page for `query`.
    pub fn search(self, query: &str) -> Result<Url, url::ParseError> {
        let (endpoint, parameter) = self.endpoint();
        Url::parse_with_params(endpoint, [(parameter, query)])
    }
}

/// Turns typed text into the address to open.
///
/// Text that names a place on the web is visited (`example.com` becomes
/// `https://example.com`); anything else is searched for with `engine`.
/// Empty text opens the engine's front page.
pub fn resolve(input: &str, engine: SearchEngine) -> Result<Url, url::ParseError> {
    let text = input.trim();
    if text.is_empty() {
        return Url::parse(engine.home());
    }
    if let Some(url) = explicit_url(text) {
        return Ok(url);
    }
    if looks_like_host(text)
        && let Ok(url) = Url::parse(&format!("https://{text}"))
    {
        return Ok(url);
    }
    engine.search(text)
}

/// Text that already says how to reach it, such as `https://…` or `about:blank`.
fn explicit_url(text: &str) -> Option<Url> {
    if text.contains(char::is_whitespace) {
        return None;
    }
    let url = Url::parse(text).ok()?;
    match url.scheme() {
        "http" | "https" => url.has_host().then_some(url),
        "file" | "about" | "data" | "blob" | "view-source" => Some(url),
        // `localhost:8000` parses as a URL with scheme `localhost`, and
        // `word:word` is far likelier to be a search than an unknown scheme.
        _ => None,
    }
}

/// Whether `text`, lacking a scheme, reads as a host name rather than as words.
fn looks_like_host(text: &str) -> bool {
    if text.contains(char::is_whitespace) {
        return false;
    }
    let authority = text.split(['/', '?', '#']).next().unwrap_or(text);
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            (host, Some(port))
        }
        _ => (authority, None),
    };
    if host.is_empty() {
        return false;
    }
    if host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok() {
        return true;
    }
    if host.starts_with('[') && host.ends_with(']') {
        return true;
    }
    // A single word with a port (`intranet:8080`) is a host; a single bare
    // word (`kedi`) is a search.
    if !host.contains('.') {
        return port.is_some();
    }
    // `example.com`, but not `3.14`, `.com` or `e.g.`.
    let Some((name, suffix)) = host.rsplit_once('.') else {
        return false;
    };
    !name.is_empty() && suffix.len() >= 2 && suffix.chars().all(|c| c.is_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(text: &str) -> String {
        resolve(text, SearchEngine::default()).unwrap().into()
    }

    #[test]
    fn nothing_typed_opens_the_search_engine() {
        assert_eq!(open(""), "https://duckduckgo.com/");
        assert_eq!(open("   "), "https://duckduckgo.com/");
    }

    #[test]
    fn addresses_are_visited() {
        assert_eq!(open("example.com"), "https://example.com/");
        assert_eq!(open("example.com/yol?a=1"), "https://example.com/yol?a=1");
        assert_eq!(open("http://example.com"), "http://example.com/");
        assert_eq!(open("localhost:8000"), "https://localhost:8000/");
        assert_eq!(open("192.168.1.1"), "https://192.168.1.1/");
        assert_eq!(open("intranet:8080/panel"), "https://intranet:8080/panel");
        assert_eq!(open("about:blank"), "about:blank");
        assert_eq!(open("türkiye.gov.tr"), "https://xn--trkiye-3ya.gov.tr/");
    }

    #[test]
    fn words_are_searched_for() {
        assert_eq!(open("kedi"), "https://duckduckgo.com/?q=kedi");
        assert_eq!(
            open("rust nedir?"),
            "https://duckduckgo.com/?q=rust+nedir%3F"
        );
        assert_eq!(
            open("çay & şeker"),
            "https://duckduckgo.com/?q=%C3%A7ay+%26+%C5%9Feker"
        );
        assert_eq!(open("3.14"), "https://duckduckgo.com/?q=3.14");
        assert_eq!(open("e.g."), "https://duckduckgo.com/?q=e.g.");
        assert_eq!(
            open("not: bir şey"),
            "https://duckduckgo.com/?q=not%3A+bir+%C5%9Fey"
        );
        assert_eq!(
            open("c++:nedir"),
            "https://duckduckgo.com/?q=c%2B%2B%3Anedir"
        );
    }

    #[test]
    fn every_engine_builds_a_results_address_on_its_own_site() {
        for engine in SearchEngine::ALL {
            let results = engine.search("lumina tarayıcı").unwrap();
            let home = Url::parse(engine.home()).unwrap();
            assert_eq!(results.scheme(), "https");
            assert_eq!(results.host_str(), home.host_str(), "{}", engine.name());
            assert!(
                results
                    .query_pairs()
                    .any(|(_, value)| value == "lumina tarayıcı"),
                "{}",
                engine.name()
            );
        }
    }

    #[test]
    fn engines_are_found_by_their_short_name() {
        assert_eq!(SearchEngine::from_id("brave"), Some(SearchEngine::Brave));
        assert_eq!(
            SearchEngine::from_id(" Google "),
            Some(SearchEngine::Google)
        );
        assert_eq!(SearchEngine::from_id("altavista"), None);
    }
}
