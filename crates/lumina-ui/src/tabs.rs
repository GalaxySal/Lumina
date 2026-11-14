//! The tabs, drawn either as a strip across the top or a column down the side.
//!
//! A tab is styled like an index tab on a paper folder: the one in front
//! takes the colour of the toolbar it leads to and carries a line of accent.
//! Pinned tabs come first and are kept small; grouped tabs follow a label in
//! their group's colour.

use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align, Color32, CornerRadius, CursorIcon, Frame, Id, Layout, Margin, Panel, Pos2, Rect,
    Response, ScrollArea, Sense, Stroke, TextEdit, TextFormat, TextureId, Ui, Vec2, WidgetInfo,
    WidgetType,
};
use lumina_store::Accent;
use lumina_theme::{
    FOCUS_WIDTH, HAIRLINE, RADIUS, Theme, accent_color, sans, sans_medium, size, space,
};

use crate::text::Strings;
use crate::widgets::focus_ring;
use crate::{Action, Output};

/// What the interface needs to know about one tab.
#[derive(Clone, Copy, Debug)]
pub struct TabInfo<'a> {
    /// Identifies the tab for as long as it is open, wherever it is moved.
    pub id: u64,
    /// The page's title, or `None` for a tab with nothing to call it yet.
    pub title: Option<&'a str>,
    pub loading: bool,
    pub favicon: Option<TextureId>,
    pub pinned: bool,
    /// The id of the group the tab belongs to.
    pub group: Option<u32>,
}

/// What the interface needs to know about one group of tabs.
#[derive(Clone, Copy, Debug)]
pub struct GroupInfo<'a> {
    pub id: u32,
    pub name: &'a str,
    pub color: Accent,
    /// Whether the group's tabs are folded away.
    pub collapsed: bool,
}

/// Everything the tab strip is drawn from.
#[derive(Clone, Copy)]
pub(crate) struct Strip<'a> {
    pub theme: &'a Theme,
    pub strings: &'a Strings,
    pub tabs: &'a [TabInfo<'a>],
    pub groups: &'a [GroupInfo<'a>],
    pub active: usize,
    /// Whether two pages are being shown side by side.
    pub split: bool,
}

const STRIP_HEIGHT: f32 = 36.0;
const TAB_HEIGHT: f32 = 30.0;
const TAB_MIN_WIDTH: f32 = 44.0;
const TAB_MAX_WIDTH: f32 = 230.0;
const PINNED_WIDTH: f32 = 36.0;
const TAB_GAP: f32 = 2.0;
const COLUMN_WIDTH: f32 = 224.0;
const ROW_HEIGHT: f32 = 32.0;
const CHIP_HEIGHT: f32 = 20.0;
const CHIP_MAX_WIDTH: f32 = 140.0;
const SMALL_BUTTON: f32 = 22.0;
const FAVICON: f32 = 16.0;

/// Which way the tabs run.
#[derive(Clone, Copy, PartialEq)]
enum Run {
    Across,
    Down,
}

/// One thing in the strip, in order.
enum Entry<'a> {
    Chip(&'a GroupInfo<'a>),
    Tab(usize, &'a TabInfo<'a>),
}

impl<'a> Strip<'a> {
    fn group(&self, id: Option<u32>) -> Option<&'a GroupInfo<'a>> {
        let id = id?;
        self.groups.iter().find(|group| group.id == id)
    }

    /// The labels and tabs to draw, in order. A folded group shows only its
    /// label, and the tab in front if that tab is in it.
    fn entries(&self) -> Vec<Entry<'a>> {
        let mut entries = Vec::new();
        let mut previous = None;
        for (index, tab) in self.tabs.iter().enumerate() {
            let group = self.group(tab.group);
            if let Some(group) = group
                && previous != Some(group.id)
            {
                entries.push(Entry::Chip(group));
            }
            previous = group.map(|group| group.id);
            if group.is_none_or(|group| !group.collapsed) || index == self.active {
                entries.push(Entry::Tab(index, tab));
            }
        }
        entries
    }

    fn chip_width(&self, ui: &Ui, group: &'a GroupInfo<'a>) -> f32 {
        let galley = ui.painter().layout_no_wrap(
            self.chip_label(group).to_owned(),
            sans_medium(size::SMALL),
            Color32::WHITE,
        );
        (galley.size().x + space::S * 2.0).min(CHIP_MAX_WIDTH)
    }

    fn chip_label(&self, group: &'a GroupInfo<'a>) -> &'a str {
        if group.name.trim().is_empty() {
            self.strings.group_unnamed
        } else {
            group.name
        }
    }
}

