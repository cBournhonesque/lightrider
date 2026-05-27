use bevy::app::App;
use bevy::prelude::Plugin;

use protocol::prelude::*;

pub mod bundle;
pub mod config;
pub mod protocol;

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(protocol::ProtocolPlugin);

        // events
        app.add_message::<SnakeCollision>();
        // registry
        app.register_type::<TailLength>()
            .register_type::<TailPoints>()
            .register_type::<Speed>()
            .register_type::<Acceleration>()
            .register_type::<FoodBoost>()
            .register_type::<HasPlayer>()
            .register_type::<Player>()
            .register_type::<PlayerScore>()
            .register_type::<PlayerRank>()
            .register_type::<PlayerStatus>()
            .register_type::<Position>();
    }
}
