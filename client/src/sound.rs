use bevy::audio::{
    AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, SpatialAudioSink,
    SpatialListener, SpatialScale, Volume,
};
use bevy::prelude::*;
use lightyear::prelude::Controlled;
use shared::config::{GameConfig, MovementConfig, SoundConfig};
use shared::network::protocol::prelude::{
    Acceleration, DeathReason, FoodBoost, Player, PlayerStatus, RoomId, SnakeHead, Speed,
    TailPoints,
};
use std::collections::{HashMap, HashSet};

use crate::collision::death::ConfirmedDeath;
use crate::food::ConfirmedFoodPickup;

const CRASH_SOUND: &str = "powerline/sounds/crash.ogg";
const FOOD_GRAB_SOUND: &str = "powerline/sounds/foodgrab.ogg";
const LINE_LOOP_SOUND: &str = "powerline/sounds/lineloop.ogg";
const LINE_FAST_LOOP_SOUND: &str = "powerline/sounds/lineloopfast.ogg";
const ELECTRO_LOOP_SOUND: &str = "powerline/sounds/electroloop.ogg";

pub(crate) struct SoundPlugin;

#[derive(Resource, Clone)]
struct PowerlineSounds {
    crash: Handle<AudioSource>,
    food_grab: Handle<AudioSource>,
    line_loop: Handle<AudioSource>,
    line_fast_loop: Handle<AudioSource>,
    electro_loop: Handle<AudioSource>,
}

#[derive(Resource, Default)]
struct LocalSpeedLoopState {
    line_loop: Option<Entity>,
    line_fast_loop: Option<Entity>,
    electro_loop: Option<Entity>,
}

#[derive(Resource, Default)]
struct RemoteSpeedLoopState {
    loops: HashMap<Entity, RemoteSpeedLoops>,
}

#[derive(Default)]
struct RemoteSpeedLoops {
    line_loop: Option<Entity>,
    line_fast_loop: Option<Entity>,
    electro_loop: Option<Entity>,
}

#[derive(Component, Default)]
struct LocalSpeedLoopSound;

#[derive(Component)]
struct RemoteSpeedLoopSound {
    #[allow(dead_code)]
    player: Entity,
}

#[derive(Component)]
struct SoundListener;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ListenerSnapshot {
    position: Vec2,
    room: RoomId,
}

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PowerlineSounds>();
        app.init_resource::<LocalSpeedLoopState>();
        app.init_resource::<RemoteSpeedLoopState>();
        app.add_systems(
            Update,
            (
                sync_spatial_listener,
                play_confirmed_death_sounds,
                play_confirmed_food_sounds,
                update_local_speed_loops,
                update_remote_speed_loops,
            )
                .chain(),
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
            electro_loop: asset_server.load(ELECTRO_LOOP_SOUND),
        }
    }
}

