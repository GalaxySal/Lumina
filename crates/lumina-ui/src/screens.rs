//! Lumina's full-window screens. Each has a layout of its own, suited to what
//! it holds: settings read like a form with labels in the margin, the
//! first-run screen like the first page of a letter.

use egui::{
    Align, Align2, Frame, Layout, Margin, Pos2, RichText, ScrollArea, Sense, Stroke, TextEdit, Ui,
    Vec2,
};
use lumina_store::{
    Accent, AddressBar, Appearance, Language, Problem, SearchEngine, Settings, StartPage, TabLayout,
};
use lumina_theme::{
    FOCUS_WIDTH, HAIRLINE, MEASURE, RADIUS, Theme, accent_color, mono, sans, sans_medium, serif,
    size, space,
};
use url::Url;

use crate::text::Strings;
use crate::widgets::{button, choice, rule, swatch};
use crate::{Action, Chrome, Output, Screen};

/// Width of the margin column that holds section labels.
const MARGIN_COLUMN: f32 = 190.0;
/// Below this width the labels move above their sections.
const NARROW: f32 = 640.0;

fn paper(ui: &mut Ui, theme: &Theme, add_contents: impl FnOnce(&mut Ui)) {
    Frame::new().fill(theme.palette.paper).show(ui, |ui| {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, add_contents);
    });
}

fn text(ui: &mut Ui, content: &str, font: egui::FontId, color: egui::Color32) {
    ui.label(RichText::new(content).font(font).color(color));
}

pub(crate) fn settings(
    chrome: &mut Chrome,
    ui: &mut Ui,
    theme: &Theme,
    settings: &Settings,
    strings: &Strings,
    output: &mut Output,
) {
    let p = theme.palette;
    let mut next = settings.clone();

    paper(ui, theme, |ui| {
        let narrow = ui.available_width() < NARROW;
        let side = if narrow { space::L } else { space::XXL };
        Frame::new()
            .inner_margin(Margin::symmetric(side as i8, space::XL as i8))
            .show(ui, |ui| {
                ui.set_max_width(MARGIN_COLUMN + MEASURE);

                ui.horizontal(|ui| {
                    text(ui, strings.settings_title, serif(size::TITLE), p.ink);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, theme, strings.return_to_page, false).clicked() {
                            chrome.screen = Screen::Page;
                        }
                    });
                });
                ui.add_space(space::M);
                rule(ui, theme);
                ui.add_space(space::L);

                report_problems(chrome, ui, theme, strings);

                section(ui, theme, narrow, strings.search_engine, |ui| {
                    search_engines(ui, theme, &mut next);
                    ui.add_space(space::XS);
                    text(
                        ui,
                        strings.search_engine_note,
                        sans(size::SMALL),
                        p.ink_muted,
                    );
                });

                section(ui, theme, narrow, strings.start_page, |ui| {
                    start_page(chrome, ui, theme, strings, &mut next);
                });

                section(ui, theme, narrow, strings.tabs, |ui| {
                    for (value, label) in [
                        (TabLayout::Top, strings.tabs_top),
                        (TabLayout::Side, strings.tabs_side),
                    ] {
                        if choice(ui, theme, next.tab_layout == value, label, None).clicked() {
                            next.tab_layout = value;
                        }
                    }
                });

                section(ui, theme, narrow, strings.address_bar, |ui| {
                    for (value, label) in [
                        (AddressBar::Wide, strings.address_wide),
                        (AddressBar::Centered, strings.address_centered),
                    ] {
                        if choice(ui, theme, next.address_bar == value, label, None).clicked() {
                            next.address_bar = value;
                        }
                    }
                });

                section(ui, theme, narrow, strings.appearance, |ui| {
                    appearance(ui, theme, strings, &mut next);
                });

                section(ui, theme, narrow, strings.accent, |ui| {
                    accents(ui, theme, strings, &mut next);
                });

                section(ui, theme, narrow, strings.language, |ui| {
                    languages(ui, theme, strings, &mut next);
                });

                match &chrome.settings_file {
                    Some(path) => {
                        text(ui, strings.stored_in, sans(size::SMALL), p.ink_muted);
                        text(
                            ui,
                            &path.display().to_string(),
                            mono(size::SMALL),
                            p.ink_muted,
                        );
                    }
                    None => text(ui, strings.not_stored, sans(size::SMALL), p.danger),
                }
                ui.add_space(space::XL);
            });
    });

    if next != *settings {
        output.actions.push(Action::ChangeSettings(next));
    }
}

