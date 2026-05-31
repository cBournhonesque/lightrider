use bevy::prelude::*;

const SHEET_PATH: &str = "powerline/sheet.png";

#[derive(Resource, Clone)]
pub(crate) struct PowerlineSpriteSheet {
    image: Handle<Image>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PowerlineFrame {
    Crown,
    Food,
    Grid,
    HeadDot,
    Lightning1,
    Lightning2,
    Lightning3,
    Spark0,
    Spark1,
    Spark2,
    WallStretch,
}

impl FromWorld for PowerlineSpriteSheet {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        Self {
            image: asset_server.load(SHEET_PATH),
        }
    }
}

impl PowerlineSpriteSheet {
    pub(crate) fn image(&self) -> Handle<Image> {
        self.image.clone()
    }

    pub(crate) fn sprite(&self, frame: PowerlineFrame, custom_size: Vec2, color: Color) -> Sprite {
        Sprite {
            image: self.image.clone(),
            rect: Some(frame.rect()),
            custom_size: Some(custom_size),
            color,
            ..default()
        }
    }
}

impl PowerlineFrame {
    pub(crate) fn rect(self) -> Rect {
        let (x, y, width, height) = match self {
            Self::Crown => (382.0, 102.0, 31.0, 27.0),
            Self::Food => (132.0, 2.0, 100.0, 100.0),
            Self::Grid => (2.0, 2.0, 128.0, 128.0),
            Self::HeadDot => (132.0, 104.0, 47.0, 46.0),
            Self::Lightning1 => (415.0, 102.0, 18.0, 30.0),
            Self::Lightning2 => (483.0, 99.0, 19.0, 30.0),
            Self::Lightning3 => (461.0, 99.0, 20.0, 30.0),
            Self::Spark0 => (434.0, 68.0, 49.0, 29.0),
            Self::Spark1 => (300.0, 102.0, 41.0, 29.0),
            Self::Spark2 => (343.0, 102.0, 37.0, 30.0),
            Self::WallStretch => (2.0, 132.0, 40.0, 1.0),
        };
        Rect::new(x, y, x + width, y + height)
    }
}
