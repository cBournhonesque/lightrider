use bevy::ecs::query::{ReadOnlyWorldQuery, WorldQuery};
use bevy::prelude::Without;
use lightyear::prelude::client::{Confirmed, Interpolated};

#[derive(WorldQuery)]
pub struct Controlled {
    filter: (Without<Confirmed>, Without<Interpolated>),
}