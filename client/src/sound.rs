use bevy::ecs::query::Or;
use bevy::prelude::*;
use bevy_seedling::prelude::{
    sample_effects, AudioSample, DefaultPoolSize, EffectsQuery, FirewheelConfig, SampleEffects,
    SamplePlayer, SeedlingPlugin, SpatialBasicNode, SpatialListener2D, SpatialScale, Volume,
    VolumeNode,
};
use lightyear::prelude::{Controlled, Interpolated, Predicted, Replicated};
use shared::config::{GameConfig, MovementConfig, SoundConfig};
use shared::network::protocol::prelude::{
    Acceleration, DeathReason, Direction, FoodBoost, Player, PlayerStatus, RoomId, SnakeHead,
    Speed, TailPoints,
};
use std::collections::{HashMap, HashSet};

use crate::collision::death::ConfirmedDeath;
use crate::food::ConfirmedFoodPickup;

const SOUND_ATLAS: &str = "powerline/sounds/sound.ogg";
const SOUND_ATLAS_DURATION_MS: f64 = 18_000.0;
const CRASH_SLICE: SoundSlice = SoundSlice::new(0.0, 804.0589569160998);
const ELECTRO_LOOP_SLICE: SoundSlice = SoundSlice::new(2_000.0, 1_821.1791383219954);
const FOOD_GRAB_SLICE: SoundSlice = SoundSlice::new(5_000.0, 461.29251700680294);
const LINE_LOOP_SLICE: SoundSlice = SoundSlice::new(7_000.0, 2_946.1224489795923);
const LINE_FAST_LOOP_SLICE: SoundSlice = SoundSlice::new(11_000.0, 2_000.0);
const SPARK_SLICE: SoundSlice = SoundSlice::new(14_000.0, 87.93650793650798);
const TURN_SLICE: SoundSlice = SoundSlice::new(16_000.0, 500.0);
const SILENT_VOLUME_EPSILON: f32 = 0.001;
const VOLUME_UPDATE_EPSILON: f32 = 0.005;
const REMOTE_LOOP_POSITION_UPDATE_DISTANCE: f32 = 8.0;
const MAX_REMOTE_SPEED_LOOP_SNAKES: usize = 2;
const MAX_ONE_SHOT_SOUNDS_PER_FRAME: usize = 2;
const MAX_ONE_SHOT_SOUND_TOKENS: f32 = 24.0;
const ONE_SHOT_SOUND_TOKENS_PER_SECOND: f32 = 24.0;
const FOOD_SOUND_COOLDOWN_SECONDS: f64 = 0.075;
const LOCAL_ELECTRO_BASE_VOLUME_RATIO: f32 = 0.28;
const FIREWHEEL_CHANNEL_CAPACITY: u32 = 65_536;
const FIREWHEEL_EVENT_QUEUE_CAPACITY: usize = 1024;
const FIREWHEEL_IMMEDIATE_EVENT_CAPACITY: usize = 4096;
const FIREWHEEL_SCHEDULED_EVENT_CAPACITY: usize = 4096;
const SAMPLE_POOL_MIN_SIZE: usize = 2;
const SAMPLE_POOL_MAX_SIZE: usize = 8;

pub(crate) struct SoundPlugin;

#[derive(Resource, Clone)]
struct PowerlineSounds {
    atlas: Handle<AudioSample>,
    crash: Option<Handle<AudioSample>>,
    food_grab: Option<Handle<AudioSample>>,
    line_loop: Option<Handle<AudioSample>>,
    line_fast_loop: Option<Handle<AudioSample>>,
    electro_loop: Option<Handle<AudioSample>>,
    spark: Option<Handle<AudioSample>>,
    turn: Option<Handle<AudioSample>>,
}

#[derive(Clone, Copy)]
struct SoundSlice {
    start_ms: f64,
    duration_ms: f64,
}

