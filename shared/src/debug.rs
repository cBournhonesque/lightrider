use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::log::{BoxedFmtLayer, BoxedLayer, Level, LogPlugin};
use bevy::prelude::*;
use bevy::time::Real;
use lightyear::frame_interpolation::FrameInterpolationSystems;
use lightyear::prelude::{
    Controlled, ControlledBy, Interpolated, Link, LocalTimeline, Predicted, Replicated, Tick,
};
use serde_json::{Map, Number, Value};
use tracing::field::{Field, Visit};
use tracing::{enabled, warn, Event, Level as TraceLevel, Subscriber};
use tracing_subscriber::filter::FilterFn;
use tracing_subscriber::layer::Context;
use tracing_subscriber::Layer;

use crate::bot::BotMarker;
use crate::config::GameConfig;
use crate::movement::SimulationSet;
use crate::network::protocol::prelude::*;
use crate::utils::query::SimulationAuthority;

pub const LIGHTYEAR_DEBUG_FILE_ENV: &str = "LIGHTYEAR_DEBUG_FILE";
const LIGHTYEAR_DEBUG_TARGET: &str = "lightyear_debug";
const LIGHTYEAR_DEBUG_TARGET_MANUAL: &str = "lightyear_debug::manual";
static DEBUG_FRAME_INDEX: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DebugSamplePoint {
    FixedUpdate,
    FixedLast,
    PostUpdate,
    Last,
}

impl DebugSamplePoint {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixedUpdate => "FixedUpdate",
            Self::FixedLast => "FixedLast",
            Self::PostUpdate => "PostUpdate",
            Self::Last => "Last",
        }
    }
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeDebugRole {
    Client,
    Server,
}

impl RuntimeDebugRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Client => "client",
            Self::Server => "server",
        }
    }
}

pub struct RuntimeDebugPlugin {
    role: RuntimeDebugRole,
}

impl RuntimeDebugPlugin {
    pub const fn client() -> Self {
        Self {
            role: RuntimeDebugRole::Client,
        }
    }

    pub const fn server() -> Self {
        Self {
            role: RuntimeDebugRole::Server,
        }
    }
}

impl Plugin for RuntimeDebugPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.role);
        app.add_systems(First, advance_debug_frame_index);
        app.add_systems(
            FixedUpdate,
            trace_snakes_fixed_update.after(SimulationSet::Movement),
        );
        app.add_systems(
            FixedLast,
            (trace_snakes_fixed_last, validate_snakes_fixed_last),
        );
        app.add_systems(
            PostUpdate,
            trace_snakes_post_update.after(FrameInterpolationSystems::Interpolate),
        );
        app.add_systems(Last, (trace_snakes_last, trace_perf_metrics));
    }
}

pub fn runtime_log_plugin(config: &GameConfig, base_filter: &str) -> LogPlugin {
    let capture_lightyear_debug =
        config.debug.lightyear_debug && std::env::var_os(LIGHTYEAR_DEBUG_FILE_ENV).is_some();
    let mut plugin = if capture_lightyear_debug {
        debug_log_plugin()
    } else {
        LogPlugin::default()
    };
    plugin.level = if capture_lightyear_debug {
        Level::TRACE
    } else {
        Level::INFO
    };
    plugin.filter = if capture_lightyear_debug {
        format!("{base_filter},lightyear_debug=trace")
    } else {
        base_filter.to_string()
    };
    plugin
}

struct DebugJsonLayer {
    writer: Mutex<BufWriter<File>>,
}