fn sync_spatial_listener(
    mut commands: Commands,
    config: Res<GameConfig>,
    players: Query<(&Player, &RoomId, Has<Controlled>)>,
    heads: Query<&SnakeHead>,
    mut listeners: Query<(Entity, &mut Transform, &mut SpatialListener), With<SoundListener>>,
) {
    if !config.sound.enabled || !config.sound.spatial_audio {
        for (entity, _, _) in &mut listeners {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(listener) = listener_snapshot(&players, &heads) else {
        return;
    };
    let ear_gap = config.sound.spatial_listener_ear_gap.max(0.0);
    let transform = Transform::from_translation(listener.position.extend(0.0));

    if let Some((_, mut existing_transform, mut spatial_listener)) = listeners.iter_mut().next() {
        *existing_transform = transform;
        *spatial_listener = SpatialListener::new(ear_gap);
    } else {
        commands.spawn((
            SoundListener,
            SpatialListener::new(ear_gap),
            transform,
            GlobalTransform::default(),
        ));
    }
}

fn play_confirmed_death_sounds(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    players: Query<(&Player, &RoomId, Has<Controlled>)>,
    heads: Query<&SnakeHead>,
    mut deaths: MessageReader<ConfirmedDeath>,
) {
    if !config.sound.enabled {
        for _ in deaths.read() {}
        return;
    }

    let listener = listener_snapshot(&players, &heads);
    for death in deaths.read() {
        let volume = death_sound_volume(death, listener, &config.sound);
        if volume <= 0.0 {
            continue;
        }
        let source_position = (!death.local_player).then_some(death.position).flatten();
        spawn_one_shot(
            &mut commands,
            sounds.crash.clone(),
            source_position,
            volume,
            &config.sound,
        );
    }
}

fn play_confirmed_food_sounds(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    players: Query<(&Player, &RoomId, Has<Controlled>)>,
    snakes: Query<(&SnakeHead, &RoomId)>,
    mut pickups: MessageReader<ConfirmedFoodPickup>,
) {
    if !config.sound.enabled {
        for _ in pickups.read() {}
        return;
    }

    let listener = listener_snapshot_from_roomed_tails(&players, &snakes);
    let local_snake = local_player_snake(&players);
    for pickup in pickups.read() {
        let collision = &pickup.collision;
        if Some(collision.snake) == local_snake {
            let volume = config.sound.master_volume * config.sound.food_volume;
            spawn_one_shot(
                &mut commands,
                sounds.food_grab.clone(),
                None,
                volume,
                &config.sound,
            );
            continue;
        }

        let Some(listener) = listener else {
            continue;
        };
        let Ok((head, room)) = snakes.get(collision.snake) else {
            continue;
        };
        if *room != listener.room {
            continue;
        }

        let source_position = head.position;
        let attenuation =
            distance_attenuation(source_position.distance(listener.position), &config.sound);
        let volume = config.sound.master_volume
            * config.sound.food_volume
            * config.sound.remote_food_volume
            * attenuation;
        spawn_one_shot(
            &mut commands,
            sounds.food_grab.clone(),
            Some(source_position),
            volume,
            &config.sound,
        );
    }
}

fn update_local_speed_loops(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    mut state: ResMut<LocalSpeedLoopState>,
    players: Query<(&Player, &RoomId, Has<Controlled>)>,
    controlled_snakes: Query<
        (&Speed, &Acceleration, &FoodBoost),
        (With<Controlled>, With<TailPoints>),
    >,
    speeds: Query<(&Speed, Option<&Acceleration>, Option<&FoodBoost>)>,
    mut sinks: Query<&mut AudioSink, With<LocalSpeedLoopSound>>,
    mut playback_settings: Query<&mut PlaybackSettings, With<LocalSpeedLoopSound>>,
) {
    let state_snapshot = local_player_sound_state(&controlled_snakes, &players, &speeds);
    let (line_volume, fast_volume) = speed_loop_volumes(
        state_snapshot.map(|state| state.speed),
        &config.sound,
        &config.movement,
    );
    let electro_volume = if state_snapshot.is_some_and(|state| {
        proximity_boost_active(state.acceleration, state.food_boost, &config.movement)
    }) {
        config.sound.master_volume * config.sound.electro_loop_volume
    } else {
        0.0
    };

    update_plain_loop(
        &mut commands,
        &mut state.line_loop,
        LocalSpeedLoopSound,
        sounds.line_loop.clone(),
        line_volume,
        &mut sinks,
        &mut playback_settings,
    );
    update_plain_loop(
        &mut commands,
        &mut state.line_fast_loop,
        LocalSpeedLoopSound,
        sounds.line_fast_loop.clone(),
        fast_volume,
        &mut sinks,
        &mut playback_settings,
    );
    update_plain_loop(
        &mut commands,
        &mut state.electro_loop,
        LocalSpeedLoopSound,
        sounds.electro_loop.clone(),
        electro_volume,
        &mut sinks,
        &mut playback_settings,
    );
}

fn update_remote_speed_loops(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    mut state: ResMut<RemoteSpeedLoopState>,
    players: Query<(Entity, &Player, &RoomId, &PlayerStatus, Has<Controlled>)>,
    heads: Query<&SnakeHead>,
    speeds: Query<&Speed>,
    accelerations: Query<&Acceleration>,
    food_boosts: Query<&FoodBoost>,
    mut sinks: Query<&mut SpatialAudioSink, With<RemoteSpeedLoopSound>>,
    mut transforms: Query<&mut Transform, With<RemoteSpeedLoopSound>>,
    mut playback_settings: Query<&mut PlaybackSettings, With<RemoteSpeedLoopSound>>,
) {
    if !config.sound.enabled || !config.sound.spatial_audio {
        clear_remote_speed_loops(&mut commands, &mut state);
        return;
    }

    let Some(listener) = remote_listener_snapshot(&players, &heads) else {
        clear_remote_speed_loops(&mut commands, &mut state);
        return;
    };

    let mut seen = HashSet::new();
    for (player_entity, player, room, status, is_local) in &players {
        if is_local || *status != PlayerStatus::Alive || *room != listener.room {
            continue;
        }
        let Some(snake) = player.snake else {
            continue;
        };
        let (Ok(head), Ok(speed)) = (heads.get(snake), speeds.get(snake)) else {
            continue;
        };
        let proximity_active = accelerations
            .get(snake)
            .ok()
            .zip(food_boosts.get(snake).ok())
            .is_some_and(|(acceleration, food_boost)| {
                proximity_boost_active(acceleration.0, food_boost.0, &config.movement)
            });

        seen.insert(player_entity);
        let source_position = head.position;
        let distance = source_position.distance(listener.position);
        let (line_volume, fast_volume) =
            remote_speed_loop_volumes(Some(speed.0), distance, &config.sound, &config.movement);
        let electro_volume = if proximity_active {
            config.sound.master_volume
                * config.sound.electro_loop_volume
                * config.sound.remote_speed_volume
                * distance_attenuation(distance, &config.sound)
        } else {
            0.0
        };
        let loops = state.loops.entry(player_entity).or_default();

        update_remote_speed_loop(
            &mut commands,
            &mut loops.line_loop,
            player_entity,
            sounds.line_loop.clone(),
            source_position,
            line_volume,
            &config.sound,
            &mut sinks,
            &mut transforms,
            &mut playback_settings,
        );
        update_remote_speed_loop(
            &mut commands,
            &mut loops.line_fast_loop,
            player_entity,
            sounds.line_fast_loop.clone(),
            source_position,
            fast_volume,
            &config.sound,
            &mut sinks,
            &mut transforms,
            &mut playback_settings,
        );
        update_remote_speed_loop(
            &mut commands,
            &mut loops.electro_loop,
            player_entity,
            sounds.electro_loop.clone(),
            source_position,
            electro_volume,
            &config.sound,
            &mut sinks,
            &mut transforms,
            &mut playback_settings,
        );
    }

    let stale_players = state
        .loops
        .keys()
        .copied()
        .filter(|player| !seen.contains(player))
        .collect::<Vec<_>>();
    for player in stale_players {
        if let Some(loops) = state.loops.remove(&player) {
            despawn_loop(&mut commands, loops.line_loop);
            despawn_loop(&mut commands, loops.line_fast_loop);
            despawn_loop(&mut commands, loops.electro_loop);
        }
    }
}

fn update_plain_loop<C: Component>(
    commands: &mut Commands,
    entity: &mut Option<Entity>,
    marker: C,
    sound: Handle<AudioSource>,
    volume: f32,
    sinks: &mut Query<&mut AudioSink, With<C>>,
    playback_settings: &mut Query<&mut PlaybackSettings, With<C>>,
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
                marker,
                AudioPlayer::new(sound),
                PlaybackSettings::LOOP.with_volume(Volume::Linear(volume)),
            ))
            .id(),
    );
}

