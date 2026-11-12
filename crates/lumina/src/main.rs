//! Lumina, an original desktop web browser.

mod app;

use anyhow::{Context, Result};
use lumina_store::Settings;
use url::Url;
use winit::event_loop::EventLoop;

use crate::app::{App, Wake};

const USAGE: &str = "\
Usage: lumina [ADDRESS OR SEARCH WORDS]

With no address, Lumina opens your start page.
Text that is not an address is searched for with your search engine.
Both are chosen in Settings.";

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,lumina=info")),
        )
        .init();

    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| argument == "-h" || argument == "--help")
    {
        println!("{USAGE}");
        return Ok(());
    }

    let config_dir = lumina_store::config_dir();
    if config_dir.is_none() {
        tracing::warn!("no settings folder could be determined; settings will not be kept");
    }
    let loaded = match &config_dir {
        Some(dir) => lumina_store::load(dir),
        None => lumina_store::LoadedSettings {
            settings: Settings::default(),
            problems: Vec::new(),
        },
    };
    for problem in &loaded.problems {
        tracing::warn!(?problem, "the settings file has a problem");
    }

    let url = start_url(&arguments, &loaded.settings)?;

    let event_loop = EventLoop::<Wake>::with_user_event()
        .build()
        .context("could not connect to the display server")?;
    let mut app = App::new(url, loaded, config_dir, event_loop.create_proxy());
    event_loop
        .run_app(&mut app)
        .context("the window system stopped unexpectedly")?;

    app.into_result()
}

/// The address to open first: what was typed on the command line, or the
/// user's start page when nothing was.
fn start_url(arguments: &[String], settings: &Settings) -> Result<Url> {
    if arguments.is_empty() {
        return settings
            .start_url()
            .context("the start page address is not valid");
    }
    let text = arguments.join(" ");
    lumina_store::resolve(&text, settings.search_engine)
        .with_context(|| format!("\"{text}\" could not be turned into an address to open"))
}

#[cfg(test)]
mod tests {
    use lumina_store::{SearchEngine, StartPage};

    use super::*;

    fn run(arguments: &[&str], settings: &Settings) -> Url {
        let arguments: Vec<String> = arguments.iter().map(|text| (*text).to_owned()).collect();
        start_url(&arguments, settings).unwrap()
    }

    #[test]
    fn no_argument_opens_the_start_page() {
        let mut settings = Settings::default();
        assert_eq!(run(&[], &settings).as_str(), SearchEngine::default().home());
        settings.start_page = StartPage::Custom(Url::parse("https://example.org/").unwrap());
        assert_eq!(run(&[], &settings).as_str(), "https://example.org/");
    }

    #[test]
    fn an_address_is_opened_and_words_are_searched_with_the_chosen_engine() {
        let settings = Settings {
            search_engine: SearchEngine::Brave,
            ..Settings::default()
        };
        assert_eq!(
            run(&["example.com"], &settings).as_str(),
            "https://example.com/"
        );
        assert_eq!(
            run(&["rust", "nedir"], &settings).as_str(),
            "https://search.brave.com/search?q=rust+nedir"
        );
    }
}
