use bevy::prelude::*;

#[cfg(feature = "render")]
mod camera;
#[cfg(feature = "render")]
pub(crate) mod snake;

pub(crate) struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, _app: &mut App) {
        #[cfg(feature = "render")]
        add_render_debug_plugins(_app);
    }
}

#[cfg(feature = "render")]
fn add_render_debug_plugins(app: &mut App) {
    use bevy::render::RenderPlugin;

    if app.is_plugin_added::<RenderPlugin>() {
        app.add_plugins(snake::SnakeRenderPlugin);
        app.add_plugins(camera::CameraPlugin);
    }
}
