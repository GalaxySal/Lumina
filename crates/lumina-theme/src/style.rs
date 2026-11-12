use egui::style::{Selection, WidgetVisuals};
use egui::{Color32, CornerRadius, Shadow, Stroke, TextStyle, Vec2};

use crate::palette::Theme;
use crate::{FOCUS_WIDTH, HAIRLINE, MOTION, RADIUS, fonts, size, space};

/// Makes every stock widget in `ctx` follow `theme`, so that nothing of
/// egui's own look remains.
pub fn apply(ctx: &egui::Context, theme: &Theme) {
    let p = theme.palette;
    let radius = CornerRadius::same(RADIUS);
    let rule = Stroke::new(HAIRLINE, p.rule);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, fonts::sans(size::SMALL)),
            (TextStyle::Body, fonts::sans(size::BODY)),
            (TextStyle::Button, fonts::sans_medium(size::BODY)),
            (TextStyle::Monospace, fonts::mono(size::MONO)),
            (TextStyle::Heading, fonts::serif(size::HEADING)),
        ]
        .into();
        style.animation_time = MOTION;

        let spacing = &mut style.spacing;
        spacing.item_spacing = Vec2::new(space::S, space::S);
        spacing.button_padding = Vec2::new(space::M, space::S);
        spacing.interact_size = Vec2::new(space::XL, space::XL);
        spacing.scroll.floating = true;

        let visuals = &mut style.visuals;
        visuals.dark_mode = theme.dark;
        visuals.override_text_color = None;
        visuals.panel_fill = p.paper;
        visuals.window_fill = p.raised;
        visuals.window_stroke = rule;
        visuals.window_corner_radius = radius;
        visuals.menu_corner_radius = radius;
        visuals.window_shadow = Shadow::NONE;
        visuals.popup_shadow = Shadow::NONE;
        visuals.extreme_bg_color = p.field;
        visuals.text_edit_bg_color = Some(p.field);
        visuals.faint_bg_color = p.raised;
        visuals.code_bg_color = p.raised;
        visuals.hyperlink_color = p.accent;
        visuals.warn_fg_color = p.danger;
        visuals.error_fg_color = p.danger;
        visuals.selection = Selection {
            bg_fill: p.accent.gamma_multiply(0.28),
            stroke: Stroke::new(FOCUS_WIDTH, p.accent),
        };
        visuals.text_cursor.stroke = Stroke::new(FOCUS_WIDTH, p.accent);

        let widget = |fill: Color32, outline: Stroke, text: Color32| WidgetVisuals {
            bg_fill: fill,
            weak_bg_fill: fill,
            bg_stroke: outline,
            corner_radius: radius,
            fg_stroke: Stroke::new(1.5, text),
            expansion: 0.0,
        };
        visuals.widgets.noninteractive = widget(p.paper, rule, p.ink);
        visuals.widgets.inactive = widget(p.raised, rule, p.ink);
        visuals.widgets.hovered = widget(p.raised, Stroke::new(HAIRLINE, p.ink_muted), p.ink);
        visuals.widgets.active = widget(p.rule, Stroke::new(HAIRLINE, p.ink), p.ink);
        visuals.widgets.open = widget(p.raised, Stroke::new(HAIRLINE, p.ink_muted), p.ink);
    });
}
