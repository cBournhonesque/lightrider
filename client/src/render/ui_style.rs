use bevy::prelude::*;

pub(crate) fn panel_background(alpha: f32) -> BackgroundColor {
    BackgroundColor(Color::srgba(0.01, 0.025, 0.04, alpha.clamp(0.0, 1.0)))
}

pub(crate) fn panel_border() -> BorderColor {
    BorderColor::all(Color::srgba(0.28, 0.86, 1.0, 0.68))
}

pub(crate) fn panel_radius() -> BorderRadius {
    BorderRadius::all(Val::Px(8.0))
}

pub(crate) fn panel_shadow() -> BoxShadow {
    BoxShadow::new(
        Color::srgba(0.12, 0.72, 1.0, 0.24),
        Val::Px(0.0),
        Val::Px(0.0),
        Val::Px(1.0),
        Val::Px(14.0),
    )
}

pub(crate) fn title_color() -> TextColor {
    TextColor(Color::srgb(0.62, 0.94, 1.0))
}

pub(crate) fn body_color() -> TextColor {
    TextColor(Color::srgb(0.78, 0.92, 1.0))
}

pub(crate) fn text_glow() -> TextShadow {
    TextShadow {
        offset: Vec2::ZERO,
        color: Color::srgba(0.16, 0.82, 1.0, 0.7),
    }
}

pub(crate) fn button_background(active: bool, interaction: Interaction) -> Color {
    match (active, interaction) {
        (_, Interaction::Pressed) => Color::srgba(0.13, 0.58, 0.88, 0.68),
        (true, Interaction::Hovered) => Color::srgba(0.08, 0.42, 0.66, 0.66),
        (false, Interaction::Hovered) => Color::srgba(0.04, 0.18, 0.27, 0.62),
        (true, Interaction::None) => Color::srgba(0.05, 0.31, 0.5, 0.6),
        (false, Interaction::None) => Color::srgba(0.01, 0.025, 0.04, 0.48),
    }
}

pub(crate) fn button_border() -> BorderColor {
    BorderColor::all(Color::srgba(0.34, 0.9, 1.0, 0.52))
}
