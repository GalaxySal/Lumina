//! Lumina's own controls. Each is drawn from theme tokens; none uses egui's
//! stock appearance.

use std::f32::consts::TAU;

use egui::{
    Align2, Color32, CornerRadius, CursorIcon, FontId, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, WidgetInfo, WidgetType,
};
use lumina_theme::{FOCUS_WIDTH, HAIRLINE, RADIUS, Theme, sans, sans_medium, size, space};

/// The pictograms on the toolbar. They are drawn as strokes rather than taken
/// from an icon font, so they share the weight of the text beside them.
#[derive(Clone, Copy)]
pub enum Icon {
    Back,
    Forward,
    Reload,
    /// Three sliders: adjustments, rather than the usual cog.
    Settings,
}

const ICON_STROKE: f32 = 1.6;
const ICON_BUTTON: f32 = 32.0;

/// A square toolbar button showing `icon`. `label` names it for screen
/// readers and is shown as a tooltip.
pub fn icon_button(ui: &mut Ui, theme: &Theme, icon: Icon, label: &str, enabled: bool) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(ICON_BUTTON), sense);
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));

    if ui.is_rect_visible(rect) {
        let p = theme.palette;
        let painter = ui.painter();
        if enabled && (response.hovered() || response.has_focus()) {
            painter.rect_filled(rect, CornerRadius::same(RADIUS), p.rule);
        }
        focus_ring(ui, theme, &response, rect);
        let color = if enabled { p.ink } else { p.rule };
        draw_icon(ui, icon, rect.center(), Stroke::new(ICON_STROKE, color));
    }

    let response = if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };
    response.on_hover_text(label)
}

fn draw_icon(ui: &Ui, icon: Icon, center: Pos2, stroke: Stroke) {
    let painter = ui.painter();
    let at = |x: f32, y: f32| Pos2::new(center.x + x, center.y + y);
    match icon {
        Icon::Back | Icon::Forward => {
            let direction = if matches!(icon, Icon::Back) {
                1.0
            } else {
                -1.0
            };
            let tip = at(-6.5 * direction, 0.0);
            painter.line_segment([tip, at(6.5 * direction, 0.0)], stroke);
            painter.add(Shape::line(
                vec![at(-1.5 * direction, -5.0), tip, at(-1.5 * direction, 5.0)],
                stroke,
            ));
        }
        Icon::Reload => {
            // Three quarters of a circle, opening at the top right, with an
            // arrowhead where the stroke ends.
            let radius = 6.0;
            let start = -0.06 * TAU;
            let sweep = 0.78 * TAU;
            let steps: u8 = 24;
            let points = (0..=steps)
                .map(|step| {
                    let angle = start + sweep * f32::from(step) / f32::from(steps);
                    at(radius * angle.cos(), radius * angle.sin())
                })
                .collect();
            painter.add(Shape::line(points, stroke));
            let head = at(radius * start.cos(), radius * start.sin());
            painter.add(Shape::line(
                vec![
                    Pos2::new(head.x - 4.2, head.y - 0.6),
                    head,
                    Pos2::new(head.x + 0.8, head.y - 4.4),
                ],
                stroke,
            ));
        }
        Icon::Settings => {
            for (y, knob) in [(-5.0, 2.5), (0.0, -3.0), (5.0, 1.0)] {
                painter.line_segment([at(-7.0, y), at(7.0, y)], stroke);
                painter.circle_filled(at(knob, y), 2.4, stroke.color);
            }
        }
    }
}

/// Outlines `rect` in the accent while `response` has keyboard focus, so the
/// focused control can always be found.
pub fn focus_ring(ui: &Ui, theme: &Theme, response: &Response, rect: Rect) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(RADIUS),
            Stroke::new(FOCUS_WIDTH, theme.palette.accent),
            StrokeKind::Outside,
        );
    }
}

