use crate::inputs::ToggleCamera;
use bevy::camera::{Projection, ScalingMode};
use bevy::prelude::*;
use lightyear::prelude::input::bei::Start;
use lightyear::prelude::Predicted;
use shared::network::protocol::prelude::TailPoints;

pub struct CameraPlugin;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, States)]
pub enum CameraState {
    // follow the player
    #[default]
    Follow,
    // view the full map
    Full,
}

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        // state
        app.init_state::<CameraState>();
        // follow
        app.add_systems(OnEnter(CameraState::Follow), enter_follow_camera);

        // full
        app.add_systems(OnEnter(CameraState::Full), enter_full_camera);

        // we could run during update, because the predicted movement is updated in FixedUpdate
        app.add_systems(
            PostUpdate,
            follow_camera.run_if(in_state(CameraState::Follow)),
        );
        app.add_observer(toggle_camera);
    }
}

fn toggle_camera(
    _trigger: On<Start<ToggleCamera>>,
    mut next_state: ResMut<NextState<CameraState>>,
    current_state: Res<State<CameraState>>,
) {
    match current_state.get() {
        CameraState::Follow => next_state.set(CameraState::Full),
        CameraState::Full => next_state.set(CameraState::Follow),
    }
}

/// TODO: this kill thing is too complicated, set HasFocusHead component on the player
/// entity once and then multiple systems can depend on HasFocusHead (follow camera, sounds, scope, etc.)
/// also server can use HasFocusHead.
///
/// System to make the camera follow the head of the player, or the head of the killer
fn follow_camera(
    predicted: Query<&TailPoints, With<Predicted>>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
) {
    // how much we stick to the new position
    // let lerp = 0.1;
    // let lerp = 1.0;
    if let Ok(mut camera_pos) = camera_query.single_mut() {
        if let Ok(pos) = predicted.single() {
            let head = pos.front().0;
            // *camera_pos = Transform::from_translation(camera_pos.translation.mul_add(Vec3::splat(1.0 - lerp), Vec3::from((head, 0.0)) * lerp));
            *camera_pos = Transform::from_xyz(head.x, head.y, 0.0);
        }
    }
    // player is dead: camera follows killer's head
}

/// Switch camera to follow view, reset the projection
fn enter_follow_camera(mut camera_query: Query<&mut Projection, With<Camera>>) {
    if let Ok(mut projection) = camera_query.single_mut() {
        let Projection::Orthographic(projection) = &mut *projection else {
            return;
        };
        // NOTE: do not set the window size to >1.0 as this can cause jitters due to fractional pixel movement
        projection.scaling_mode = ScalingMode::WindowSize;
        projection.scale = 1.0;
    }
}

/// Switch camera to full view, reset the projection
fn enter_full_camera(mut camera_query: Query<&mut Projection, With<Camera>>) {
    if let Ok(mut projection) = camera_query.single_mut() {
        let Projection::Orthographic(projection) = &mut *projection else {
            return;
        };
        projection.scaling_mode = ScalingMode::WindowSize;
        projection.scale = 1.0;
    }
}