struct PowerlineSoundSamples {
    crash: AudioSample,
    food_grab: AudioSample,
    line_loop: AudioSample,
    line_fast_loop: AudioSample,
    electro_loop: AudioSample,
    spark: AudioSample,
    turn: AudioSample,
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

#[derive(Resource, Default)]
struct TurnSoundState {
    directions: HashMap<Entity, Direction>,
}

#[derive(Resource, Default)]
struct ProximityBoostSoundState {
    active: HashSet<Entity>,
}

#[derive(Resource, Default)]
struct FoodSoundCooldown {
    last_played_seconds: Option<f64>,
}

#[derive(Resource, Default)]
struct OneShotSoundBudget {
    spawned_this_frame: usize,
    tokens: f32,
    last_refill_seconds: Option<f64>,
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
    snake: Entity,
}

#[derive(Component)]
struct SoundListener;

#[derive(Clone, Copy, Debug, PartialEq)]
struct ListenerSnapshot {
    position: Vec2,
    room: RoomId,
}

#[derive(Clone, Copy)]
struct RemoteLoopCandidate {
    snake: Entity,
    position: Vec2,
    line_volume: f32,
    fast_volume: f32,
    electro_volume: f32,
    distance: f32,
}

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SeedlingPlugin {
            config: firewheel_config(),
            ..default()
        });
        app.insert_resource(DefaultPoolSize(SAMPLE_POOL_MIN_SIZE..=SAMPLE_POOL_MAX_SIZE));
        app.init_resource::<PowerlineSounds>();
        app.init_resource::<LocalSpeedLoopState>();
        app.init_resource::<RemoteSpeedLoopState>();
        app.init_resource::<TurnSoundState>();
        app.init_resource::<ProximityBoostSoundState>();
        app.init_resource::<FoodSoundCooldown>();
        app.init_resource::<OneShotSoundBudget>();
        app.add_systems(
            Update,
            (
                reset_one_shot_sound_budget,
                build_powerline_sound_slices,
                sync_spatial_listener,
                play_confirmed_death_sounds,
                play_confirmed_food_sounds,
                play_turn_sounds,
                play_proximity_boost_sounds,
                update_local_speed_loops,
                update_remote_speed_loops,
            )
                .chain(),
        );
    }
}

fn firewheel_config() -> FirewheelConfig {
    FirewheelConfig {
        initial_node_capacity: 1024,
        initial_edge_capacity: 2048,
        channel_capacity: FIREWHEEL_CHANNEL_CAPACITY,
        event_queue_capacity: FIREWHEEL_EVENT_QUEUE_CAPACITY,
        immediate_event_capacity: FIREWHEEL_IMMEDIATE_EVENT_CAPACITY,
        scheduled_event_capacity: FIREWHEEL_SCHEDULED_EVENT_CAPACITY,
        ..default()
    }
}

fn reset_one_shot_sound_budget(time: Res<Time>, mut budget: ResMut<OneShotSoundBudget>) {
    budget.spawned_this_frame = 0;
    let now = time.elapsed_secs_f64();
    let Some(last_refill_seconds) = budget.last_refill_seconds else {
        budget.tokens = MAX_ONE_SHOT_SOUND_TOKENS;
        budget.last_refill_seconds = Some(now);
        return;
    };
    let elapsed = (now - last_refill_seconds).max(0.0) as f32;
    budget.tokens =
        (budget.tokens + elapsed * ONE_SHOT_SOUND_TOKENS_PER_SECOND).min(MAX_ONE_SHOT_SOUND_TOKENS);
    budget.last_refill_seconds = Some(now);
}

impl FromWorld for PowerlineSounds {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        Self {
            atlas: asset_server.load(SOUND_ATLAS),
            crash: None,
            food_grab: None,
            line_loop: None,
            line_fast_loop: None,
            electro_loop: None,
            spark: None,
            turn: None,
        }
    }
}

impl PowerlineSounds {
    fn is_ready(&self) -> bool {
        self.crash.is_some()
    }

    fn crash(&self) -> Option<Handle<AudioSample>> {
        self.crash.clone()
    }

    fn food_grab(&self) -> Option<Handle<AudioSample>> {
        self.food_grab.clone()
    }

    fn spark(&self) -> Option<Handle<AudioSample>> {
        self.spark.clone()
    }

    fn turn(&self) -> Option<Handle<AudioSample>> {
        self.turn.clone()
    }

    fn speed_loops(&self) -> Option<SpeedLoopSounds> {
        Some(SpeedLoopSounds {
            line_loop: self.line_loop.clone()?,
            line_fast_loop: self.line_fast_loop.clone()?,
            electro_loop: self.electro_loop.clone()?,
        })
    }
}

#[derive(Clone)]
struct SpeedLoopSounds {
    line_loop: Handle<AudioSample>,
    line_fast_loop: Handle<AudioSample>,
    electro_loop: Handle<AudioSample>,
}

fn build_powerline_sound_slices(
    mut sounds: ResMut<PowerlineSounds>,
    mut samples: ResMut<Assets<AudioSample>>,
) {
    if sounds.is_ready() {
        return;
    }

    let Some(powerline_samples) = samples
        .get(&sounds.atlas)
        .map(powerline_sound_samples_from_atlas)
    else {
        return;
    };

    sounds.crash = Some(samples.add(powerline_samples.crash));
    sounds.food_grab = Some(samples.add(powerline_samples.food_grab));
    sounds.line_loop = Some(samples.add(powerline_samples.line_loop));
    sounds.line_fast_loop = Some(samples.add(powerline_samples.line_fast_loop));
    sounds.electro_loop = Some(samples.add(powerline_samples.electro_loop));
    sounds.spark = Some(samples.add(powerline_samples.spark));
    sounds.turn = Some(samples.add(powerline_samples.turn));
}

