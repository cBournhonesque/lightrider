use crate::collision::death::DeathView;
use crate::inputs::ToggleCamera;
use bevy::camera::{Projection, ScalingMode};
use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::input::bei::Start;
use lightyear::prelude::Predicted;
use shared::config::GameConfig;
use shared::network::protocol::prelude::{TailLength, TailPoints};

pub struct CameraPlugin {
    pub(crate) debug_enabled: bool,
}

#[derive(Resource, Clone, Copy, Debug)]
struct CameraSettings {
    debug_enabled: bool,
}

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
        app.insert_resource(CameraSettings {
            debug_enabled: self.debug_enabled,
        });

        // state
        app.init_state::<CameraState>();
        // follow
        app.add_systems(OnEnter(CameraState::Follow), enter_follow_camera);

        // full
        app.add_systems(OnEnter(CameraState::Full), enter_full_camera);

        // we could run during update, because the predicted movement is updated in FixedUpdate
        app.add_systems(
            PostUpdate,
            follow_camera.after(FrameInterpolationSystems::Interpolate),
        );
        app.add_observer(toggle_camera);
    }
}

fn toggle_camera(
    _trigger: On<Start<ToggleCamera>>,
    settings: Res<CameraSettings>,
    mut next_state: ResMut<NextState<CameraState>>,
    current_state: Res<State<CameraState>>,
) {
    if !settings.debug_enabled {
        return;
    }
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
    config: Res<GameConfig>,
    camera_state: Res<State<CameraState>>,
    death_view: Res<DeathView>,
    predicted: Query<(&TailPoints, &TailLength), With<Predicted>>,
    tails: Query<&TailPoints>,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera>>,
) {
    // how much we stick to the new position
    // let lerp = 0.1;
    // let lerp = 1.0;
    if let Ok((mut camera_pos, mut projection)) = camera_query.single_mut() {
        if let Ok((pos, tail_length)) = predicted.single() {
            let head = pos.front().0;
            // *camera_pos = Transform::from_translation(camera_pos.translation.mul_add(Vec3::splat(1.0 - lerp), Vec3::from((head, 0.0)) * lerp));
            camera_pos.translation.x = head.x;
            camera_pos.translation.y = head.y;
            if *camera_state.get() == CameraState::Follow {
                set_camera_scale(
                    &mut projection,
                    normal_camera_scale_for_tail(&config, tail_length),
                );
            }
        } else if let Some(killer_snake) = death_view.killer_snake {
            if let Ok(pos) = tails.get(killer_snake) {
                let head = pos.front().0;
                camera_pos.translation.x = head.x;
                camera_pos.translation.y = head.y;
            }
        }
    }
}

/// Switch camera to follow view, reset the projection
fn enter_follow_camera(
    config: Res<GameConfig>,
    mut camera_query: Query<&mut Projection, With<Camera>>,
) {
    if let Ok(mut projection) = camera_query.single_mut() {
        set_camera_scale(&mut projection, config.render.normal_camera_scale.max(0.1));
    }
}

/// Switch camera to full view, reset the projection
fn enter_full_camera(
    config: Res<GameConfig>,
    mut camera_query: Query<&mut Projection, With<Camera>>,
) {
    if let Ok(mut projection) = camera_query.single_mut() {
        set_camera_scale(&mut projection, config.render.debug_camera_scale.max(0.1));
    }
}

fn set_camera_scale(projection: &mut Projection, scale: f32) {
    let Projection::Orthographic(projection) = &mut *projection else {
        return;
    };
    // NOTE: do not set the window size to >1.0 as this can cause jitters due to fractional pixel movement
    projection.scaling_mode = ScalingMode::WindowSize;
    projection.scale = scale.max(0.1);
}

fn normal_camera_scale_for_tail(config: &GameConfig, tail_length: &TailLength) -> f32 {
    let min_scale = config.render.normal_camera_scale.max(0.1);
    let max_scale = config.render.normal_camera_max_scale.max(min_scale);
    let growth = (tail_length.current_size - config.movement.starting_tail_length).max(0.0);
    (min_scale + growth * config.render.normal_camera_growth_per_tail_length.max(0.0))
        .clamp(min_scale, max_scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_camera_scale_grows_with_tail_length() {
        let config = GameConfig::default();

        assert_eq!(
            normal_camera_scale_for_tail(
                &config,
                &TailLength {
                    current_size: config.movement.starting_tail_length,
                    target_size: config.movement.starting_tail_length,
                },
            ),
            config.render.normal_camera_scale
        );

        let grown = normal_camera_scale_for_tail(
            &config,
            &TailLength {
                current_size: config.movement.starting_tail_length + 200.0,
                target_size: config.movement.starting_tail_length + 200.0,
            },
        );
        assert!(grown > config.render.normal_camera_scale);
        assert!(grown <= config.render.normal_camera_max_scale);
    }
}