pub(crate) fn top(ui: &mut Ui, strip: Strip<'_>, rename: &mut (u32, String), output: &mut Output) {
    let theme = strip.theme;
    Panel::top("lumina-tabs-top")
        .exact_size(STRIP_HEIGHT)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(theme.palette.sunken)
                .inner_margin(Margin::symmetric(6, 0)),
        )
        .show(ui, |ui| {
            ui.with_layout(Layout::left_to_right(Align::Max), |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(TAB_GAP, 0.0);
                let entries = strip.entries();

                // Labels and pinned tabs take what they need; the other tabs
                // share what is left.
                let mut fixed = SMALL_BUTTON + space::S;
                let mut sharing = 0.0_f32;
                for entry in &entries {
                    fixed += TAB_GAP;
                    match entry {
                        Entry::Chip(group) => fixed += strip.chip_width(ui, group),
                        Entry::Tab(_, tab) if tab.pinned => fixed += PINNED_WIDTH,
                        Entry::Tab(..) => sharing += 1.0,
                    }
                }
                let width = ((ui.available_width() - fixed) / sharing.max(1.0))
                    .clamp(TAB_MIN_WIDTH, TAB_MAX_WIDTH);

                let mut slots = Vec::new();
                let mut dragged = None;
                for entry in &entries {
                    match entry {
                        Entry::Chip(group) => {
                            let size = Vec2::new(strip.chip_width(ui, group), TAB_HEIGHT);
                            chip(ui, &strip, group, size, rename, output);
                        }
                        Entry::Tab(index, tab) => {
                            let width = if tab.pinned { PINNED_WIDTH } else { width };
                            let item = Item {
                                strip: &strip,
                                tab,
                                index: *index,
                                size: Vec2::new(width, TAB_HEIGHT),
                                run: Run::Across,
                            };
                            let (rect, pointer) = item.show(ui, output);
                            slots.push((*index, rect.left(), rect.right()));
                            if let Some(pointer) = pointer {
                                dragged = Some((*index, pointer.x));
                            }
                        }
                    }
                }
                reorder(&slots, dragged, output);

                ui.add_space(space::XS);
                if plus_button(ui, theme, strip.strings.new_tab).clicked() {
                    output.actions.push(Action::NewTab);
                }
            });
        });
}

pub(crate) fn side(ui: &mut Ui, strip: Strip<'_>, rename: &mut (u32, String), output: &mut Output) {
    let theme = strip.theme;
    Panel::left("lumina-tabs-side")
        .exact_size(COLUMN_WIDTH)
        .show_separator_line(false)
        .frame(
            Frame::new()
                .fill(theme.palette.sunken)
                .inner_margin(Margin::symmetric(6, 6)),
        )
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, TAB_GAP);
            ui.horizontal(|ui| {
                if plus_button(ui, theme, strip.strings.new_tab).clicked() {
                    output.actions.push(Action::NewTab);
                }
                ui.add_space(space::XS);
                ui.label(
                    egui::RichText::new(strip.strings.new_tab)
                        .font(sans(size::SMALL))
                        .color(theme.palette.ink_muted),
                );
            });
            ui.add_space(space::XS);
            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let width = ui.available_width();
                    let mut slots = Vec::new();
                    let mut dragged = None;
                    for entry in strip.entries() {
                        match entry {
                            Entry::Chip(group) => {
                                ui.add_space(space::XS);
                                let size = Vec2::new(strip.chip_width(ui, group), CHIP_HEIGHT);
                                chip(ui, &strip, group, size, rename, output);
                            }
                            Entry::Tab(index, tab) => {
                                let item = Item {
                                    strip: &strip,
                                    tab,
                                    index,
                                    size: Vec2::new(width, ROW_HEIGHT),
                                    run: Run::Down,
                                };
                                let (rect, pointer) = item.show(ui, output);
                                slots.push((index, rect.top(), rect.bottom()));
                                if let Some(pointer) = pointer {
                                    dragged = Some((index, pointer.y));
                                }
                            }
                        }
                    }
                    reorder(&slots, dragged, output);
                });
        });
}

/// Turns a drag into a move: the dragged tab goes to the place of whichever
/// tab the pointer is over. `slots` are each tab's position and extent along
/// the strip.
fn reorder(slots: &[(usize, f32, f32)], dragged: Option<(usize, f32)>, output: &mut Output) {
    let Some((from, pointer)) = dragged else {
        return;
    };
    let target = slots
        .iter()
        .find(|(_, start, end)| pointer >= *start && pointer < *end)
        .map(|(index, ..)| *index);
    if let Some(to) = target
        && to != from
    {
        output.actions.push(Action::MoveTab { from, to });
    }
}