/// One titled group of settings: its label in the margin, its controls beside.
fn section(
    ui: &mut Ui,
    theme: &Theme,
    narrow: bool,
    label: &str,
    add_contents: impl FnOnce(&mut Ui),
) {
    let p = theme.palette;
    if narrow {
        text(ui, label, sans_medium(size::SMALL), p.ink_muted);
        ui.add_space(space::XS);
        ui.vertical(add_contents);
    } else {
        ui.horizontal_top(|ui| {
            let (cell, _) =
                ui.allocate_exact_size(Vec2::new(MARGIN_COLUMN, size::BODY * 1.6), Sense::hover());
            ui.painter().text(
                Pos2::new(cell.left(), cell.top() + space::XS),
                Align2::LEFT_TOP,
                label,
                sans_medium(size::SMALL),
                p.ink_muted,
            );
            ui.vertical(add_contents);
        });
    }
    ui.add_space(space::L);
    rule(ui, theme);
    ui.add_space(space::L);
}

fn search_engines(ui: &mut Ui, theme: &Theme, next: &mut Settings) {
    for engine in SearchEngine::ALL {
        let host = engine
            .home()
            .trim_start_matches("https://")
            .trim_end_matches('/');
        let note = Some((host, mono(size::SMALL)));
        if choice(ui, theme, next.search_engine == engine, engine.name(), note).clicked() {
            next.search_engine = engine;
        }
    }
}

fn start_page(
    chrome: &mut Chrome,
    ui: &mut Ui,
    theme: &Theme,
    strings: &Strings,
    next: &mut Settings,
) {
    let p = theme.palette;
    if choice(
        ui,
        theme,
        !chrome.start_is_custom,
        strings.start_with_search_engine,
        None,
    )
    .clicked()
    {
        chrome.start_is_custom = false;
        next.start_page = StartPage::SearchEngine;
    }
    if choice(
        ui,
        theme,
        chrome.start_is_custom,
        strings.start_with_address,
        None,
    )
    .clicked()
    {
        chrome.start_is_custom = true;
    }

    if !chrome.start_is_custom {
        return;
    }
    let address = parse_start_address(&chrome.start_address);
    let empty = chrome.start_address.trim().is_empty();
    let outline = if address.is_none() && !empty {
        Stroke::new(FOCUS_WIDTH, p.danger)
    } else {
        Stroke::new(HAIRLINE, p.rule)
    };
    ui.add_space(space::XS);
    ui.horizontal(|ui| {
        ui.add_space(space::L + space::XS);
        TextEdit::singleline(&mut chrome.start_address)
            .font(mono(size::MONO))
            .text_color(p.ink)
            .hint_text(strings.start_address_hint)
            .desired_width(360.0_f32.min(ui.available_width()))
            .frame(
                Frame::new()
                    .fill(p.field)
                    .stroke(outline)
                    .corner_radius(RADIUS)
                    .inner_margin(Margin::symmetric(10, 6)),
            )
            .show(ui);
    });
    match parse_start_address(&chrome.start_address) {
        Some(url) => next.start_page = StartPage::Custom(url),
        None => {
            // Until the text is a usable address the search engine opens.
            next.start_page = StartPage::SearchEngine;
            if !chrome.start_address.trim().is_empty() {
                ui.horizontal(|ui| {
                    ui.add_space(space::L + space::XS);
                    text(
                        ui,
                        strings.start_address_invalid,
                        sans(size::SMALL),
                        p.danger,
                    );
                });
            }
        }
    }
}