fn update_remote_speed_loop(
    commands: &mut Commands,
    entity: &mut Option<Entity>,
    player: Entity,
    sound: Handle<AudioSource>,
    position: Vec2,
    volume: f32,
    sound_config: &SoundConfig,
    sinks: &mut Query<&mut SpatialAudioSink, With<RemoteSpeedLoopSound>>,
    transforms: &mut Query<&mut Transform, With<RemoteSpeedLoopSound>>,
    playback_settings: &mut Query<&mut PlaybackSettings, With<RemoteSpeedLoopSound>>,
) {
    if let Some(existing) = *entity {
        if let Ok(mut transform) = transforms.get_mut(existing) {
            transform.translation = position.extend(0.0);
        }
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

    if volume <= 0.0 || !sound_config.spatial_audio {
        return;
    }

    *entity = Some(
        commands
            .spawn((
                RemoteSpeedLoopSound { player },
                Transform::from_translation(position.extend(0.0)),
                GlobalTransform::default(),
                AudioPlayer::new(sound),
                spatial_playback_settings(PlaybackSettings::LOOP, volume, sound_config),
            ))
            .id(),
    );
}

fn spawn_one_shot(
    commands: &mut Commands,
    sound: Handle<AudioSource>,
    source_position: Option<Vec2>,
    volume: f32,
    sound_config: &SoundConfig,
) {
    if volume <= 0.0 {
        return;
    }

    let playback = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume));
    if let Some(source_position) = source_position.filter(|_| sound_config.spatial_audio) {
        commands.spawn((
            AudioPlayer::new(sound),
            spatial_playback_settings(playback, volume, sound_config),
            Transform::from_translation(source_position.extend(0.0)),
            GlobalTransform::default(),
        ));
    } else {
        commands.spawn((AudioPlayer::new(sound), playback));
    }
}

