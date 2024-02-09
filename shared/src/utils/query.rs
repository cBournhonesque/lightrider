use bevy::ecs::query::WorldQuery;
use bevy::prelude::Without;
use lightyear::prelude::client::{Confirmed, Interpolated};

#[derive(WorldQuery)]
pub struct Controlled {
    filter: (Without<Confirmed>, Without<Interpolated>),
}