/// What the user typed as a start address, if it is one a browser can open.
/// A bare host such as `example.com` is taken to mean `https://example.com`.
fn parse_start_address(input: &str) -> Option<Url> {
    let input = input.trim();
    if input.is_empty() || input.contains(char::is_whitespace) {
        return None;
    }
    if let Ok(url) = Url::parse(input) {
        return match url.scheme() {
            "http" | "https" => url.has_host().then_some(url),
            "file" | "about" => Some(url),
            _ => None,
        };
    }
    if !input.contains('.') {
        return None;
    }
    Url::parse(&format!("https://{input}"))
        .ok()
        .filter(Url::has_host)
}

fn appearance(ui: &mut Ui, theme: &Theme, strings: &Strings, next: &mut Settings) {
    for value in Appearance::ALL {
        let label = match value {
            Appearance::System => strings.appearance_system,
            Appearance::Light => strings.appearance_light,
            Appearance::Dark => strings.appearance_dark,
        };
        if choice(ui, theme, next.appearance == value, label, None).clicked() {
            next.appearance = value;
        }
    }
}

fn accents(ui: &mut Ui, theme: &Theme, strings: &Strings, next: &mut Settings) {
    ui.horizontal_wrapped(|ui| {
        for value in Accent::ALL {
            let label = match value {
                Accent::Brick => strings.accent_brick,
                Accent::Indigo => strings.accent_indigo,
                Accent::Olive => strings.accent_olive,
            };
            let color = accent_color(value, theme.dark);
            if swatch(ui, theme, color, next.accent == value, label).clicked() {
                next.accent = value;
            }
        }
    });
}

fn languages(ui: &mut Ui, theme: &Theme, strings: &Strings, next: &mut Settings) {
    for value in Language::ALL {
        // A language is named in itself, so it can be found by someone who
        // cannot read the current one.
        let label = match value {
            Language::System => strings.language_system,
            Language::Turkish => "Türkçe",
            Language::English => "English",
        };
        if choice(ui, theme, next.language == value, label, None).clicked() {
            next.language = value;
        }
    }
}

fn report_problems(chrome: &Chrome, ui: &mut Ui, theme: &Theme, strings: &Strings) {
    let p = theme.palette;
    if chrome.problems.is_empty() && chrome.save_error.is_none() {
        return;
    }
    for problem in &chrome.problems {
        let message = match problem {
            Problem::Unreadable { .. } => strings.problem_unreadable.to_owned(),
            Problem::Syntax { line, .. } => match line {
                Some(line) => format!("{} ({} {line})", strings.problem_syntax, strings.line),
                None => strings.problem_syntax.to_owned(),
            },
            Problem::InvalidValue { key, line } => match line {
                Some(line) => format!("{} {key} ({} {line})", strings.problem_value, strings.line),
                None => format!("{} {key}", strings.problem_value),
            },
        };
        text(ui, &message, sans(size::BODY), p.danger);
    }
    if let Some(error) = &chrome.save_error {
        text(
            ui,
            &format!("{} {error}", strings.save_failed),
            sans(size::BODY),
            p.danger,
        );
    }
    ui.add_space(space::L);
    rule(ui, theme);
    ui.add_space(space::L);
}

