use bevy::ecs::query::{QueryFilter};
use bevy::prelude::Without;
use lightyear::prelude::client::{Confirmed, Interpolated};

#[derive(QueryFilter)]
pub struct Controlled {
    filter: (Without<Confirmed>, Without<Interpolated>),
}