fn powerline_sound_samples_from_atlas(atlas: &AudioSample) -> PowerlineSoundSamples {
    PowerlineSoundSamples {
        crash: audio_sprite_slice(atlas, CRASH_SLICE),
        food_grab: audio_sprite_slice(atlas, FOOD_GRAB_SLICE),
        line_loop: audio_sprite_slice(atlas, LINE_LOOP_SLICE),
        line_fast_loop: audio_sprite_slice(atlas, LINE_FAST_LOOP_SLICE),
        electro_loop: audio_sprite_slice(atlas, ELECTRO_LOOP_SLICE),
        spark: audio_sprite_slice(atlas, SPARK_SLICE),
        turn: audio_sprite_slice(atlas, TURN_SLICE),
    }
}

fn audio_sprite_slice(atlas: &AudioSample, slice: SoundSlice) -> AudioSample {
    let sample = atlas.get();
    let range = sound_slice_frame_range(sample.len_frames(), slice);
    let frame_count = range.end.saturating_sub(range.start).max(1);
    let mut channels = vec![vec![0.0; frame_count]; sample.num_channels().get()];
    let mut buffers = channels
        .iter_mut()
        .map(Vec::as_mut_slice)
        .collect::<Vec<_>>();
    sample.fill_buffers(&mut buffers, 0..frame_count, range.start as u64);
    AudioSample::new(channels, atlas.original_sample_rate())
}

fn sound_slice_frame_range(total_frames: u64, slice: SoundSlice) -> std::ops::Range<usize> {
    if total_frames == 0 {
        return 0..1;
    }

    let total_frames = total_frames as f64;
    let start = ((slice.start_ms / SOUND_ATLAS_DURATION_MS) * total_frames)
        .round()
        .clamp(0.0, total_frames) as usize;
    let end = (((slice.start_ms + slice.duration_ms) / SOUND_ATLAS_DURATION_MS) * total_frames)
        .round()
        .clamp(start as f64 + 1.0, total_frames) as usize;
    start..end
}

impl SoundSlice {
    const fn new(start_ms: f64, duration_ms: f64) -> Self {
        Self {
            start_ms,
            duration_ms,
        }
    }
}

fn sync_spatial_listener(
    mut commands: Commands,
    config: Res<GameConfig>,
    players: Query<(&Player, &RoomId, Has<Controlled>)>,
    controlled_snakes: Query<(&SnakeHead, &RoomId), (With<Controlled>, With<TailPoints>)>,
    heads: Query<&SnakeHead>,
    mut listeners: Query<(Entity, &mut Transform), (With<SoundListener>, With<SpatialListener2D>)>,
) {
    if !config.sound.enabled || !config.sound.spatial_audio {
        for (entity, _) in &mut listeners {
            commands.entity(entity).despawn();
        }
        return;
    }

    let Some(listener) = controlled_listener_snapshot(&controlled_snakes)
        .or_else(|| listener_snapshot(&players, &heads))
    else {
        return;
    };
    let transform = Transform::from_translation(listener.position.extend(0.0));

    if let Some((_, mut existing_transform)) = listeners.iter_mut().next() {
        *existing_transform = transform;
    } else {
        commands.spawn((
            SoundListener,
            SpatialListener2D,
            transform,
            GlobalTransform::default(),
        ));
    }
}

fn play_turn_sounds(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    mut state: ResMut<TurnSoundState>,
    mut budget: ResMut<OneShotSoundBudget>,
    players: Query<(Entity, &Player, &RoomId, &PlayerStatus, Has<Controlled>)>,
    local_snakes: Query<(Entity, &SnakeHead, &RoomId), (With<Controlled>, With<TailPoints>)>,
    remote_snakes: Query<
        (Entity, &SnakeHead, &RoomId),
        (
            With<TailPoints>,
            Without<Controlled>,
            Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
        ),
    >,
    heads: Query<&SnakeHead>,
) {
    if !config.sound.enabled {
        state.directions.clear();
        return;
    }

    let listener = local_snakes
        .iter()
        .next()
        .map(|(_, head, room)| ListenerSnapshot {
            position: head.position,
            room: *room,
        })
        .or_else(|| remote_listener_snapshot(&players, &heads));
    let mut seen = HashSet::new();

    if let Some((snake, head, _)) = local_snakes.iter().next() {
        seen.insert(snake);
        if direction_changed(&mut state.directions, snake, head.direction) {
            if let Some(turn_sound) = sounds.turn() {
                let volume = config.sound.master_volume * config.sound.turn_volume;
                spawn_one_shot(
                    &mut commands,
                    turn_sound,
                    None,
                    volume,
                    &config.sound,
                    &mut budget,
                );
            }
        }
    }

    let Some(listener) = listener else {
        state.directions.retain(|entity, _| seen.contains(entity));
        return;
    };

    for (snake, head, room) in &remote_snakes {
        if *room != listener.room {
            continue;
        }

        seen.insert(snake);
        if !direction_changed(&mut state.directions, snake, head.direction) {
            continue;
        }

        let volume = remote_event_volume(
            head.position,
            listener,
            config.sound.master_volume * config.sound.turn_volume,
            config.sound.remote_turn_volume,
            &config.sound,
        );
        if let Some(turn_sound) = sounds.turn() {
            spawn_one_shot(
                &mut commands,
                turn_sound,
                Some(head.position),
                volume,
                &config.sound,
                &mut budget,
            );
        }
    }

    state.directions.retain(|entity, _| seen.contains(entity));
}

