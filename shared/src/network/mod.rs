use bevy::app::App;
use bevy::prelude::Plugin;

use protocol::prelude::*;

pub mod protocol;
pub mod config;


pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        // events
        app.add_event::<SnakeCollision>();
        // registry
        app.register_type::<TailLength>()
            .register_type::<TailPoints>()
            .register_type::<Speed>()
            .register_type::<Acceleration>()
            .register_type::<HasPlayer>()
            .register_type::<Player>();
    }
}
