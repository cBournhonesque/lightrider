use bevy::prelude::*;
use shared::colors;
use shared::network::protocol::prelude::Player;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SnakePaletteColor {
    red: f32,
    green: f32,
    blue: f32,
}

impl SnakePaletteColor {
    fn from_shared(color: colors::SnakePaletteColor) -> Self {
        Self {
            red: color.red,
            green: color.green,
            blue: color.blue,
        }
    }

    pub(crate) fn tail_core(self) -> Color {
        self.linear_color(4.2, 1.0)
    }

    pub(crate) fn head_glow(self, alpha: f32) -> Color {
        self.linear_color(1.6, alpha.clamp(0.0, 1.0))
    }

    pub(crate) fn spark(self) -> Color {
        self.linear_color(2.7, 0.95)
    }

    pub(crate) fn lightning(self) -> Color {
        self.linear_color(2.4, 0.72)
    }

    pub(crate) fn label(self) -> Color {
        self.srgb_color(0.82)
    }

    fn linear_color(self, intensity: f32, alpha: f32) -> Color {
        Color::linear_rgba(
            self.red * intensity,
            self.green * intensity,
            self.blue * intensity,
            alpha,
        )
    }

    fn srgb_color(self, value: f32) -> Color {
        Color::srgb(
            value + self.red * (1.0 - value),
            value + self.green * (1.0 - value),
            value + self.blue * (1.0 - value),
        )
    }
}

pub(crate) fn snake_color_for_player(player: &Player) -> SnakePaletteColor {
    snake_color_for_name(&player.name)
}

pub(crate) fn snake_color_for_name(name: &str) -> SnakePaletteColor {
    SnakePaletteColor::from_shared(colors::snake_color_for_name(name))
}

pub(crate) fn snake_color_for_fallback(value: u64) -> SnakePaletteColor {
    SnakePaletteColor::from_shared(colors::snake_color_for_fallback(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_color_hash_is_case_insensitive() {
        assert_eq!(snake_color_for_name("Alice"), snake_color_for_name("alice"));
    }

    #[test]
    fn snake_color_hash_uses_ten_color_palette() {
        let mut colors = Vec::new();
        for index in 0..32 {
            let color = snake_color_for_name(&format!("Player {index}"));
            if !colors.contains(&color) {
                colors.push(color);
            }
        }

        assert!(colors.len() <= 10);
        assert!(colors.len() > 4);
    }
}
