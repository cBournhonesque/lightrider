use bevy::prelude::*;
use bevy_rand::prelude::{EntropyPlugin, WyRand};

pub struct RandPlugin;

pub const SEED: u64 = 56;

impl Plugin for RandPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EntropyPlugin::<WyRand>::with_seed(SEED.to_ne_bytes()));
    }
}
