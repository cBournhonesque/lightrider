use crate::collision::death::{ConfirmedDeath, DeathView};
use crate::inputs::ToggleCamera;
use bevy::camera::{Projection, ScalingMode};
use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::input::bei::Start;
use lightyear::prelude::Predicted;
use shared::config::GameConfig;
use shared::network::protocol::prelude::{SnakeHead, TailLength};

pub struct CameraPlugin {
    pub(crate) debug_enabled: bool,
}

#[derive(Resource, Clone, Copy, Debug)]
struct CameraSettings {
    debug_enabled: bool,
}

#[derive(Resource, Clone, Copy, Debug, Default)]
struct CameraShake {
    remaining_seconds: f32,
    duration_seconds: f32,
    amplitude: f32,
    phase: f32,
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
        app.init_resource::<CameraShake>();

        // state
        app.init_state::<CameraState>();
        // follow
        app.add_systems(OnEnter(CameraState::Follow), enter_follow_camera);

        // full
        app.add_systems(OnEnter(CameraState::Full), enter_full_camera);

        // we could run during update, because the predicted movement is updated in FixedUpdate
        app.add_systems(
            PostUpdate,
            (trigger_death_camera_shake, follow_camera)
                .chain()
                .after(FrameInterpolationSystems::Interpolate),
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
    time: Res<Time>,
    camera_state: Res<State<CameraState>>,
    death_view: Res<DeathView>,
    mut shake: ResMut<CameraShake>,
    predicted: Query<(&SnakeHead, &TailLength), With<Predicted>>,
    tails: Query<&SnakeHead>,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera>>,
) {
    // how much we stick to the new position
    // let lerp = 0.1;
    // let lerp = 1.0;
    if let Ok((mut camera_pos, mut projection)) = camera_query.single_mut() {
        let mut has_target = false;
        if let Ok((head, tail_length)) = predicted.single() {
            // *camera_pos = Transform::from_translation(camera_pos.translation.mul_add(Vec3::splat(1.0 - lerp), Vec3::from((head, 0.0)) * lerp));
            camera_pos.translation.x = head.position.x;
            camera_pos.translation.y = head.position.y;
            has_target = true;
            if *camera_state.get() == CameraState::Follow {
                smooth_camera_scale(
                    &mut projection,
                    normal_camera_scale_for_tail(&config, tail_length),
                    time.delta_secs(),
                    config.render.normal_camera_scale_smoothing,
                );
            }
        } else if let Some(killer_snake) = death_view.killer_snake {
            if let Ok(head) = tails.get(killer_snake) {
                camera_pos.translation.x = head.position.x;
                camera_pos.translation.y = head.position.y;
                has_target = true;
            }
        }

        if has_target {
            let offset = shake.sample(time.delta_secs());
            camera_pos.translation.x += offset.x;
            camera_pos.translation.y += offset.y;
        } else {
            shake.decay(time.delta_secs());
        }
    }
}

fn trigger_death_camera_shake(
    mut deaths: MessageReader<ConfirmedDeath>,
    mut shake: ResMut<CameraShake>,
) {
    for death in deaths.read() {
        if death.local_player {
            shake.start(0.22, 7.0);
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

fn smooth_camera_scale(
    projection: &mut Projection,
    target_scale: f32,
    delta_seconds: f32,
    smoothing: f32,
) {
    let Some(current_scale) = camera_scale(projection) else {
        return;
    };
    let t = smoothing_factor(smoothing, delta_seconds);
    set_camera_scale(
        projection,
        current_scale + (target_scale - current_scale) * t,
    );
}

fn camera_scale(projection: &Projection) -> Option<f32> {
    let Projection::Orthographic(projection) = projection else {
        return None;
    };
    Some(projection.scale)
}

fn smoothing_factor(smoothing: f32, delta_seconds: f32) -> f32 {
    if smoothing <= 0.0 {
        1.0
    } else {
        1.0 - (-smoothing * delta_seconds.max(0.0)).exp()
    }
}

fn normal_camera_scale_for_tail(config: &GameConfig, tail_length: &TailLength) -> f32 {
    let min_scale = config.render.normal_camera_scale.max(0.1);
    let max_scale = config.render.normal_camera_max_scale.max(min_scale);
    let growth = (tail_length.current_size - config.movement.starting_tail_length).max(0.0);
    (min_scale + growth * config.render.normal_camera_growth_per_tail_length.max(0.0))
        .clamp(min_scale, max_scale)
}

impl CameraShake {
    fn start(&mut self, duration_seconds: f32, amplitude: f32) {
        self.remaining_seconds = duration_seconds.max(0.0);
        self.duration_seconds = duration_seconds.max(f32::EPSILON);
        self.amplitude = amplitude.max(0.0);
        self.phase += 1.618_034;
    }

    fn sample(&mut self, delta_seconds: f32) -> Vec2 {
        if self.remaining_seconds <= 0.0 || self.amplitude <= 0.0 {
            return Vec2::ZERO;
        }

        self.decay(delta_seconds);
        let progress = 1.0 - (self.remaining_seconds / self.duration_seconds.max(f32::EPSILON));
        let strength = self.amplitude * (1.0 - progress).powi(2);
        let phase = self.phase + progress * std::f32::consts::TAU * 14.0;
        Vec2::new(phase.sin(), (phase * 1.37).cos()) * strength
    }

    fn decay(&mut self, delta_seconds: f32) {
        self.remaining_seconds = (self.remaining_seconds - delta_seconds.max(0.0)).max(0.0);
    }
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

    #[test]
    fn smoothing_factor_can_snap_or_smooth() {
        assert_eq!(smoothing_factor(0.0, 1.0), 1.0);
        let smoothed = smoothing_factor(6.0, 1.0 / 60.0);
        assert!(smoothed > 0.0);
        assert!(smoothed < 1.0);
    }

    #[test]
    fn death_camera_shake_decays_to_zero() {
        let mut shake = CameraShake::default();
        shake.start(0.2, 7.0);

        assert_ne!(shake.sample(1.0 / 60.0), Vec2::ZERO);
        shake.sample(1.0);
        assert_eq!(shake.sample(1.0 / 60.0), Vec2::ZERO);
    }
}
