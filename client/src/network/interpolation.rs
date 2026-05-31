use bevy::prelude::*;
use lightyear::frame_interpolation::{FrameInterpolate, FrameInterpolationPlugin};
use lightyear::prelude::*;

use shared::network::protocol::prelude::*;

pub struct InterpolationPlugin;

impl Plugin for InterpolationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameInterpolationPlugin::<TailPoints>::default());
        app.add_plugins(FrameInterpolationPlugin::<TailLength>::default());
        app.add_systems(
            PostUpdate,
            interpolate_snakes.in_set(InterpolationSystems::Interpolate),
        );
        app.add_systems(Update, add_frame_interpolation_to_predicted_snakes);
    }
}

fn add_frame_interpolation_to_predicted_snakes(
    mut commands: Commands,
    tails: Query<
        Entity,
        (
            With<Predicted>,
            With<TailPoints>,
            Without<FrameInterpolate<TailPoints>>,
        ),
    >,
    lengths: Query<
        Entity,
        (
            With<Predicted>,
            With<TailLength>,
            Without<FrameInterpolate<TailLength>>,
        ),
    >,
) {
    for snake in &tails {
        commands
            .entity(snake)
            .insert(FrameInterpolate::<TailPoints>::default());
    }
    for snake in &lengths {
        commands
            .entity(snake)
            .insert(FrameInterpolate::<TailLength>::default());
    }
}

fn interpolate_snakes(
    timeline: Single<&InterpolationTimeline, With<IsSynced<InterpolationTimeline>>>,
    mut snakes: Query<
        (
            &mut TailPoints,
            &mut TailLength,
            &ConfirmedHistory<TailPoints>,
            &ConfirmedHistory<TailLength>,
        ),
        With<Interpolated>,
    >,
) {
    let interpolation_tick = timeline.tick();
    let interpolation_overstep = timeline.overstep().to_f32();

    for (mut tail, mut length, tail_history, length_history) in &mut snakes {
        let Some((start_tick, tail_start)) = tail_history.start() else {
            continue;
        };
        if interpolation_tick < start_tick {
            continue;
        }
        let Some((end_tick, tail_end)) = tail_history.end() else {
            *tail = tail_start.clone();
            continue;
        };

        let length_start = length_history
            .start()
            .map(|(_, length)| length)
            .unwrap_or(&length);
        let length_end = length_history
            .end()
            .map(|(_, length)| length)
            .unwrap_or(length_start);
        let t = interpolation_fraction(
            start_tick,
            end_tick,
            interpolation_tick,
            interpolation_overstep,
        );

        let (interpolated_tail, interpolated_length) =
            interpolate_tail_points_with_length(tail_start, tail_end, length_start, length_end, t);
        *tail = interpolated_tail;
        *length = interpolated_length;
    }
}

fn interpolation_fraction(start: Tick, end: Tick, current: Tick, overstep: f32) -> f32 {
    if start == end {
        return 1.0;
    }
    (((current - start) as f32 + overstep) / (end - start) as f32).clamp(0.0, 1.0)
}