struct Item<'a> {
    strip: &'a Strip<'a>,
    tab: &'a TabInfo<'a>,
    index: usize,
    size: Vec2,
    run: Run,
}

impl Item<'_> {
    fn active(&self) -> bool {
        self.index == self.strip.active
    }

    /// A pinned tab in the top strip shows only its icon.
    fn compact(&self) -> bool {
        self.tab.pinned && self.run == Run::Across
    }

    /// Draws the tab. Returns where it was drawn and, while it is being
    /// dragged, where the pointer is.
    fn show(&self, ui: &mut Ui, output: &mut Output) -> (Rect, Option<Pos2>) {
        let strings = self.strip.strings;
        let theme = self.strip.theme;
        let title = self.tab.title.unwrap_or(strings.new_tab);

        // The id follows the tab rather than its position, so a drag carries
        // on while the tab changes places.
        let id = Id::new(("lumina-tab", self.tab.id));
        let (_, rect) = ui.allocate_space(self.size);
        let response = ui.interact(rect, id, Sense::click_and_drag());
        response
            .widget_info(|| WidgetInfo::selected(WidgetType::Button, true, self.active(), title));

        let close_rect = Rect::from_center_size(
            Pos2::new(
                rect.right() - SMALL_BUTTON / 2.0 - space::XS,
                rect.center().y,
            ),
            Vec2::splat(SMALL_BUTTON - 4.0),
        );
        // A pinned tab is kept on purpose, so it has no close button.
        let show_close = !self.tab.pinned && (self.active() || ui.rect_contains_pointer(rect));
        let close = show_close
            .then(|| ui.interact(close_rect, id.with("close"), Sense::click()))
            .inspect(|close| {
                close.widget_info(|| {
                    WidgetInfo::labeled(WidgetType::Button, true, strings.close_tab)
                });
            });

        if ui.is_rect_visible(rect) {
            self.paint(ui, rect, &response, title);
            if let Some(close) = &close {
                paint_close(ui, theme, close_rect, close);
            }
        }

        if close.as_ref().is_some_and(Response::clicked) || response.middle_clicked() {
            output.actions.push(Action::CloseTab(self.tab.id));
        } else if response.clicked() || response.drag_started() {
            output.actions.push(Action::SelectTab(self.tab.id));
        }

        let pointer = response
            .dragged()
            .then(|| response.interact_pointer_pos())
            .flatten();
        if pointer.is_some() {
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
        }

        response.context_menu(|ui| self.menu(ui, output));
        response.on_hover_text(title);
        (rect, pointer)
    }

    fn menu(&self, ui: &mut Ui, output: &mut Output) {
        let strings = self.strip.strings;
        let theme = self.strip.theme;
        let id = self.tab.id;
        ui.set_min_width(190.0);

        let pin = if self.tab.pinned {
            strings.unpin_tab
        } else {
            strings.pin_tab
        };
        if menu_item(ui, theme, pin, None) {
            output.actions.push(Action::TogglePin(id));
        }

        if !self.tab.pinned {
            if menu_item(ui, theme, strings.add_to_new_group, None) {
                output.actions.push(Action::NewGroup(id));
            }
            for group in self.strip.groups {
                if self.tab.group != Some(group.id) {
                    let swatch = accent_color(group.color, theme.dark);
                    let label =
                        format!("{} {}", strings.add_to_group, self.strip.chip_label(group));
                    if menu_item(ui, theme, &label, Some(swatch)) {
                        output.actions.push(Action::AddToGroup {
                            tab: id,
                            group: group.id,
                        });
                    }
                }
            }
            if self.tab.group.is_some() && menu_item(ui, theme, strings.remove_from_group, None) {
                output.actions.push(Action::Ungroup(id));
            }
        }

        if self.strip.split {
            if menu_item(ui, theme, strings.close_split, None) {
                output.actions.push(Action::CloseSplit);
            }
        } else if !self.active() && menu_item(ui, theme, strings.split_with, None) {
            output.actions.push(Action::SplitWith(id));
        }

        if menu_item(ui, theme, strings.close_tab, None) {
            output.actions.push(Action::CloseTab(id));
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, response: &Response, title: &str) {
        let theme = self.strip.theme;
        let p = theme.palette;
        let painter = ui.painter();
        let radius = match self.run {
            Run::Across => CornerRadius {
                nw: RADIUS,
                ne: RADIUS,
                sw: 0,
                se: 0,
            },
            Run::Down => CornerRadius::same(RADIUS),
        };

        if self.active() {
            painter.rect_filled(rect, radius, p.raised);
            // The line of accent that marks the tab in front.
            let (from, to) = match self.run {
                Run::Across => (
                    Pos2::new(rect.left() + 1.0, rect.top() + FOCUS_WIDTH / 2.0),
                    Pos2::new(rect.right() - 1.0, rect.top() + FOCUS_WIDTH / 2.0),
                ),
                Run::Down => (
                    Pos2::new(rect.left() + FOCUS_WIDTH / 2.0, rect.top() + 5.0),
                    Pos2::new(rect.left() + FOCUS_WIDTH / 2.0, rect.bottom() - 5.0),
                ),
            };
            painter.line_segment([from, to], Stroke::new(FOCUS_WIDTH, p.accent));
        } else if response.hovered() {
            painter.rect_filled(rect, radius, p.rule);
        }

        // A grouped tab carries its group's colour along one edge.
        if let Some(group) = self.strip.group(self.tab.group) {
            let color = accent_color(group.color, theme.dark);
            let (from, to) = match self.run {
                Run::Across => (
                    Pos2::new(rect.left(), rect.bottom() - 1.0),
                    Pos2::new(rect.right(), rect.bottom() - 1.0),
                ),
                Run::Down => (
                    Pos2::new(rect.right() - 1.0, rect.top() + 4.0),
                    Pos2::new(rect.right() - 1.0, rect.bottom() - 4.0),
                ),
            };
            painter.line_segment([from, to], Stroke::new(FOCUS_WIDTH, color));
        }
        focus_ring(ui, theme, response, rect);

        // The site's icon, or a small mark standing in for it.
        let icon_x = if self.compact() {
            rect.center().x
        } else {
            rect.left() + space::S + FAVICON / 2.0
        };
        let icon = Rect::from_center_size(Pos2::new(icon_x, rect.center().y), Vec2::splat(FAVICON));
        if self.tab.loading {
            painter.circle_filled(icon.center(), 3.5, p.accent);
        } else if let Some(texture) = self.tab.favicon {
            painter.image(
                texture,
                icon,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            painter.circle_stroke(icon.center(), 4.0, Stroke::new(1.2, p.ink_muted));
        }
        if self.compact() {
            return;
        }

        let text_left = icon.right() + space::S;
        let text_width = rect.right() - SMALL_BUTTON - space::S - text_left;
        if text_width > 12.0 {
            let (font, color) = if self.active() {
                (sans_medium(size::SMALL), p.ink)
            } else {
                (sans(size::SMALL), p.ink_muted)
            };
            let mut job = LayoutJob::single_section(
                title.to_owned(),
                TextFormat {
                    font_id: font,
                    color,
                    ..TextFormat::default()
                },
            );
            job.wrap = TextWrapping::truncate_at_width(text_width);
            let galley = painter.layout_job(job);
            painter.galley(
                Pos2::new(text_left, rect.center().y - galley.size().y / 2.0),
                galley,
                color,
            );
        }

        // In the column a pinned tab is marked where the close button would be.
        if self.tab.pinned {
            let c = Pos2::new(
                rect.right() - SMALL_BUTTON / 2.0 - space::XS,
                rect.center().y,
            );
            painter.circle_filled(c, 2.5, p.ink_muted);
        }
    }
}

/// A group's label. Clicking it folds or unfolds the group.
fn chip<'a>(
    ui: &mut Ui,
    strip: &Strip<'a>,
    group: &'a GroupInfo<'a>,
    size: Vec2,
    rename: &mut (u32, String),
    output: &mut Output,
) {
    let theme = strip.theme;
    let p = theme.palette;
    let label = strip.chip_label(group);
    let id = Id::new(("lumina-tab-group", group.id));
    let (_, rect) = ui.allocate_space(size);
    let response = ui.interact(rect, id, Sense::click());
    response
        .widget_info(|| WidgetInfo::selected(WidgetType::Button, true, !group.collapsed, label));

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let body = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), CHIP_HEIGHT));
        let color = accent_color(group.color, theme.dark);
        if group.collapsed {
            // A folded group is drawn in outline, as if its tabs were put away.
            painter.rect_stroke(
                body,
                CornerRadius::same(RADIUS),
                Stroke::new(FOCUS_WIDTH, color),
                egui::StrokeKind::Inside,
            );
        } else {
            painter.rect_filled(body, CornerRadius::same(RADIUS), color);
        }
        focus_ring(ui, theme, &response, body);
        let text_color = if group.collapsed { p.ink } else { p.on_accent };
        let mut job = LayoutJob::single_section(
            label.to_owned(),
            TextFormat {
                font_id: sans_medium(size::SMALL),
                color: text_color,
                ..TextFormat::default()
            },
        );
        job.wrap = TextWrapping::truncate_at_width(body.width() - space::S * 1.5);
        let galley = painter.layout_job(job);
        painter.galley(
            Pos2::new(
                body.center().x - galley.size().x / 2.0,
                body.center().y - galley.size().y / 2.0,
            ),
            galley,
            text_color,
        );
    }

    if response.clicked() {
        output.actions.push(Action::ToggleGroup(group.id));
    }

    response.context_menu(|ui| {
        ui.set_min_width(190.0);
        if rename.0 != group.id {
            *rename = (group.id, group.name.to_owned());
        }
        let field = TextEdit::singleline(&mut rename.1)
            .font(sans(size::BODY))
            .text_color(p.ink)
            .hint_text(strip.strings.group_name_hint)
            .desired_width(170.0)
            .frame(
                Frame::new()
                    .fill(p.field)
                    .stroke(Stroke::new(HAIRLINE, p.rule))
                    .corner_radius(RADIUS)
                    .inner_margin(Margin::symmetric(8, 5)),
            )
            .show(ui);
        if field.response.changed() {
            output
                .actions
                .push(Action::RenameGroup(group.id, rename.1.clone()));
        }
        ui.add_space(space::XS);
        ui.horizontal(|ui| {
            for color in Accent::ALL {
                let (dot, response) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                let fill = accent_color(color, theme.dark);
                ui.painter().circle_filled(dot.center(), 8.0, fill);
                if color == group.color {
                    ui.painter()
                        .circle_stroke(dot.center(), 11.0, Stroke::new(1.5, p.ink));
                }
                focus_ring(ui, theme, &response, dot);
                if response.clicked() {
                    output.actions.push(Action::RecolorGroup(group.id, color));
                }
            }
        });
        if menu_item(ui, theme, strip.strings.dissolve_group, None) {
            output.actions.push(Action::DissolveGroup(group.id));
        }
    });
    response.on_hover_text(label);
}