impl DebugJsonLayer {
    fn file(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            writer: Mutex::new(BufWriter::new(file)),
        })
    }

    fn from_env() -> io::Result<Self> {
        let path = std::env::var_os(LIGHTYEAR_DEBUG_FILE_ENV).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{LIGHTYEAR_DEBUG_FILE_ENV} is not set"),
            )
        })?;
        Self::file(path)
    }

    fn write_event(&self, event: &Event<'_>) {
        let metadata = event.metadata();
        if !is_lightyear_debug_target(metadata.target()) {
            return;
        }

        let mut visitor = JsonFieldVisitor::default();
        event.record(&mut visitor);

        let mut root = Map::new();
        root.insert("timestamp".to_string(), Value::from(unix_timestamp_ns()));
        root.insert("process_id".to_string(), Value::from(std::process::id()));
        root.insert(
            "frame_index".to_string(),
            Value::from(DEBUG_FRAME_INDEX.load(Ordering::Relaxed)),
        );
        root.insert("target".to_string(), Value::from(metadata.target()));
        root.insert("level".to_string(), Value::from(metadata.level().as_str()));
        if let Some(category) = category_from_target(metadata.target()) {
            root.insert("category".to_string(), Value::from(category));
        }
        for field in PROMOTED_FIELDS {
            if let Some(value) = visitor.fields.remove(*field) {
                root.insert((*field).to_string(), value);
            }
        }
        root.insert("fields".to_string(), Value::Object(visitor.fields));

        let Ok(mut writer) = self.writer.lock() else {
            return;
        };
        if serde_json::to_writer(&mut *writer, &Value::Object(root)).is_ok() {
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
    }
}

impl Drop for DebugJsonLayer {
    fn drop(&mut self) {
        if let Ok(writer) = self.writer.get_mut() {
            let _ = writer.flush();
        }
    }
}

impl<S> Layer<S> for DebugJsonLayer
where
    S: Subscriber,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        self.write_event(event);
    }
}

fn debug_custom_layer(_: &mut App) -> Option<BoxedLayer> {
    match DebugJsonLayer::from_env() {
        Ok(layer) => Some(Box::new(layer)),
        Err(error) => {
            eprintln!("failed to initialize lightrider debug layer: {error}");
            None
        }
    }
}

fn non_debug_fmt_layer(_: &mut App) -> Option<BoxedFmtLayer> {
    Some(Box::new(
        tracing_subscriber::fmt::Layer::default()
            .with_writer(io::stderr)
            .with_filter(FilterFn::new(|metadata| {
                !is_lightyear_debug_target(metadata.target())
            })),
    ))
}

fn debug_log_plugin() -> LogPlugin {
    LogPlugin {
        custom_layer: debug_custom_layer,
        fmt_layer: non_debug_fmt_layer,
        ..Default::default()
    }
}

const PROMOTED_FIELDS: &[&str] = &[
    "category",
    "entity",
    "kind",
    "role",
    "sample_point",
    "schedule",
    "tick",
    "tick_id",
];

#[derive(Default)]
struct JsonFieldVisitor {
    fields: Map<String, Value>,
}

impl JsonFieldVisitor {
    fn insert(&mut self, field: &Field, value: Value) {
        self.fields.insert(field.name().to_string(), value);
    }
}

