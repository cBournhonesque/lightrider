use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume,
};
use bevy::prelude::*;
use lightyear::prelude::{Client, Controlled, MessageReceiver};
use shared::config::{GameConfig, MovementConfig, SoundConfig};
use shared::network::protocol::prelude::{DeathReason, FoodCollision, Player, Speed};

use crate::collision::death::ConfirmedDeath;

const CRASH_SOUND: &str = "powerline/sounds/crash.ogg";
const FOOD_GRAB_SOUND: &str = "powerline/sounds/foodgrab.ogg";
const LINE_LOOP_SOUND: &str = "powerline/sounds/lineloop.ogg";
const LINE_FAST_LOOP_SOUND: &str = "powerline/sounds/lineloopfast.ogg";

pub(crate) struct SoundPlugin;

#[derive(Resource, Clone)]
struct PowerlineSounds {
    crash: Handle<AudioSource>,
    food_grab: Handle<AudioSource>,
    line_loop: Handle<AudioSource>,
    line_fast_loop: Handle<AudioSource>,
}

#[derive(Resource, Default)]
struct SpeedLoopState {
    line_loop: Option<Entity>,
    line_fast_loop: Option<Entity>,
}

#[derive(Component)]
struct SpeedLoopSound;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PowerlineSounds>();
        app.init_resource::<SpeedLoopState>();
        app.add_systems(
            Update,
            (
                play_confirmed_death_sounds,
                play_confirmed_food_sounds,
                update_speed_loops,
            ),
        );
    }
}

impl FromWorld for PowerlineSounds {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        Self {
            crash: asset_server.load(CRASH_SOUND),
            food_grab: asset_server.load(FOOD_GRAB_SOUND),
            line_loop: asset_server.load(LINE_LOOP_SOUND),
            line_fast_loop: asset_server.load(LINE_FAST_LOOP_SOUND),
        }
    }
}

fn play_confirmed_death_sounds(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    mut deaths: MessageReader<ConfirmedDeath>,
) {
    if !config.sound.enabled {
        for _ in deaths.read() {}
        return;
    }

    for death in deaths.read() {
        let local_multiplier = if death.local_player { 1.0 } else { 0.65 };
        let volume = config.sound.master_volume
            * config.sound.death_volume
            * local_multiplier
            * death_reason_volume_multiplier(&death.message.reason);
        if volume <= 0.0 {
            continue;
        }
        commands.spawn((
            AudioPlayer::new(sounds.crash.clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
        ));
    }
}

fn play_confirmed_food_sounds(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    players: Query<&Player, With<Controlled>>,
    mut receivers: Query<&mut MessageReceiver<FoodCollision>, With<Client>>,
) {
    let Ok(mut receiver) = receivers.single_mut() else {
        return;
    };
    if !config.sound.enabled {
        for _ in receiver.receive() {}
        return;
    }

    let local_snake = players.single().ok().and_then(|player| player.snake);
    for collision in receiver.receive() {
        if Some(collision.snake) != local_snake {
            continue;
        }
        let volume = config.sound.master_volume * config.sound.food_volume;
        if volume <= 0.0 {
            continue;
        }
        commands.spawn((
            AudioPlayer::new(sounds.food_grab.clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
        ));
    }
}

fn update_speed_loops(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    mut state: ResMut<SpeedLoopState>,
    players: Query<&Player, With<Controlled>>,
    speeds: Query<&Speed>,
    mut sinks: Query<&mut AudioSink, With<SpeedLoopSound>>,
    mut playback_settings: Query<&mut PlaybackSettings, With<SpeedLoopSound>>,
) {
    let speed = local_player_speed(&players, &speeds);
    let (line_volume, fast_volume) = speed_loop_volumes(speed, &config.sound, &config.movement);

    update_speed_loop(
        &mut commands,
        &mut state.line_loop,
        sounds.line_loop.clone(),
        line_volume,
        &mut sinks,
        &mut playback_settings,
    );
    update_speed_loop(
        &mut commands,
        &mut state.line_fast_loop,
        sounds.line_fast_loop.clone(),
        fast_volume,
        &mut sinks,
        &mut playback_settings,
    );
}

fn update_speed_loop(
    commands: &mut Commands,
    entity: &mut Option<Entity>,
    sound: Handle<AudioSource>,
    volume: f32,
    sinks: &mut Query<&mut AudioSink, With<SpeedLoopSound>>,
    playback_settings: &mut Query<&mut PlaybackSettings, With<SpeedLoopSound>>,
) {
    if let Some(existing) = *entity {
        if let Ok(mut sink) = sinks.get_mut(existing) {
            sink.set_volume(Volume::Linear(volume));
            return;
        }
        if let Ok(mut settings) = playback_settings.get_mut(existing) {
            settings.volume = Volume::Linear(volume);
            return;
        }
        *entity = None;
    }

    if volume <= 0.0 {
        return;
    }

    *entity = Some(
        commands
            .spawn((
                SpeedLoopSound,
                AudioPlayer::new(sound),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(volume)),
            ))
            .id(),
    );
}

fn local_player_speed(
    players: &Query<&Player, With<Controlled>>,
    speeds: &Query<&Speed>,
) -> Option<f32> {
    let player = players.single().ok()?;
    let snake = player.snake?;
    speeds.get(snake).ok().map(|speed| speed.0)
}

fn speed_loop_volumes(
    speed: Option<f32>,
    sound: &SoundConfig,
    movement: &MovementConfig,
) -> (f32, f32) {
    if !sound.enabled {
        return (0.0, 0.0);
    }
    let Some(speed) = speed else {
        return (0.0, 0.0);
    };

    let line_ratio = normalized_range(
        speed,
        sound.speed_loop_start_speed,
        movement.max_speed.max(sound.speed_loop_start_speed),
    );
    let line_volume = if line_ratio <= 0.0 {
        0.0
    } else {
        sound.master_volume
            * lerp(
                sound.speed_loop_min_volume,
                sound.speed_loop_max_volume,
                line_ratio,
            )
    };

    let fast_ratio = normalized_range(
        speed,
        sound.speed_fast_loop_start_speed,
        movement.max_speed.max(sound.speed_fast_loop_start_speed),
    );
    let fast_volume = sound.master_volume * sound.speed_fast_loop_volume * fast_ratio;

    (line_volume, fast_volume)
}

fn normalized_range(value: f32, start: f32, end: f32) -> f32 {
    let width = (end - start).max(f32::EPSILON);
    ((value - start) / width).clamp(0.0, 1.0)
}

fn lerp(start: f32, end: f32, ratio: f32) -> f32 {
    start + (end - start) * ratio.clamp(0.0, 1.0)
}

fn death_reason_volume_multiplier(_reason: &DeathReason) -> f32 {
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speed_loop_is_silent_without_a_local_speed() {
        assert_eq!(
            speed_loop_volumes(None, &SoundConfig::default(), &MovementConfig::default()),
            (0.0, 0.0)
        );
    }

    #[test]
    fn speed_loop_volume_grows_with_speed() {
        let sound = SoundConfig::default();
        let movement = MovementConfig::default();

        let slow = speed_loop_volumes(Some(sound.speed_loop_start_speed), &sound, &movement);
        let fast = speed_loop_volumes(Some(movement.max_speed), &sound, &movement);

        assert_eq!(slow, (0.0, 0.0));
        assert!(fast.0 > sound.speed_loop_min_volume);
        assert!(fast.1 > 0.0);
    }
}
