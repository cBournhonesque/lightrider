use bevy::prelude::*;
use shared::network::protocol::prelude::Player;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SnakePaletteColor {
    red: f32,
    green: f32,
    blue: f32,
}

const SNAKE_PALETTE: [SnakePaletteColor; 10] = [
    SnakePaletteColor::new(0.10, 0.88, 1.00),
    SnakePaletteColor::new(1.00, 0.38, 0.72),
    SnakePaletteColor::new(0.48, 1.00, 0.36),
    SnakePaletteColor::new(1.00, 0.78, 0.18),
    SnakePaletteColor::new(0.64, 0.46, 1.00),
    SnakePaletteColor::new(1.00, 0.48, 0.18),
    SnakePaletteColor::new(0.24, 0.58, 1.00),
    SnakePaletteColor::new(0.98, 0.24, 0.32),
    SnakePaletteColor::new(0.28, 1.00, 0.72),
    SnakePaletteColor::new(0.90, 0.98, 0.24),
];

impl SnakePaletteColor {
    const fn new(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }

    pub(crate) fn tail_core(self) -> Color {
        self.linear_color(4.2, 1.0)
    }

    pub(crate) fn inner_glow(self) -> Color {
        self.linear_color(1.4, 0.15)
    }

    pub(crate) fn outer_glow(self) -> Color {
        self.linear_color(0.75, 0.06)
    }

    pub(crate) fn head(self) -> Color {
        self.linear_color(3.2, 1.0)
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
    let index = stable_name_hash(name) as usize % SNAKE_PALETTE.len();
    SNAKE_PALETTE[index]
}

pub(crate) fn snake_color_for_fallback(value: u64) -> SnakePaletteColor {
    SNAKE_PALETTE[value as usize % SNAKE_PALETTE.len()]
}

fn stable_name_hash(name: &str) -> u64 {
    let trimmed = name.trim();
    let bytes = if trimmed.is_empty() {
        "player".as_bytes()
    } else {
        trimmed.as_bytes()
    };
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes.iter().map(|byte| byte.to_ascii_lowercase()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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