fn spatial_playback_settings(
    playback: PlaybackSettings,
    volume: f32,
    sound_config: &SoundConfig,
) -> PlaybackSettings {
    playback
        .with_volume(Volume::Linear(volume))
        .with_spatial(true)
        .with_spatial_scale(SpatialScale::new_2d(sound_config.spatial_scale.max(0.0001)))
}

fn clear_remote_speed_loops(commands: &mut Commands, state: &mut RemoteSpeedLoopState) {
    let loops = std::mem::take(&mut state.loops);
    for (_, loops) in loops {
        despawn_loop(commands, loops.line_loop);
        despawn_loop(commands, loops.line_fast_loop);
        despawn_loop(commands, loops.electro_loop);
    }
}

fn despawn_loop(commands: &mut Commands, entity: Option<Entity>) {
    if let Some(entity) = entity {
        commands.entity(entity).despawn();
    }
}

fn listener_snapshot(
    players: &Query<(&Player, &RoomId, Has<Controlled>)>,
    heads: &Query<&SnakeHead>,
) -> Option<ListenerSnapshot> {
    players
        .iter()
        .find(|(_, _, is_local)| *is_local)
        .and_then(|(player, room, _)| {
            player.snake.and_then(|snake| {
                heads.get(snake).ok().map(|head| ListenerSnapshot {
                    position: head.position,
                    room: *room,
                })
            })
        })
}

fn listener_snapshot_from_roomed_tails(
    players: &Query<(&Player, &RoomId, Has<Controlled>)>,
    tails: &Query<(&SnakeHead, &RoomId)>,
) -> Option<ListenerSnapshot> {
    players
        .iter()
        .find(|(_, _, is_local)| *is_local)
        .and_then(|(player, room, _)| {
            player.snake.and_then(|snake| {
                tails.get(snake).ok().map(|(head, _)| ListenerSnapshot {
                    position: head.position,
                    room: *room,
                })
            })
        })
}

fn remote_listener_snapshot(
    players: &Query<(Entity, &Player, &RoomId, &PlayerStatus, Has<Controlled>)>,
    heads: &Query<&SnakeHead>,
) -> Option<ListenerSnapshot> {
    players
        .iter()
        .find(|(_, _, _, _, is_local)| *is_local)
        .and_then(|(_, player, room, _, _)| {
            player.snake.and_then(|snake| {
                heads.get(snake).ok().map(|head| ListenerSnapshot {
                    position: head.position,
                    room: *room,
                })
            })
        })
}

fn local_player_snake(players: &Query<(&Player, &RoomId, Has<Controlled>)>) -> Option<Entity> {
    players
        .iter()
        .find(|(_, _, is_local)| *is_local)
        .and_then(|(player, _, _)| player.snake)
}

#[derive(Clone, Copy, Debug)]
struct LocalSoundState {
    speed: f32,
    acceleration: f32,
    food_boost: f32,
}

