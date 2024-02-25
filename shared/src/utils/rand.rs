use bevy::prelude::*;
use bevy_turborand::GlobalRng;

pub struct RandPlugin;

pub const SEED: u64 = 56;

impl Plugin for RandPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(GlobalRng::with_seed(SEED));
    }
}