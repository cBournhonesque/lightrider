use bevy::prelude::*;

use shared::network::protocol::prelude::*;

mod arena;
mod assets;
mod camera;
mod colors;
mod effects;
mod food;
mod hud;
mod leader;
mod name_labels;
pub(crate) mod snake;

pub(crate) struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<assets::PowerlineSpriteSheet>();
        app.add_plugins(arena::ArenaRenderPlugin);
        app.add_plugins(snake::SnakeRenderPlugin);
        app.add_plugins(camera::CameraPlugin);
        app.add_plugins(effects::EffectsRenderPlugin);
        app.add_plugins(food::FoodRenderPlugin);
        app.add_plugins(hud::HudRenderPlugin);
        app.add_plugins(leader::LeaderRenderPlugin);
        app.add_plugins(name_labels::NameLabelRenderPlugin);
        app.add_systems(Update, log_first_rendered_entities);
    }
}

fn log_first_rendered_entities(
    mut logged: Local<bool>,
    tails: Query<(), With<TailPoints>>,
    food: Query<(), With<FoodMarker>>,
) {
    if *logged {
        return;
    }

    let snake_count = tails.iter().count();
    let food_count = food.iter().count();
    if snake_count == 0 && food_count == 0 {
        return;
    }

    info!(
        snake_count,
        food_count, "Client render received gameplay entities"
    );
    *logged = true;
}