fn play_proximity_boost_sounds(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    mut state: ResMut<ProximityBoostSoundState>,
    mut budget: ResMut<OneShotSoundBudget>,
    players: Query<(Entity, &Player, &RoomId, &PlayerStatus, Has<Controlled>)>,
    local_snakes: Query<(Entity, &Acceleration, &FoodBoost), (With<Controlled>, With<TailPoints>)>,
    controlled_heads: Query<(&SnakeHead, &RoomId), (With<Controlled>, With<TailPoints>)>,
    remote_snakes: Query<
        (Entity, &SnakeHead, &RoomId, &Acceleration, &FoodBoost),
        (
            With<TailPoints>,
            Without<Controlled>,
            Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
        ),
    >,
    heads: Query<&SnakeHead>,
) {
    if !config.sound.enabled {
        state.active.clear();
        return;
    }

    let listener = controlled_listener_snapshot(&controlled_heads)
        .or_else(|| remote_listener_snapshot(&players, &heads));
    let mut active_now = HashSet::new();

    if let Some((snake, acceleration, food_boost)) = local_snakes.iter().next() {
        if proximity_boost_active(acceleration.0, food_boost.0, &config.movement) {
            active_now.insert(snake);
            if !state.active.contains(&snake) {
                if let Some(spark_sound) = sounds.spark() {
                    let volume = config.sound.master_volume * config.sound.proximity_boost_volume;
                    spawn_one_shot(
                        &mut commands,
                        spark_sound,
                        None,
                        volume,
                        &config.sound,
                        &mut budget,
                    );
                }
            }
        }
    }

    if let Some(listener) = listener {
        for (snake, head, room, acceleration, food_boost) in &remote_snakes {
            if *room != listener.room {
                continue;
            }
            if !proximity_boost_active(acceleration.0, food_boost.0, &config.movement) {
                continue;
            }

            active_now.insert(snake);
            if state.active.contains(&snake) {
                continue;
            }

            let volume = remote_event_volume(
                head.position,
                listener,
                config.sound.master_volume * config.sound.proximity_boost_volume,
                config.sound.remote_proximity_boost_volume,
                &config.sound,
            );
            if let Some(spark_sound) = sounds.spark() {
                spawn_one_shot(
                    &mut commands,
                    spark_sound,
                    Some(head.position),
                    volume,
                    &config.sound,
                    &mut budget,
                );
            }
        }
    }

    state.active = active_now;
}

fn play_confirmed_death_sounds(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    mut budget: ResMut<OneShotSoundBudget>,
    players: Query<(&Player, &RoomId, Has<Controlled>)>,
    controlled_snakes: Query<(&SnakeHead, &RoomId), (With<Controlled>, With<TailPoints>)>,
    heads: Query<&SnakeHead>,
    mut deaths: MessageReader<ConfirmedDeath>,
) {
    if !config.sound.enabled {
        for _ in deaths.read() {}
        return;
    }
    let Some(crash_sound) = sounds.crash() else {
        for _ in deaths.read() {}
        return;
    };

    let listener = controlled_listener_snapshot(&controlled_snakes)
        .or_else(|| listener_snapshot(&players, &heads));
    for death in deaths.read() {
        let volume = death_sound_volume(death, listener, &config.sound);
        if volume <= 0.0 {
            continue;
        }
        let source_position = (!death.local_player).then_some(death.position).flatten();
        spawn_one_shot(
            &mut commands,
            crash_sound.clone(),
            source_position,
            volume,
            &config.sound,
            &mut budget,
        );
    }
}