pub(crate) fn setup(
    chrome: &mut Chrome,
    ui: &mut Ui,
    theme: &Theme,
    settings: &Settings,
    strings: &Strings,
    output: &mut Output,
) {
    let p = theme.palette;
    let mut next = settings.clone();
    let mut begin = false;

    paper(ui, theme, |ui| {
        // A single column, centred like a page in a book.
        let column = MEASURE.min(ui.available_width() - space::L * 2.0).max(0.0);
        let side = ((ui.available_width() - column) / 2.0).max(0.0);
        ui.horizontal_top(|ui| {
            ui.add_space(side);
            ui.vertical(|ui| {
                ui.set_width(column);
                ui.add_space(space::XXL);

                // The language can be changed before anything else is read.
                ui.horizontal(|ui| {
                    for (value, label) in [
                        (Language::Turkish, "Türkçe"),
                        (Language::English, "English"),
                    ] {
                        let current = match next.language {
                            Language::System => {
                                std::ptr::eq(strings, crate::Lang::Turkish.strings())
                                    == (value == Language::Turkish)
                            }
                            chosen => chosen == value,
                        };
                        if choice(ui, theme, current, label, None).clicked() {
                            next.language = value;
                        }
                    }
                });
                ui.add_space(space::L);

                text(ui, strings.welcome, serif(size::DISPLAY), p.ink);
                ui.add_space(space::S);
                text(ui, strings.welcome_lead, sans(size::BODY), p.ink);
                ui.add_space(space::XL);

                numbered(ui, theme, 1, strings.search_engine);
                search_engines(ui, theme, &mut next);
                ui.add_space(space::XL);

                numbered(ui, theme, 2, strings.appearance);
                appearance(ui, theme, strings, &mut next);
                ui.add_space(space::S);
                accents(ui, theme, strings, &mut next);
                ui.add_space(space::XL);

                rule(ui, theme);
                ui.add_space(space::M);
                text(ui, strings.welcome_caveat, sans(size::SMALL), p.ink_muted);
                ui.add_space(space::L);
                if button(ui, theme, strings.begin, true).clicked() {
                    begin = true;
                }
                if let Some(error) = &chrome.save_error {
                    ui.add_space(space::S);
                    text(
                        ui,
                        &format!("{} {error}", strings.save_failed),
                        sans(size::SMALL),
                        p.danger,
                    );
                }
                ui.add_space(space::XXL);
            });
        });
    });

    if begin {
        next.setup_complete = true;
        chrome.screen = Screen::Page;
        if let Ok(url) = next.start_url() {
            output.actions.push(Action::Open(url));
        }
    }
    if next != *settings {
        output.actions.push(Action::ChangeSettings(next));
    }
}

/// A heading with its step number set in the accent, as in a printed form.
fn numbered(ui: &mut Ui, theme: &Theme, number: u8, label: &str) {
    let p = theme.palette;
    ui.horizontal(|ui| {
        text(ui, &number.to_string(), serif(size::HEADING), p.accent);
        ui.add_space(space::XS);
        text(ui, label, serif(size::HEADING), p.ink);
    });
    ui.add_space(space::XS);
}

/// Divides the page area in two when pages are shown side by side, and draws
/// the rule between them and a line of accent over the one in front. Returns
/// the front page's area and the other's.
pub(crate) fn panes(
    ui: &Ui,
    theme: &Theme,
    whole: egui::Rect,
    split: Option<bool>,
) -> (egui::Rect, Option<egui::Rect>) {
    let Some(front_on_right) = split else {
        return (whole, None);
    };
    let p = theme.palette;
    let middle = whole.center().x.round();
    let left = egui::Rect::from_min_max(whole.min, Pos2::new(middle, whole.max.y));
    let right = egui::Rect::from_min_max(Pos2::new(middle + HAIRLINE, whole.min.y), whole.max);
    let painter = ui.painter();
    painter.line_segment(
        [
            Pos2::new(middle + HAIRLINE / 2.0, whole.min.y),
            Pos2::new(middle + HAIRLINE / 2.0, whole.max.y),
        ],
        Stroke::new(HAIRLINE, p.rule),
    );
    let (front, other) = if front_on_right {
        (right, left)
    } else {
        (left, right)
    };
    let y = front.min.y + FOCUS_WIDTH / 2.0;
    painter.line_segment(
        [Pos2::new(front.min.x, y), Pos2::new(front.max.x, y)],
        Stroke::new(FOCUS_WIDTH, p.accent),
    );
    (front, Some(other))
}

