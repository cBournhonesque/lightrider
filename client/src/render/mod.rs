use bevy::app::{App, Plugin};

mod camera;
mod food;
pub(crate) mod snake;

pub(crate) struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(snake::SnakeRenderPlugin);
        app.add_plugins(camera::CameraPlugin);
        app.add_plugins(food::FoodRenderPlugin);
    }
}