/// One line in a menu. Returns whether it was chosen, and closes the menu if
/// so. `swatch` puts a dot of colour before the label.
fn menu_item(ui: &mut Ui, theme: &Theme, label: &str, swatch: Option<Color32>) -> bool {
    let p = theme.palette;
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() || response.has_focus() {
            painter.rect_filled(rect, CornerRadius::same(RADIUS), p.rule);
        }
        let mut x = rect.left() + space::S;
        if let Some(color) = swatch {
            painter.circle_filled(Pos2::new(x + 5.0, rect.center().y), 5.0, color);
            x += 16.0;
        }
        painter.text(
            Pos2::new(x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            sans(size::BODY),
            p.ink,
        );
    }
    let chosen = response.clicked();
    if chosen {
        ui.close();
    }
    chosen
}

fn paint_close(ui: &Ui, theme: &Theme, rect: Rect, response: &Response) {
    let p = theme.palette;
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, CornerRadius::same(RADIUS), p.rule);
    }
    focus_ring(ui, theme, response, rect);
    let color = if response.hovered() {
        p.ink
    } else {
        p.ink_muted
    };
    let stroke = Stroke::new(1.4, color);
    let (c, r) = (rect.center(), 3.6);
    painter.line_segment(
        [Pos2::new(c.x - r, c.y - r), Pos2::new(c.x + r, c.y + r)],
        stroke,
    );
    painter.line_segment(
        [Pos2::new(c.x - r, c.y + r), Pos2::new(c.x + r, c.y - r)],
        stroke,
    );
}

/// The small "+" that opens a tab.
fn plus_button(ui: &mut Ui, theme: &Theme, label: &str) -> Response {
    let p = theme.palette;
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(SMALL_BUTTON + 4.0), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() || response.has_focus() {
            painter.rect_filled(rect, CornerRadius::same(RADIUS), p.rule);
        }
        focus_ring(ui, theme, &response, rect);
        let stroke = Stroke::new(1.5, p.ink);
        let (c, r) = (rect.center(), 5.0);
        painter.line_segment([Pos2::new(c.x - r, c.y), Pos2::new(c.x + r, c.y)], stroke);
        painter.line_segment([Pos2::new(c.x, c.y - r), Pos2::new(c.x, c.y + r)], stroke);
    }
    response
        .on_hover_cursor(CursorIcon::PointingHand)
        .on_hover_text(label)
}