/// Shows where the link under the pointer leads, at the foot of the page.
pub(crate) fn status_line(ui: &Ui, theme: &Theme, page: egui::Rect, status: &str) {
    let p = theme.palette;
    let painter = ui.painter();
    let mut job = egui::text::LayoutJob::single_section(
        status.to_owned(),
        egui::TextFormat {
            font_id: mono(size::SMALL),
            color: p.ink,
            ..egui::TextFormat::default()
        },
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width((page.width() * 0.6).max(120.0));
    let galley = painter.layout_job(job);
    let padding = Vec2::new(space::S, space::XS);
    let height = galley.size().y + padding.y * 2.0;
    let rect = egui::Rect::from_min_size(
        Pos2::new(page.min.x, page.max.y - height),
        Vec2::new(galley.size().x + padding.x * 2.0, height),
    );
    painter.rect_filled(
        rect,
        egui::CornerRadius {
            nw: 0,
            ne: RADIUS,
            sw: 0,
            se: 0,
        },
        p.raised,
    );
    painter.galley(
        Pos2::new(rect.min.x + padding.x, rect.min.y + padding.y),
        galley,
        p.ink,
    );
}

/// What a tab shows before it has a page: the name, and one field to say
/// where to go. Laid out like a title page, a third of the way down.
pub(crate) fn new_tab(
    chrome: &mut Chrome,
    ui: &mut Ui,
    theme: &Theme,
    settings: &Settings,
    strings: &Strings,
    output: &mut Output,
) {
    let p = theme.palette;
    let id = egui::Id::new("lumina-new-tab-field");
    Frame::new().fill(p.paper).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        let column = 560.0_f32
            .min(ui.available_width() - space::L * 2.0)
            .max(0.0);
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.26).max(space::XL));
            text(ui, "Lumina", serif(size::DISPLAY), p.ink);
            ui.add_space(space::L);

            // Nothing else on this page takes typing, so the field has it.
            if ui.memory(|memory| memory.focused().is_none()) {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            let focused = ui.memory(|memory| memory.has_focus(id));
            let outline = if focused {
                Stroke::new(FOCUS_WIDTH, p.accent)
            } else {
                Stroke::new(HAIRLINE, p.rule)
            };
            let field = TextEdit::singleline(&mut chrome.new_tab_query)
                .id(id)
                .font(sans(size::HEADING))
                .text_color(p.ink)
                .hint_text(strings.address_hint)
                .desired_width(column - 30.0)
                .frame(
                    Frame::new()
                        .fill(p.field)
                        .stroke(outline)
                        .corner_radius(RADIUS)
                        .inner_margin(Margin::symmetric(14, 10)),
                )
                .show(ui);
            if field.response.lost_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Enter))
                && !chrome.new_tab_query.trim().is_empty()
                && let Ok(url) =
                    lumina_store::resolve(&chrome.new_tab_query, settings.search_engine)
            {
                chrome.new_tab_query.clear();
                output.actions.push(Action::Open(url));
            }

            ui.add_space(space::M);
            text(
                ui,
                &format!(
                    "{} {}",
                    strings.searched_with,
                    settings.search_engine.name()
                ),
                sans(size::SMALL),
                p.ink_muted,
            );
        });
    });
}

pub(crate) fn crashed(ui: &mut Ui, theme: &Theme, strings: &Strings, output: &mut Output) {
    let p = theme.palette;
    Frame::new().fill(p.paper).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 3.0).max(space::XL));
            text(ui, strings.page_crashed, serif(size::TITLE), p.ink);
            ui.add_space(space::S);
            text(ui, strings.page_crashed_note, sans(size::BODY), p.ink_muted);
            ui.add_space(space::L);
            if button(ui, theme, strings.reload, true).clicked() {
                output.actions.push(Action::Reload);
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_addresses_a_browser_can_open_are_accepted() {
        let open = |text| parse_start_address(text).map(String::from);
        assert_eq!(
            open("https://example.com"),
            Some("https://example.com/".into())
        );
        assert_eq!(
            open("  example.com/haber "),
            Some("https://example.com/haber".into())
        );
        assert_eq!(open("about:blank"), Some("about:blank".into()));
    }

    #[test]
    fn anything_else_is_refused() {
        for text in [
            "",
            "   ",
            "kedi",
            "iki kelime",
            "javascript:alert(1)",
            "https://",
        ] {
            assert_eq!(parse_start_address(text), None, "{text:?}");
        }
    }
}
