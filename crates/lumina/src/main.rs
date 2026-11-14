//! Lumina, an original desktop web browser.

mod app;
mod tabs;

use anyhow::{Context, Result};
use lumina_store::Settings;
use url::Url;
use winit::event_loop::EventLoop;

use crate::app::{App, Paths, Wake};

const USAGE: &str = "\
Usage: lumina [ADDRESS OR SEARCH WORDS]

With no address, Lumina opens the tabs you had open last time, or your
start page. Text that is not an address is searched for with your search
engine. Both are chosen in Settings.";

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

    let paths = Paths {
        config: lumina_store::config_dir(),
        state: lumina_store::state_dir(),
    };
    if paths.config.is_none() {
        tracing::warn!("no settings folder could be determined; settings will not be kept");
    }
    let loaded = match &paths.config {
        Some(dir) => lumina_store::load(dir),
        None => lumina_store::LoadedSettings {
            settings: Settings::default(),
            problems: Vec::new(),
        },
    };
    for problem in &loaded.problems {
        tracing::warn!(?problem, "the settings file has a problem");
    }

    let requested = requested_url(&arguments, &loaded.settings)?;

    let event_loop = EventLoop::<Wake>::with_user_event()
        .build()
        .context("could not connect to the display server")?;
    let mut app = App::new(requested, loaded, paths, event_loop.create_proxy());
    event_loop
        .run_app(&mut app)
        .context("the window system stopped unexpectedly")?;

    app.into_result()
}

/// The address asked for on the command line, if there was one.
fn requested_url(arguments: &[String], settings: &Settings) -> Result<Option<Url>> {
    if arguments.is_empty() {
        return Ok(None);
    }
    let text = arguments.join(" ");
    lumina_store::resolve(&text, settings.search_engine)
        .map(Some)
        .with_context(|| format!("\"{text}\" could not be turned into an address to open"))
}

#[cfg(test)]
mod tests {
    use lumina_store::SearchEngine;

    use super::*;

    fn run(arguments: &[&str], settings: &Settings) -> Option<Url> {
        let arguments: Vec<String> = arguments.iter().map(|text| (*text).to_owned()).collect();
        requested_url(&arguments, settings).unwrap()
    }

    #[test]
    fn no_argument_asks_for_nothing() {
        assert_eq!(run(&[], &Settings::default()), None);
    }

    #[test]
    fn an_address_is_opened_and_words_are_searched_with_the_chosen_engine() {
        let settings = Settings {
            search_engine: SearchEngine::Brave,
            ..Settings::default()
        };
        assert_eq!(
            run(&["example.com"], &settings).unwrap().as_str(),
            "https://example.com/"
        );
        assert_eq!(
            run(&["rust", "nedir"], &settings).unwrap().as_str(),
            "https://search.brave.com/search?q=rust+nedir"
        );
    }
}