fn play_confirmed_food_sounds(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    time: Res<Time>,
    mut cooldown: ResMut<FoodSoundCooldown>,
    mut budget: ResMut<OneShotSoundBudget>,
    players: Query<(&Player, &RoomId, Has<Controlled>)>,
    controlled_snakes: Query<(&SnakeHead, &RoomId), (With<Controlled>, With<TailPoints>)>,
    snakes: Query<(&SnakeHead, &RoomId)>,
    mut pickups: MessageReader<ConfirmedFoodPickup>,
) {
    if !config.sound.enabled {
        for _ in pickups.read() {}
        return;
    }
    let Some(food_grab_sound) = sounds.food_grab() else {
        for _ in pickups.read() {}
        return;
    };

    let listener = controlled_listener_snapshot(&controlled_snakes)
        .or_else(|| listener_snapshot_from_roomed_tails(&players, &snakes));
    let local_snake = local_player_snake(&players);
    for pickup in pickups.read() {
        let collision = &pickup.collision;
        if !food_sound_ready(time.elapsed_secs_f64(), &cooldown) {
            continue;
        }
        if Some(collision.snake) == local_snake {
            let volume = config.sound.master_volume * config.sound.food_volume;
            if spawn_one_shot(
                &mut commands,
                food_grab_sound.clone(),
                None,
                volume,
                &config.sound,
                &mut budget,
            ) {
                cooldown.last_played_seconds = Some(time.elapsed_secs_f64());
            }
            continue;
        }

        let Some(listener) = listener else {
            continue;
        };
        let source_position = match snakes.get(collision.snake) {
            Ok((head, room)) if *room == listener.room => head.position,
            Ok(_) => continue,
            Err(_) => collision.head_position,
        };
        let distance = source_position.distance(listener.position);
        if !within_remote_one_shot_radius(distance, &config.sound) {
            continue;
        }
        let attenuation = distance_attenuation(distance, &config.sound);
        let volume = config.sound.master_volume
            * config.sound.food_volume
            * config.sound.remote_food_volume
            * attenuation;
        if spawn_one_shot(
            &mut commands,
            food_grab_sound.clone(),
            Some(source_position),
            volume,
            &config.sound,
            &mut budget,
        ) {
            cooldown.last_played_seconds = Some(time.elapsed_secs_f64());
        }
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
    sample_effects: Query<&SampleEffects, With<LocalSpeedLoopSound>>,
    mut volume_nodes: Query<&mut VolumeNode>,
) {
    let state_snapshot = local_player_sound_state(&controlled_snakes, &players, &speeds);
    let (line_volume, fast_volume) = speed_loop_volumes(
        state_snapshot.map(|state| state.speed),
        &config.sound,
        &config.movement,
    );
    let electro_volume = local_electro_loop_volume(state_snapshot, &config.sound, &config.movement);
    let Some(loop_sounds) = sounds.speed_loops() else {
        clear_local_speed_loops(&mut commands, &mut state);
        return;
    };

    update_plain_loop(
        &mut commands,
        &mut state.line_loop,
        LocalSpeedLoopSound,
        loop_sounds.line_loop,
        line_volume,
        &sample_effects,
        &mut volume_nodes,
    );
    update_plain_loop(
        &mut commands,
        &mut state.line_fast_loop,
        LocalSpeedLoopSound,
        loop_sounds.line_fast_loop,
        fast_volume,
        &sample_effects,
        &mut volume_nodes,
    );
    update_plain_loop(
        &mut commands,
        &mut state.electro_loop,
        LocalSpeedLoopSound,
        loop_sounds.electro_loop,
        electro_volume,
        &sample_effects,
        &mut volume_nodes,
    );
}

fn update_remote_speed_loops(
    mut commands: Commands,
    config: Res<GameConfig>,
    sounds: Res<PowerlineSounds>,
    mut state: ResMut<RemoteSpeedLoopState>,
    players: Query<(Entity, &Player, &RoomId, &PlayerStatus, Has<Controlled>)>,
    controlled_snakes: Query<(&SnakeHead, &RoomId), (With<Controlled>, With<TailPoints>)>,
    remote_snakes: Query<
        (
            Entity,
            &SnakeHead,
            &RoomId,
            &Speed,
            Option<&Acceleration>,
            Option<&FoodBoost>,
        ),
        (
            With<TailPoints>,
            Without<Controlled>,
            Or<(With<Predicted>, With<Interpolated>, Without<Replicated>)>,
        ),
    >,
    heads: Query<&SnakeHead>,
    sample_effects: Query<&SampleEffects, With<RemoteSpeedLoopSound>>,
    mut transforms: Query<&mut Transform, With<RemoteSpeedLoopSound>>,
    mut volume_nodes: Query<&mut VolumeNode>,
) {
    if !config.sound.enabled || !config.sound.spatial_audio {
        clear_remote_speed_loops(&mut commands, &mut state);
        return;
    }
    let Some(loop_sounds) = sounds.speed_loops() else {
        clear_remote_speed_loops(&mut commands, &mut state);
        return;
    };

    let Some(listener) = controlled_listener_snapshot(&controlled_snakes)
        .or_else(|| remote_listener_snapshot(&players, &heads))
    else {
        clear_remote_speed_loops(&mut commands, &mut state);
        return;
    };

    let mut candidates = Vec::new();
    for (snake, head, room, speed, acceleration, food_boost) in &remote_snakes {
        if *room != listener.room {
            continue;
        }
        let proximity_active =
            acceleration
                .zip(food_boost)
                .is_some_and(|(acceleration, food_boost)| {
                    proximity_boost_active(acceleration.0, food_boost.0, &config.movement)
                });

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

        if line_volume <= SILENT_VOLUME_EPSILON
            && fast_volume <= SILENT_VOLUME_EPSILON
            && electro_volume <= SILENT_VOLUME_EPSILON
        {
            continue;
        }

        candidates.push(RemoteLoopCandidate {
            snake,
            position: source_position,
            line_volume,
            fast_volume,
            electro_volume,
            distance,
        });
    }

    candidates.sort_by(|left, right| left.distance.total_cmp(&right.distance));

    let mut seen = HashSet::new();
    for candidate in candidates.into_iter().take(MAX_REMOTE_SPEED_LOOP_SNAKES) {
        seen.insert(candidate.snake);
        let loops = state.loops.entry(candidate.snake).or_default();

        update_remote_speed_loop(
            &mut commands,
            &mut loops.line_loop,
            candidate.snake,
            loop_sounds.line_loop.clone(),
            candidate.position,
            candidate.line_volume,
            &config.sound,
            &sample_effects,
            &mut transforms,
            &mut volume_nodes,
        );
        update_remote_speed_loop(
            &mut commands,
            &mut loops.line_fast_loop,
            candidate.snake,
            loop_sounds.line_fast_loop.clone(),
            candidate.position,
            candidate.fast_volume,
            &config.sound,
            &sample_effects,
            &mut transforms,
            &mut volume_nodes,
        );
        update_remote_speed_loop(
            &mut commands,
            &mut loops.electro_loop,
            candidate.snake,
            loop_sounds.electro_loop.clone(),
            candidate.position,
            candidate.electro_volume,
            &config.sound,
            &sample_effects,
            &mut transforms,
            &mut volume_nodes,
        );
    }

    let stale_players = state
        .loops
        .keys()
        .copied()
        .filter(|snake| !seen.contains(snake))
        .collect::<Vec<_>>();
    for snake in stale_players {
        if let Some(loops) = state.loops.remove(&snake) {
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
    sound: Handle<AudioSample>,
    volume: f32,
    sample_effects: &Query<&SampleEffects, With<C>>,
    volume_nodes: &mut Query<&mut VolumeNode>,
) {
    if volume <= SILENT_VOLUME_EPSILON {
        if let Some(existing) = entity.take() {
            despawn_loop(commands, Some(existing));
        }
        return;
    }

    if let Some(existing) = *entity {
        if let Ok(effects) = sample_effects.get(existing) {
            if let Ok(mut volume_node) = volume_nodes.get_effect_mut(effects) {
                set_volume_if_changed(&mut volume_node, volume);
                return;
            }
        }
        if commands.get_entity(existing).is_ok() {
            commands.entity(existing).despawn();
        }
        *entity = None;
    }

    *entity = Some(
        commands
            .spawn((
                marker,
                SamplePlayer::new(sound).looping(),
                sample_effects![VolumeNode {
                    volume: seedling_volume(volume),
                    ..default()
                }],
            ))
            .id(),
    );
}

fn update_remote_speed_loop(
    commands: &mut Commands,
    entity: &mut Option<Entity>,
    snake: Entity,
    sound: Handle<AudioSample>,
    position: Vec2,
    volume: f32,
    sound_config: &SoundConfig,
    sample_effects: &Query<&SampleEffects, With<RemoteSpeedLoopSound>>,
    transforms: &mut Query<&mut Transform, With<RemoteSpeedLoopSound>>,
    volume_nodes: &mut Query<&mut VolumeNode>,
) {
    if volume <= SILENT_VOLUME_EPSILON || !sound_config.spatial_audio {
        if let Some(existing) = entity.take() {
            despawn_loop(commands, Some(existing));
        }
        return;
    }

    if let Some(existing) = *entity {
        if let Ok(mut transform) = transforms.get_mut(existing) {
            let translation = position.extend(0.0);
            if transform.translation.distance_squared(translation)
                >= REMOTE_LOOP_POSITION_UPDATE_DISTANCE * REMOTE_LOOP_POSITION_UPDATE_DISTANCE
            {
                transform.translation = translation;
            }
        }
        if let Ok(effects) = sample_effects.get(existing) {
            if let Ok(mut volume_node) = volume_nodes.get_effect_mut(effects) {
                set_volume_if_changed(&mut volume_node, volume);
                return;
            }
        }
        if commands.get_entity(existing).is_ok() {
            commands.entity(existing).despawn();
        }
        *entity = None;
    }

    *entity = Some(
        commands
            .spawn((
                RemoteSpeedLoopSound { snake },
                Transform::from_translation(position.extend(0.0)),
                GlobalTransform::default(),
                SamplePlayer::new(sound).looping(),
                sample_effects![
                    (
                        SpatialBasicNode::default(),
                        seedling_spatial_scale(sound_config)
                    ),
                    VolumeNode {
                        volume: seedling_volume(volume),
                        ..default()
                    }
                ],
            ))
            .id(),
    );
}

fn spawn_one_shot(
    commands: &mut Commands,
    sound: Handle<AudioSample>,
    source_position: Option<Vec2>,
    volume: f32,
    sound_config: &SoundConfig,
    budget: &mut OneShotSoundBudget,
) -> bool {
    if volume <= SILENT_VOLUME_EPSILON
        || budget.spawned_this_frame >= MAX_ONE_SHOT_SOUNDS_PER_FRAME
        || budget.tokens < 1.0
    {
        return false;
    }
    budget.spawned_this_frame += 1;
    budget.tokens -= 1.0;

    let player = SamplePlayer::new(sound).with_volume(seedling_volume(volume));
    if let Some(source_position) = source_position.filter(|_| sound_config.spatial_audio) {
        commands.spawn((
            player,
            Transform::from_translation(source_position.extend(0.0)),
            GlobalTransform::default(),
            sample_effects![(
                SpatialBasicNode::default(),
                seedling_spatial_scale(sound_config)
            )],
        ));
    } else {
        commands.spawn(player);
    }
    true
}

fn food_sound_ready(now_seconds: f64, cooldown: &FoodSoundCooldown) -> bool {
    cooldown
        .last_played_seconds
        .is_none_or(|last| now_seconds - last + 1e-9 >= FOOD_SOUND_COOLDOWN_SECONDS)
}

fn seedling_volume(volume: f32) -> Volume {
    let volume = if volume.is_finite() {
        volume.max(0.0)
    } else {
        0.0
    };
    Volume::Linear(volume)
}

fn set_volume_if_changed(volume_node: &mut VolumeNode, volume: f32) {
    let volume = seedling_volume(volume);
    if (volume_node.volume.linear() - volume.linear()).abs() >= VOLUME_UPDATE_EPSILON {
        volume_node.volume = volume;
    }
}

fn seedling_spatial_scale(sound_config: &SoundConfig) -> SpatialScale {
    SpatialScale(Vec3::splat(sound_config.spatial_scale.max(0.0001)))
}

fn clear_remote_speed_loops(commands: &mut Commands, state: &mut RemoteSpeedLoopState) {
    let loops = std::mem::take(&mut state.loops);
    for (_, loops) in loops {
        despawn_loop(commands, loops.line_loop);
        despawn_loop(commands, loops.line_fast_loop);
        despawn_loop(commands, loops.electro_loop);
    }
}

fn clear_local_speed_loops(commands: &mut Commands, state: &mut LocalSpeedLoopState) {
    despawn_loop(commands, state.line_loop.take());
    despawn_loop(commands, state.line_fast_loop.take());
    despawn_loop(commands, state.electro_loop.take());
}

fn despawn_loop(commands: &mut Commands, entity: Option<Entity>) {
    if let Some(entity) = entity {
        if commands.get_entity(entity).is_ok() {
            commands.entity(entity).despawn();
        }
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

fn controlled_listener_snapshot(
    controlled_snakes: &Query<(&SnakeHead, &RoomId), (With<Controlled>, With<TailPoints>)>,
) -> Option<ListenerSnapshot> {
    controlled_snakes
        .iter()
        .next()
        .map(|(head, room)| ListenerSnapshot {
            position: head.position,
            room: *room,
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

fn local_electro_loop_volume(
    state: Option<LocalSoundState>,
    sound: &SoundConfig,
    movement: &MovementConfig,
) -> f32 {
    if !sound.enabled {
        return 0.0;
    }
    let Some(state) = state else {
        return 0.0;
    };
    let base = sound.master_volume * sound.electro_loop_volume * LOCAL_ELECTRO_BASE_VOLUME_RATIO;
    if proximity_boost_active(state.acceleration, state.food_boost, movement) {
        sound.master_volume * sound.electro_loop_volume
    } else {
        base
    }
}

fn direction_changed(
    directions: &mut HashMap<Entity, Direction>,
    entity: Entity,
    direction: Direction,
) -> bool {
    let changed = directions
        .insert(entity, direction)
        .is_some_and(|previous| previous != direction);
    changed
}

fn remote_event_volume(
    source_position: Vec2,
    listener: ListenerSnapshot,
    base_volume: f32,
    remote_multiplier: f32,
    sound: &SoundConfig,
) -> f32 {
    let distance = source_position.distance(listener.position);
    if !within_remote_one_shot_radius(distance, sound) {
        return 0.0;
    }

    base_volume * remote_multiplier * distance_attenuation(distance, sound)
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

    let distance = source_position.distance(listener.position);
    if !within_remote_one_shot_radius(distance, sound) {
        return 0.0;
    }

    base * sound.remote_death_volume * distance_attenuation(distance, sound)
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

fn within_remote_one_shot_radius(distance: f32, sound: &SoundConfig) -> bool {
    let max_distance = sound
        .remote_one_shot_max_distance
        .max(sound.remote_sound_full_volume_distance);
    distance <= max_distance
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
    fn local_electro_loop_has_baseline_volume_for_local_snake() {
        let sound = SoundConfig::default();
        let movement = MovementConfig::default();

        let volume = local_electro_loop_volume(
            Some(LocalSoundState {
                speed: movement.min_speed,
                acceleration: movement.base_acceleration,
                food_boost: 0.0,
            }),
            &sound,
            &movement,
        );

        assert!(volume > 0.0);
        assert!(volume < sound.master_volume * sound.electro_loop_volume);
    }

    #[test]
    fn food_sound_cooldown_blocks_rapid_replays() {
        let cooldown = FoodSoundCooldown {
            last_played_seconds: Some(1.0),
        };

        assert!(!food_sound_ready(
            1.0 + FOOD_SOUND_COOLDOWN_SECONDS * 0.5,
            &cooldown
        ));
        assert!(food_sound_ready(
            1.0 + FOOD_SOUND_COOLDOWN_SECONDS,
            &cooldown
        ));
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

    #[test]
    fn direction_change_only_triggers_after_first_observed_direction() {
        let entity = Entity::from_bits(7);
        let mut directions = HashMap::new();

        assert!(!direction_changed(
            &mut directions,
            entity,
            Direction::Right
        ));
        assert!(!direction_changed(
            &mut directions,
            entity,
            Direction::Right
        ));
        assert!(direction_changed(&mut directions, entity, Direction::Up));
    }

    #[test]
    fn remote_event_volume_uses_distance_attenuation() {
        let sound = SoundConfig::default();
        let listener = ListenerSnapshot {
            position: Vec2::ZERO,
            room: RoomId(1),
        };
        let base_volume = 0.5;

        let nearby = remote_event_volume(
            Vec2::new(sound.remote_sound_full_volume_distance * 0.5, 0.0),
            listener,
            base_volume,
            sound.remote_turn_volume,
            &sound,
        );
        let far = remote_event_volume(
            Vec2::new(sound.remote_sound_max_distance, 0.0),
            listener,
            base_volume,
            sound.remote_turn_volume,
            &sound,
        );

        assert_eq!(nearby, base_volume * sound.remote_turn_volume);
        assert_eq!(far, 0.0);
    }

    #[test]
    fn remote_event_volume_is_silent_outside_one_shot_radius() {
        let sound = SoundConfig {
            remote_one_shot_max_distance: 120.0,
            remote_sound_max_distance: 360.0,
            ..default()
        };
        let listener = ListenerSnapshot {
            position: Vec2::ZERO,
            room: RoomId(1),
        };

        assert_eq!(
            remote_event_volume(
                Vec2::new(180.0, 0.0),
                listener,
                0.5,
                sound.remote_turn_volume,
                &sound,
            ),
            0.0
        );
    }

    #[test]
    fn sound_sprite_ranges_match_original_howler_atlas() {
        let total_frames = 44_100 * 18;

        assert_eq!(
            sound_slice_frame_range(total_frames, TURN_SLICE),
            705_600..727_650
        );
        assert_eq!(
            sound_slice_frame_range(total_frames, LINE_FAST_LOOP_SLICE),
            485_100..573_300
        );
    }

    #[test]
    fn sound_sprite_ranges_are_non_empty_and_inside_atlas() {
        let total_frames = 44_100 * 18;
        for slice in [
            CRASH_SLICE,
            ELECTRO_LOOP_SLICE,
            FOOD_GRAB_SLICE,
            LINE_LOOP_SLICE,
            LINE_FAST_LOOP_SLICE,
            SPARK_SLICE,
            TURN_SLICE,
        ] {
            let range = sound_slice_frame_range(total_frames, slice);
            assert!(range.start < range.end);
            assert!(range.end <= total_frames as usize);
        }
    }
}
