use egui::text::{CCursor, CCursorRange};
use egui::{Align, Frame, Id, Key, Layout, Margin, Panel, Pos2, Stroke, TextEdit, Ui, Vec2};
use lumina_store::Settings;
use lumina_theme::{FOCUS_WIDTH, HAIRLINE, RADIUS, Theme, mono, size, space};

use crate::text::Strings;
use crate::widgets::{Icon, icon_button};
use crate::{Action, Chrome, Output, PageState, Screen};

const HEIGHT: f32 = 46.0;
const FIELD_HEIGHT: f32 = 30.0;

fn address_id() -> Id {
    Id::new("lumina-address-bar")
}

pub(crate) fn show(
    chrome: &mut Chrome,
    ui: &mut Ui,
    theme: &Theme,
    settings: &Settings,
    strings: &Strings,
    page: PageState<'_>,
    output: &mut Output,
) {
    let p = theme.palette;
    let panel = Panel::top("lumina-toolbar")
        .exact_size(HEIGHT)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(p.raised)
                .inner_margin(Margin::symmetric(8, 0)),
        )
        .show(ui, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(space::XS, 0.0);

                if icon_button(ui, theme, Icon::Back, strings.back, page.can_go_back).clicked() {
                    output.actions.push(Action::Back);
                }
                if icon_button(
                    ui,
                    theme,
                    Icon::Forward,
                    strings.forward,
                    page.can_go_forward,
                )
                .clicked()
                {
                    output.actions.push(Action::Forward);
                }
                if icon_button(ui, theme, Icon::Reload, strings.reload, true).clicked() {
                    output.actions.push(Action::Reload);
                }

                ui.add_space(space::S);
                // Laid out from the right so the address bar takes what is left.
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, theme, Icon::Settings, strings.settings, true).clicked() {
                        chrome.toggle_settings();
                    }
                    ui.add_space(space::S);
                    address_bar(chrome, ui, theme, settings, strings, page, output);
                });
            });
        });

    // The toolbar's lower edge: a hairline, inked in the accent while a page
    // is loading.
    let edge = panel.response.rect;
    let (width, color) = if page.loading && chrome.screen == Screen::Page {
        (FOCUS_WIDTH, p.accent)
    } else {
        (HAIRLINE, p.rule)
    };
    let y = edge.bottom() - width / 2.0;
    ui.painter().line_segment(
        [Pos2::new(edge.left(), y), Pos2::new(edge.right(), y)],
        Stroke::new(width, color),
    );
}

fn address_bar(
    chrome: &mut Chrome,
    ui: &mut Ui,
    theme: &Theme,
    settings: &Settings,
    strings: &Strings,
    page: PageState<'_>,
    output: &mut Output,
) {
    let p = theme.palette;
    let id = address_id();

    if std::mem::take(&mut chrome.focus_address) {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    let focused = ui.memory(|memory| memory.has_focus(id));
    if !focused {
        // While nobody is typing, the bar shows where the page really is.
        chrome.address = page.url.map(Url::to_string).unwrap_or_default();
    }

    let outline = if focused {
        Stroke::new(FOCUS_WIDTH, p.accent)
    } else {
        Stroke::new(HAIRLINE, p.rule)
    };
    let edit = TextEdit::singleline(&mut chrome.address)
        .id(id)
        .font(mono(size::MONO))
        .text_color(p.ink)
        .hint_text(strings.address_hint)
        .desired_width(ui.available_width())
        .min_size(Vec2::new(0.0, FIELD_HEIGHT))
        .vertical_align(Align::Center)
        .frame(
            Frame::new()
                .fill(p.field)
                .stroke(outline)
                .corner_radius(RADIUS)
                .inner_margin(Margin::symmetric(10, 0)),
        )
        .show(ui);

    if edit.response.gained_focus() {
        // Typing replaces the address, as people expect of an address bar.
        let mut state = edit.state;
        let end = CCursor::new(chrome.address.chars().count());
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(0), end)));
        state.store(ui.ctx(), id);
    }

    if edit.response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
        // Text that cannot be made into an address is left in the bar for the
        // user to correct.
        if let Ok(url) = lumina_store::resolve(&chrome.address, settings.search_engine) {
            chrome.screen = Screen::Page;
            output.actions.push(Action::Open(url));
        }
    }
}

use url::Url;
