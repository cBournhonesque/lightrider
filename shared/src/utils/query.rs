use bevy::ecs::query::{Or, QueryFilter};
use bevy::prelude::{Component, With, Without};
use lightyear::prelude::{Interpolated, Predicted, Replicated};

/// Marks authoritative server-side entities that should run the shared fixed-tick simulation.
///
/// Lightyear main can attach replication/interpolation marker components to source entities.
/// Relying only on `Without<Replicated>` or `Without<Interpolated>` would therefore skip
/// authoritative server entities and freeze the server simulation.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct SimulationAuthority;

#[derive(QueryFilter)]
pub struct Simulated {
    filter: Or<(
        With<Predicted>,
        With<SimulationAuthority>,
        (Without<Replicated>, Without<Interpolated>),
    )>,
}