impl Visit for JsonFieldVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        let value = Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or(Value::Null);
        self.insert(field, value);
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.insert(field, Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.insert(field, Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.insert(field, Value::from(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.insert(field, Value::from(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.insert(field, Value::from(format!("{value:?}")));
    }
}

fn unix_timestamp_ns() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

fn is_lightyear_debug_target(target: &str) -> bool {
    target == LIGHTYEAR_DEBUG_TARGET
        || target
            .strip_prefix(LIGHTYEAR_DEBUG_TARGET)
            .is_some_and(|suffix| suffix.starts_with("::"))
}

fn category_from_target(target: &str) -> Option<&str> {
    target
        .strip_prefix(LIGHTYEAR_DEBUG_TARGET)?
        .strip_prefix("::")?
        .split("::")
        .next()
        .filter(|category| !category.is_empty())
}

fn trace_snakes_fixed_update(
    config: Res<GameConfig>,
    role: Res<RuntimeDebugRole>,
    timeline: Res<LocalTimeline>,
    snakes: Query<SnakeTraceItem>,
    players: Query<&Player>,
) {
    trace_snake_sample(
        &config,
        *role,
        &timeline,
        DebugSamplePoint::FixedUpdate,
        "FixedUpdate",
        snakes,
        players,
    );
}

fn trace_snakes_fixed_last(
    config: Res<GameConfig>,
    role: Res<RuntimeDebugRole>,
    timeline: Res<LocalTimeline>,
    snakes: Query<SnakeTraceItem>,
    players: Query<&Player>,
) {
    trace_snake_sample(
        &config,
        *role,
        &timeline,
        DebugSamplePoint::FixedLast,
        "FixedLast",
        snakes,
        players,
    );
}

fn trace_snakes_post_update(
    config: Res<GameConfig>,
    role: Res<RuntimeDebugRole>,
    timeline: Res<LocalTimeline>,
    snakes: Query<SnakeTraceItem>,
    players: Query<&Player>,
    mut last_sampled_tick: Local<Option<u32>>,
) {
    if already_sampled_frame_tick(&timeline, &mut last_sampled_tick) {
        return;
    }
    trace_snake_sample(
        &config,
        *role,
        &timeline,
        DebugSamplePoint::PostUpdate,
        "PostUpdate",
        snakes,
        players,
    );
}

fn trace_snakes_last(
    config: Res<GameConfig>,
    role: Res<RuntimeDebugRole>,
    timeline: Res<LocalTimeline>,
    snakes: Query<SnakeTraceItem>,
    players: Query<&Player>,
    mut last_sampled_tick: Local<Option<u32>>,
) {
    if already_sampled_frame_tick(&timeline, &mut last_sampled_tick) {
        return;
    }
    trace_snake_sample(
        &config,
        *role,
        &timeline,
        DebugSamplePoint::Last,
        "Last",
        snakes,
        players,
    );
}

fn advance_debug_frame_index() {
    DEBUG_FRAME_INDEX.fetch_add(1, Ordering::Relaxed);
}

#[derive(Default)]
struct PerfMetricWindow {
    frame_count: u64,
    elapsed_seconds: f64,
    frame_delta_total_ms: f64,
    frame_delta_max_ms: f64,
}

impl PerfMetricWindow {
    fn push_frame(&mut self, delta_seconds: f64) {
        let delta_ms = delta_seconds * 1000.0;
        self.frame_count += 1;
        self.elapsed_seconds += delta_seconds;
        self.frame_delta_total_ms += delta_ms;
        self.frame_delta_max_ms = self.frame_delta_max_ms.max(delta_ms);
    }

    fn should_flush(&self) -> bool {
        self.elapsed_seconds >= 1.0
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

fn trace_perf_metrics(
    role: Res<RuntimeDebugRole>,
    timeline: Res<LocalTimeline>,
    real_time: Res<Time<Real>>,
    links: Query<&Link>,
    mut window: Local<PerfMetricWindow>,
) {
    if !manual_trace_enabled() {
        return;
    }

    window.push_frame(real_time.delta_secs_f64());
    if !window.should_flush() {
        return;
    }

    let mut link_count = 0_u64;
    let mut rtt_total_ms = 0.0_f64;
    let mut rtt_max_ms = 0.0_f64;
    let mut jitter_total_ms = 0.0_f64;
    let mut jitter_max_ms = 0.0_f64;
    let mut recv_buffered = 0_u64;
    let mut send_buffered = 0_u64;
    for link in &links {
        link_count += 1;
        let rtt_ms = link.stats.rtt.as_secs_f64() * 1000.0;
        let jitter_ms = link.stats.jitter.as_secs_f64() * 1000.0;
        rtt_total_ms += rtt_ms;
        rtt_max_ms = rtt_max_ms.max(rtt_ms);
        jitter_total_ms += jitter_ms;
        jitter_max_ms = jitter_max_ms.max(jitter_ms);
        recv_buffered += link.recv.len() as u64;
        send_buffered += link.send.len() as u64;
    }

    let frame_delta_avg_ms = if window.frame_count > 0 {
        window.frame_delta_total_ms / window.frame_count as f64
    } else {
        0.0
    };
    let fps = if window.elapsed_seconds > 0.0 {
        window.frame_count as f64 / window.elapsed_seconds
    } else {
        0.0
    };
    let link_rtt_avg_ms = if link_count > 0 {
        rtt_total_ms / link_count as f64
    } else {
        0.0
    };
    let link_jitter_avg_ms = if link_count > 0 {
        jitter_total_ms / link_count as f64
    } else {
        0.0
    };
    let tick = timeline.tick();

    tracing::trace!(
        target: LIGHTYEAR_DEBUG_TARGET_MANUAL,
        kind = "perf_frame",
        sample_point = DebugSamplePoint::Last.as_str(),
        schedule = "Last",
        role = role.as_str(),
        tick = ?tick,
        tick_id = u64::from(tick.0),
        frame_count = window.frame_count,
        elapsed_seconds = window.elapsed_seconds,
        fps = fps,
        frame_delta_avg_ms = frame_delta_avg_ms,
        frame_delta_max_ms = window.frame_delta_max_ms,
        link_count = link_count,
        link_rtt_avg_ms = link_rtt_avg_ms,
        link_rtt_max_ms = rtt_max_ms,
        link_jitter_avg_ms = link_jitter_avg_ms,
        link_jitter_max_ms = jitter_max_ms,
        link_recv_buffered = recv_buffered,
        link_send_buffered = send_buffered,
        "lightrider performance sample"
    );
    window.reset();
}

type SnakeTraceItem<'a> = (
    Entity,
    &'a TailPoints,
    Option<&'a TailLength>,
    Option<&'a Speed>,
    Option<&'a Acceleration>,
    Option<&'a RoomId>,
    Option<&'a HasPlayer>,
    Has<Predicted>,
    Has<Interpolated>,
    Has<Replicated>,
    Has<SimulationAuthority>,
    Has<Controlled>,
    Option<&'a ControlledBy>,
    Has<BotMarker>,
);

fn trace_snake_sample(
    config: &GameConfig,
    role: RuntimeDebugRole,
    timeline: &LocalTimeline,
    sample_point: DebugSamplePoint,
    schedule: &'static str,
    snakes: Query<SnakeTraceItem>,
    players: Query<&Player>,
) {
    if !config.debug.json_snapshots || !manual_trace_enabled() {
        return;
    }
    let tick = timeline.tick();
    if !should_sample_tick(config, u32::from(tick.0)) {
        return;
    }

    for (
        entity,
        tail,
        length,
        speed,
        acceleration,
        room,
        has_player,
        is_predicted,
        is_interpolated,
        is_replicated,
        has_simulation_authority,
        is_controlled,
        controlled_by,
        is_bot,
    ) in &snakes
    {
        let Some((head, direction)) = tail.0.front() else {
            tracing::trace!(
                target: LIGHTYEAR_DEBUG_TARGET_MANUAL,
                kind = "snake_head_missing",
                sample_point = sample_point.as_str(),
                schedule = schedule,
                role = role.as_str(),
                entity = ?entity,
                tick = ?tick,
                tick_id = u64::from(tick.0),
                "snake has no head point"
            );
            continue;
        };
        let player_id_bits = has_player
            .and_then(|has_player| players.get(has_player.0).ok())
            .map(|player| player.id.to_bits())
            .unwrap_or(u64::MAX);
        let room_id = room.map(|room| room.0).unwrap_or(u64::MAX);
        let tail_length_current = length.map(|length| length.current_size).unwrap_or(f32::NAN);
        let tail_length_target = length.map(|length| length.target_size).unwrap_or(f32::NAN);
        let speed = speed.map(|speed| speed.0).unwrap_or(f32::NAN);
        let acceleration = acceleration
            .map(|acceleration| acceleration.0)
            .unwrap_or(f32::NAN);

        tracing::trace!(
            target: LIGHTYEAR_DEBUG_TARGET_MANUAL,
            kind = "snake_head",
            sample_point = sample_point.as_str(),
            schedule = schedule,
            role = role.as_str(),
            entity = ?entity,
            tick = ?tick,
            tick_id = u64::from(tick.0),
            room_id = room_id,
            has_room = room.is_some(),
            player_id_bits = player_id_bits,
            has_player = has_player.is_some(),
            player_entity = ?has_player.map(|has_player| has_player.0),
            controlled_by_owner = ?controlled_by.map(|controlled_by| controlled_by.owner),
            head_x = head.x,
            head_y = head.y,
            direction = ?direction,
            tail_points = tail.0.len() as u64,
            tail_total_length = tail.total_length(),
            tail_length_current = tail_length_current,
            tail_length_target = tail_length_target,
            speed = speed,
            acceleration = acceleration,
            is_predicted = is_predicted,
            is_interpolated = is_interpolated,
            is_replicated = is_replicated,
            has_simulation_authority = has_simulation_authority,
            is_controlled = is_controlled,
            has_controlled_by = controlled_by.is_some(),
            is_bot = is_bot,
            "snake head debug sample"
        );
    }
}

fn validate_snakes_fixed_last(
    config: Res<GameConfig>,
    role: Res<RuntimeDebugRole>,
    timeline: Res<LocalTimeline>,
    snakes: Query<(
        Entity,
        &TailPoints,
        Option<&TailLength>,
        Option<&Speed>,
        Option<&RoomId>,
        Has<Interpolated>,
    )>,
) {
    if !config.debug.invariant_checks {
        return;
    }
    let tick = timeline.tick();
    let half_width = config.arena.width * 0.5;
    let half_height = config.arena.height * 0.5;

    for (entity, tail, length, speed, room, is_interpolated) in &snakes {
        let Some((head, _direction)) = tail.0.front() else {
            emit_invariant_violation(
                *role,
                entity,
                room,
                tick,
                "empty_tail",
                "snake tail has no head point",
            );
            continue;
        };

        if head.x < -half_width
            || head.x > half_width
            || head.y < -half_height
            || head.y > half_height
        {
            emit_invariant_violation(
                *role,
                entity,
                room,
                tick,
                "head_outside_arena",
                "snake head is outside configured arena bounds",
            );
        }

        if !is_interpolated {
            if let Some(speed) = speed {
                let tolerance = 0.001;
                if speed.0 < config.movement.min_speed - tolerance
                    || speed.0 > config.movement.max_speed + tolerance
                {
                    emit_invariant_violation(
                        *role,
                        entity,
                        room,
                        tick,
                        "speed_outside_config",
                        "snake speed is outside configured movement clamps",
                    );
                }
            }
            if let Some(length) = length {
                let length_delta = (tail.total_length() - length.current_size).abs();
                if length_delta > 1.0 {
                    emit_invariant_violation(
                        *role,
                        entity,
                        room,
                        tick,
                        "tail_length_mismatch",
                        "tail polyline length differs from TailLength current_size",
                    );
                }
            }
        }
    }
}

fn emit_invariant_violation(
    role: RuntimeDebugRole,
    entity: Entity,
    room: Option<&RoomId>,
    tick: Tick,
    invariant: &'static str,
    detail: &'static str,
) {
    warn!(
        target: "lightrider::debug",
        role = role.as_str(),
        ?entity,
        tick_id = u64::from(tick.0),
        room_id = room.map(|room| room.0).unwrap_or(u64::MAX),
        invariant = invariant,
        detail = detail,
        "snake invariant violation"
    );
    if manual_trace_enabled() {
        tracing::trace!(
            target: LIGHTYEAR_DEBUG_TARGET_MANUAL,
            kind = "snake_invariant_violation",
            sample_point = DebugSamplePoint::FixedLast.as_str(),
            schedule = "FixedLast",
            role = role.as_str(),
            entity = ?entity,
            tick = ?tick,
            tick_id = u64::from(tick.0),
            room_id = room.map(|room| room.0).unwrap_or(u64::MAX),
            has_room = room.is_some(),
            invariant = invariant,
            detail = detail,
            "snake invariant violation"
        );
    }
}

fn should_sample_tick(config: &GameConfig, tick: u32) -> bool {
    let interval = config.debug.snake_trace_sample_interval_ticks.max(1);
    tick % interval == 0
}

fn already_sampled_frame_tick(
    timeline: &LocalTimeline,
    last_sampled_tick: &mut Local<Option<u32>>,
) -> bool {
    let tick = timeline.tick().0;
    if last_sampled_tick.is_some_and(|last_tick| last_tick == tick) {
        return true;
    }
    **last_sampled_tick = Some(tick);
    false
}

fn manual_trace_enabled() -> bool {
    enabled!(target: LIGHTYEAR_DEBUG_TARGET_MANUAL, TraceLevel::TRACE)
}