/// One option in a list where exactly one is chosen. The whole row is the
/// target. `note` is shown after the label in the muted colour.
pub fn choice(
    ui: &mut Ui,
    theme: &Theme,
    selected: bool,
    label: &str,
    note: Option<(&str, FontId)>,
) -> Response {
    let p = theme.palette;
    let label_font = if selected {
        sans_medium(size::BODY)
    } else {
        sans(size::BODY)
    };
    let label_galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), label_font, p.ink);
    let note_galley = note.map(|(text, font)| {
        ui.painter()
            .layout_no_wrap(text.to_owned(), font, p.ink_muted)
    });

    let marker = 18.0;
    let note_width = note_galley
        .as_ref()
        .map_or(0.0, |galley| galley.size().x + space::M);
    let width = marker + space::S + label_galley.size().x + note_width + space::S;
    let height = label_galley.size().y.max(marker) + space::S;
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, label));

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        focus_ring(ui, theme, &response, rect);

        // The marker is a small square, like a ballot box, inked when chosen.
        let box_rect = Rect::from_center_size(
            Pos2::new(
                rect.left() + marker / 2.0 + space::XS / 2.0,
                rect.center().y,
            ),
            Vec2::splat(12.0),
        );
        let outline = if response.hovered() || selected {
            p.ink
        } else {
            p.ink_muted
        };
        painter.rect_stroke(
            box_rect,
            CornerRadius::same(2),
            Stroke::new(HAIRLINE * 1.5, outline),
            StrokeKind::Inside,
        );
        if selected {
            painter.rect_filled(box_rect.shrink(3.0), CornerRadius::same(1), p.accent);
        }

        let text_x = rect.left() + marker + space::S;
        let label_width = label_galley.size().x;
        let label_y = rect.center().y - label_galley.size().y / 2.0;
        painter.galley(Pos2::new(text_x, label_y), label_galley, p.ink);
        if let Some(note_galley) = note_galley {
            let note_y = rect.center().y - note_galley.size().y / 2.0;
            painter.galley(
                Pos2::new(text_x + label_width + space::M, note_y),
                note_galley,
                p.ink_muted,
            );
        }
    }

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A colour chip with its name: one option among the accent colours.
pub fn swatch(ui: &mut Ui, theme: &Theme, color: Color32, selected: bool, label: &str) -> Response {
    let p = theme.palette;
    let font = if selected {
        sans_medium(size::BODY)
    } else {
        sans(size::BODY)
    };
    let galley = ui.painter().layout_no_wrap(label.to_owned(), font, p.ink);
    let chip = Vec2::new(34.0, 22.0);
    let size = Vec2::new(
        chip.x + space::S + galley.size().x + space::M,
        chip.y + space::S * 1.5,
    );
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::selected(WidgetType::RadioButton, true, selected, label));

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        focus_ring(ui, theme, &response, rect);
        let chip_rect = Rect::from_min_size(
            Pos2::new(rect.left() + space::XS, rect.center().y - chip.y / 2.0),
            chip,
        );
        painter.rect_filled(chip_rect, CornerRadius::same(RADIUS), color);
        if selected {
            // An ink underline, as if marked with a pen.
            let y = chip_rect.bottom() + 4.0;
            painter.line_segment(
                [
                    Pos2::new(chip_rect.left(), y),
                    Pos2::new(chip_rect.right(), y),
                ],
                Stroke::new(FOCUS_WIDTH, p.ink),
            );
        }
        painter.galley(
            Pos2::new(
                chip_rect.right() + space::S,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            p.ink,
        );
    }

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A text button. `primary` fills it with the accent: use it for the one
/// action a screen exists for.
pub fn button(ui: &mut Ui, theme: &Theme, label: &str, primary: bool) -> Response {
    let p = theme.palette;
    let text_color = if primary { p.on_accent } else { p.ink };
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), sans_medium(size::BODY), text_color);
    let padding = if primary {
        Vec2::new(space::L, space::S + 2.0)
    } else {
        Vec2::new(space::M, space::S)
    };
    let outer = Vec2::new(
        galley.size().x + padding.x * 2.0,
        galley.size().y + padding.y * 2.0,
    );
    let (rect, response) = ui.allocate_exact_size(outer, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));

    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = CornerRadius::same(RADIUS);
        if primary {
            let fill = if response.is_pointer_button_down_on() {
                p.ink
            } else {
                p.accent
            };
            painter.rect_filled(rect, radius, fill);
        } else {
            let outline = if response.hovered() { p.ink } else { p.rule };
            painter.rect_stroke(
                rect,
                radius,
                Stroke::new(HAIRLINE, outline),
                StrokeKind::Inside,
            );
        }
        focus_ring(ui, theme, &response, rect.expand(2.0));
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            sans_medium(size::BODY),
            text_color,
        );
    }

    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// A full-width hairline.
pub fn rule(ui: &mut Ui, theme: &Theme) {
    let (rect, _) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), HAIRLINE), Sense::hover());
    ui.painter().line_segment(
        [rect.left_center(), rect.right_center()],
        Stroke::new(HAIRLINE, theme.palette.rule),
    );
}