fn local_player_sound_state(
    controlled_snakes: &Query<
        (&Speed, &Acceleration, &FoodBoost),
        (With<Controlled>, With<TailPoints>),
    >,
    players: &Query<(&Player, &RoomId, Has<Controlled>)>,
    speeds: &Query<(&Speed, Option<&Acceleration>, Option<&FoodBoost>)>,
) -> Option<LocalSoundState> {
    if let Some((speed, acceleration, food_boost)) = controlled_snakes.iter().next() {
        return Some(LocalSoundState {
            speed: speed.0,
            acceleration: acceleration.0,
            food_boost: food_boost.0,
        });
    }

    let snake = local_player_snake(players)?;
    speeds
        .get(snake)
        .ok()
        .map(|(speed, acceleration, food_boost)| LocalSoundState {
            speed: speed.0,
            acceleration: acceleration.map(|value| value.0).unwrap_or(0.0),
            food_boost: food_boost.map(|value| value.0).unwrap_or(0.0),
        })
}

fn proximity_boost_active(acceleration: f32, food_boost: f32, movement: &MovementConfig) -> bool {
    acceleration - food_boost > movement.base_acceleration + 0.001
}

fn death_sound_volume(
    death: &ConfirmedDeath,
    listener: Option<ListenerSnapshot>,
    sound: &SoundConfig,
) -> f32 {
    let base = sound.master_volume
        * sound.death_volume
        * death_reason_volume_multiplier(&death.message.reason);
    if death.local_player {
        return base;
    }

    let Some(listener) = listener else {
        return 0.0;
    };
    let Some(source_position) = death.position else {
        return 0.0;
    };
    if death.message.room != listener.room {
        return 0.0;
    }

    base * sound.remote_death_volume
        * distance_attenuation(source_position.distance(listener.position), sound)
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

fn remote_speed_loop_volumes(
    speed: Option<f32>,
    distance: f32,
    sound: &SoundConfig,
    movement: &MovementConfig,
) -> (f32, f32) {
    let (line, fast) = speed_loop_volumes(speed, sound, movement);
    let attenuation = distance_attenuation(distance, sound) * sound.remote_speed_volume;
    (line * attenuation, fast * attenuation)
}

fn distance_attenuation(distance: f32, sound: &SoundConfig) -> f32 {
    let full_volume_distance = sound.remote_sound_full_volume_distance.max(0.0);
    let max_distance = sound
        .remote_sound_max_distance
        .max(full_volume_distance + f32::EPSILON);

    if distance <= full_volume_distance {
        return 1.0;
    }
    if distance >= max_distance {
        return 0.0;
    }

    let ratio = 1.0 - ((distance - full_volume_distance) / (max_distance - full_volume_distance));
    ratio.clamp(0.0, 1.0)
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

    #[test]
    fn remote_attenuation_is_full_nearby_and_silent_far_away() {
        let sound = SoundConfig::default();

        assert_eq!(
            distance_attenuation(sound.remote_sound_full_volume_distance * 0.5, &sound),
            1.0
        );
        assert_eq!(
            distance_attenuation(sound.remote_sound_max_distance, &sound),
            0.0
        );
        assert!(
            distance_attenuation(
                (sound.remote_sound_full_volume_distance + sound.remote_sound_max_distance) * 0.5,
                &sound,
            ) < 1.0
        );
    }

    #[test]
    fn remote_speed_loop_volume_depends_on_distance() {
        let sound = SoundConfig::default();
        let movement = MovementConfig::default();

        let nearby = remote_speed_loop_volumes(Some(movement.max_speed), 0.0, &sound, &movement);
        let far = remote_speed_loop_volumes(
            Some(movement.max_speed),
            sound.remote_sound_max_distance,
            &sound,
            &movement,
        );

        assert!(nearby.0 > far.0);
        assert!(nearby.1 > far.1);
        assert_eq!(far, (0.0, 0.0));
    }

    #[test]
    fn proximity_boost_detects_acceleration_after_removing_food_boost() {
        let movement = MovementConfig::default();

        assert!(!proximity_boost_active(
            movement.base_acceleration + 0.03,
            0.03,
            &movement
        ));
        assert!(proximity_boost_active(0.04, 0.01, &movement));
    }
}
