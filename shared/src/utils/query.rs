use bevy::ecs::query::{Or, QueryFilter};
use bevy::prelude::{With, Without};
use lightyear::prelude::{Interpolated, Predicted, Replicated};

#[derive(QueryFilter)]
pub struct Simulated {
    filter: (
        Without<Interpolated>,
        Or<(With<Predicted>, Without<Replicated>)>,
    ),
}
