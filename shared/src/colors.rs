#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnakePaletteColor {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
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
    pub const fn new(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }
}

pub fn snake_color_for_name(name: &str) -> SnakePaletteColor {
    let index = stable_name_hash(name) as usize % SNAKE_PALETTE.len();
    SNAKE_PALETTE[index]
}

pub fn snake_color_for_fallback(value: u64) -> SnakePaletteColor {
    SNAKE_PALETTE[value as usize % SNAKE_PALETTE.len()]
}

pub fn death_food_tone_for_name(name: &str, index: usize) -> SnakePaletteColor {
    let base = snake_color_for_name(name);
    let intensity = [0.94, 1.0, 1.08, 0.9, 1.04][index % 5];
    let lift = [0.04, 0.08, 0.0, 0.12][index % 4];
    SnakePaletteColor::new(
        (base.red * intensity + lift).clamp(0.0, 1.0),
        (base.green * intensity + lift).clamp(0.0, 1.0),
        (base.blue * intensity + lift).clamp(0.0, 1.0),
    )
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

    #[test]
    fn death_food_tones_stay_near_line_color() {
        let base = snake_color_for_name("Alice");
        let tone = death_food_tone_for_name("Alice", 3);

        assert!((tone.red - base.red).abs() <= 0.16);
        assert!((tone.green - base.green).abs() <= 0.16);
        assert!((tone.blue - base.blue).abs() <= 0.16);
    }
}
