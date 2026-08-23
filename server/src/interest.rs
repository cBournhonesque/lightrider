use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::prelude::*;
use bevy_replicon::server::visibility::{
    client_visibility::ClientVisibility, filters_mask::FilterBit, registry::FilterRegistry,
};
use bevy_replicon::shared::replication::registry::ReplicationRegistry;
use lightyear::connection::client::Connected;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{
    LocalTimeline, MessageReceiver, PeerId, RemoteId, ReplicationSender, ReplicationSystems,
};

use crate::rooms::ClientRoom;
use shared::config::{GameConfig, NetworkInterestConfig};
use shared::network::protocol::prelude::*;
use shared::spatial::{FoodPoint, FoodSpatialIndex, TailSpatialIndex};

pub(crate) struct InterestPlugin;

#[derive(Resource, Deref)]
struct InterestVisibilityBit(FilterBit);

#[derive(Resource, Default)]
struct InterestVisibilityCache {
    visible_by_client: EntityHashMap<EntityHashSet>,
    last_update_tick: Option<u32>,
}

#[derive(Resource, Default)]
struct InterestMetrics {
    samples: u64,
    visible_food: u64,
    total_food: u64,
    visible_remote_snakes: u64,
    total_remote_snakes: u64,
    last_report_seconds: f64,
}

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
struct ClientScreenSize(UVec2);

#[derive(Clone)]
struct SnakeSnapshot {
    entity: Entity,
    player: Option<Entity>,
    room: RoomId,
    head_position: Vec2,
    tail_length: f32,
    tail: TailPolyline,
}

#[derive(Clone, Copy)]
struct PlayerSnapshot {
    entity: Entity,
    id: PeerId,
    snake: Option<Entity>,
    room: RoomId,
}

#[derive(Clone, Copy, Debug)]
struct ClientFocus {
    player: Entity,
    snake: Entity,
    position: Vec2,
    tail_length: f32,
}

impl Plugin for InterestPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InterestVisibilityBit>();
        app.init_resource::<InterestVisibilityCache>();
        app.init_resource::<InterestMetrics>();
        app.add_systems(Update, receive_client_viewport_updates);
        app.add_systems(
            PostUpdate,
            update_interest_visibility.before(ReplicationSystems::Send),
        );
    }
}

fn receive_client_viewport_updates(
    mut commands: Commands,
    config: Res<GameConfig>,
    mut clients: Query<
        (Entity, &mut MessageReceiver<ClientViewportUpdate>),
        (With<ClientOf>, With<Connected>),
    >,
) {
    for (client, mut receiver) in &mut clients {
        for update in receiver.receive() {
            let size = config
                .network
                .interest
                .clamp_screen_size(update.max_screen_width, update.max_screen_height);
            commands.entity(client).insert(ClientScreenSize(size));
        }
    }
}

impl FromWorld for InterestVisibilityBit {
    fn from_world(world: &mut World) -> Self {
        let bit = world.resource_scope(|world, mut filter_registry: Mut<FilterRegistry>| {
            world.resource_scope(|world, mut registry: Mut<ReplicationRegistry>| {
                filter_registry.register_scope::<Entity>(world, &mut registry)
            })
        });
        Self(bit)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct InterestVisibilityStats {
    visible_food: usize,
    total_food: usize,
    visible_remote_snakes: usize,
    total_remote_snakes: usize,
}

impl InterestMetrics {
    fn record(&mut self, stats: InterestVisibilityStats) {
        self.samples += 1;
        self.visible_food += stats.visible_food as u64;
        self.total_food += stats.total_food as u64;
        self.visible_remote_snakes += stats.visible_remote_snakes as u64;
        self.total_remote_snakes += stats.total_remote_snakes as u64;
    }

    fn maybe_report(&mut self, config: &NetworkInterestConfig, now_seconds: f64) {
        if now_seconds - self.last_report_seconds < f64::from(config.metrics_interval_seconds()) {
            return;
        }
        self.last_report_seconds = now_seconds;
        if self.samples == 0 {
            return;
        }

        let food_visible_percent = percent(self.visible_food, self.total_food);
        let food_visible_avg = self.visible_food as f64 / self.samples as f64;
        let food_total_avg = self.total_food as f64 / self.samples as f64;
        let remote_snakes_visible_percent =
            percent(self.visible_remote_snakes, self.total_remote_snakes);
        let remote_snakes_visible_avg = self.visible_remote_snakes as f64 / self.samples as f64;
        let remote_snakes_total_avg = self.total_remote_snakes as f64 / self.samples as f64;

        info!(
            target: "lightrider::interest",
            samples = self.samples,
            food_visible_percent,
            food_visible_avg,
            food_total_avg,
            remote_snakes_visible_percent,
            remote_snakes_visible_avg,
            remote_snakes_total_avg,
            "interest visibility sample",
        );
        trace!(
            target: "lightyear_debug::manual",
            category = "interest",
            kind = "interest_visibility_sample",
            role = "server",
            samples = self.samples,
            food_visible_percent,
            food_visible_avg,
            food_total_avg,
            remote_snakes_visible_percent,
            remote_snakes_visible_avg,
            remote_snakes_total_avg,
            "interest visibility sample",
        );
        self.samples = 0;
        self.visible_food = 0;
        self.total_food = 0;
        self.visible_remote_snakes = 0;
        self.total_remote_snakes = 0;
    }
}

fn percent(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        100.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

fn update_interest_visibility(
    config: Res<GameConfig>,
    time: Res<Time>,
    timeline: Option<Res<LocalTimeline>>,
    bit: Res<InterestVisibilityBit>,
    mut cache: ResMut<InterestVisibilityCache>,
    mut metrics: ResMut<InterestMetrics>,
    mut clients: Query<
        (
            Entity,
            &RemoteId,
            &ClientRoom,
            Option<&ClientScreenSize>,
            &mut ClientVisibility,
        ),
        (With<ClientOf>, With<Connected>, With<ReplicationSender>),
    >,
    players: Query<(Entity, &Player, &RoomId)>,
    snakes: Query<(
        Entity,
        &SnakeHead,
        &TailPoints,
        &TailLength,
        &RoomId,
        Option<&HasPlayer>,
    )>,
    food: Query<(Entity, &Position, &RoomId), With<FoodMarker>>,
) {
    if !should_update_interest(&config.network.interest, timeline.as_deref(), &mut cache) {
        return;
    }

    let player_snapshots = players
        .iter()
        .map(|(entity, player, room)| PlayerSnapshot {
            entity,
            id: player.id,
            snake: player.snake,
            room: *room,
        })
        .collect::<Vec<_>>();
    let player_by_snake = player_snapshots
        .iter()
        .filter_map(|player| player.snake.map(|snake| (snake, player.entity)))
        .collect::<EntityHashMap<_>>();
    let snake_snapshots = snakes
        .iter()
        .map(
            |(entity, head, tail, length, room, has_player)| SnakeSnapshot {
                entity,
                player: has_player
                    .map(|has_player| has_player.0)
                    .or_else(|| player_by_snake.get(&entity).copied()),
                room: *room,
                head_position: head.position,
                tail_length: length.current_size,
                tail: tail.polyline(head, length.current_size),
            },
        )
        .collect::<Vec<_>>();
    let food_points = food
        .iter()
        .map(|(entity, position, room)| FoodPoint {
            entity,
            room: *room,
            position: position.0,
        })
        .collect::<Vec<_>>();
    let food_index = FoodSpatialIndex::from_food(food_points.iter().copied());
    let tail_index = TailSpatialIndex::from_tails(
        snake_snapshots
            .iter()
            .map(|snake| (snake.entity, snake.room, &snake.tail)),
    );

    let mut active_clients = EntityHashSet::default();
    for (client, remote_id, client_room, screen_size, mut visibility) in &mut clients {
        active_clients.insert(client);
        let focus = find_client_focus(
            remote_id,
            client_room.room,
            &player_snapshots,
            &snake_snapshots,
        );
        let screen_size = screen_size
            .map(|screen_size| {
                config
                    .network
                    .interest
                    .clamp_screen_size(screen_size.0.x, screen_size.0.y)
            })
            .unwrap_or_else(|| config.network.interest.fallback_screen_size());
        let previous = cache.visible_by_client.get(&client);
        let current = visible_entities_for_client(
            client_room.room,
            focus,
            screen_size,
            previous,
            &config,
            &player_snapshots,
            &snake_snapshots,
            &food_points,
            &food_index,
            &tail_index,
        );
        metrics.record(visibility_stats_for_client(
            client_room.room,
            focus,
            &current,
            &snake_snapshots,
            &food_points,
        ));
        let mut touched = all_managed_entities_in_room(
            client_room.room,
            &player_snapshots,
            &snake_snapshots,
            &food_points,
        );
        if let Some(previous) = previous {
            touched.extend(previous.iter().copied());
        }
        for entity in touched {
            visibility.set(entity, **bit, current.contains(&entity));
        }
        cache.visible_by_client.insert(client, current);
    }
    cache
        .visible_by_client
        .retain(|client, _| active_clients.contains(client));
    metrics.maybe_report(&config.network.interest, time.elapsed_secs_f64());
}

fn should_update_interest(
    config: &NetworkInterestConfig,
    timeline: Option<&LocalTimeline>,
    cache: &mut InterestVisibilityCache,
) -> bool {
    let Some(timeline) = timeline else {
        return true;
    };
    let tick = timeline.tick().0;
    let should_update = cache
        .last_update_tick
        .map(|previous| tick.saturating_sub(previous) >= config.update_interval_ticks())
        .unwrap_or(true);
    if should_update {
        cache.last_update_tick = Some(tick);
    }
    should_update
}

fn find_client_focus(
    remote_id: &RemoteId,
    room: RoomId,
    players: &[PlayerSnapshot],
    snakes: &[SnakeSnapshot],
) -> Option<ClientFocus> {
    let player = players
        .iter()
        .find(|player| player.room == room && player.id == remote_id.0)?;
    let snake = player.snake?;
    let snake = snakes
        .iter()
        .find(|snapshot| snapshot.entity == snake && snapshot.room == room)?;
    Some(ClientFocus {
        player: player.entity,
        snake: snake.entity,
        position: snake.head_position,
        tail_length: snake.tail_length,
    })
}

fn visible_entities_for_client(
    room: RoomId,
    focus: Option<ClientFocus>,
    screen_size: UVec2,
    previous_visible: Option<&EntityHashSet>,
    config: &GameConfig,
    players: &[PlayerSnapshot],
    snakes: &[SnakeSnapshot],
    food: &[FoodPoint],
    food_index: &FoodSpatialIndex,
    tail_index: &TailSpatialIndex,
) -> EntityHashSet {
    let interest = &config.network.interest;
    if !interest.enabled || focus.is_none() {
        return all_managed_entities_in_room(room, players, snakes, food);
    }

    let focus = focus.unwrap();
    let camera_scale = config.normal_camera_scale_for_tail_length(focus.tail_length);
    let enter_region = InterestRegion::new(
        focus.position,
        interest.enter_half_extents(screen_size, camera_scale),
    );
    let leave_region = InterestRegion::new(
        focus.position,
        interest.leave_half_extents(screen_size, camera_scale),
    );
    let mut visible = EntityHashSet::default();
    visible.insert(focus.player);
    visible.insert(focus.snake);

    for food in food_index.within_aabb(
        room,
        leave_region.min_x(),
        leave_region.max_x(),
        leave_region.min_y(),
        leave_region.max_y(),
    ) {
        let region = entity_region(food.entity, previous_visible, enter_region, leave_region);
        if region.contains_point(food.position) {
            visible.insert(food.entity);
        }
    }

    let tail_candidates = tail_index
        .segments_intersecting_aabb(
            room,
            leave_region.min_x(),
            leave_region.max_x(),
            leave_region.min_y(),
            leave_region.max_y(),
        )
        .into_iter()
        .map(|segment| segment.owner)
        .collect::<EntityHashSet>();

    for snake in snakes.iter().filter(|snake| snake.room == room) {
        if snake.entity == focus.snake {
            insert_snake_actor(&mut visible, snake);
            continue;
        }

        let region = entity_region(snake.entity, previous_visible, enter_region, leave_region);
        let head_visible = region.contains_point(snake.head_position);
        let tail_visible =
            tail_candidates.contains(&snake.entity) && tail_intersects_region(&snake.tail, region);
        if head_visible || tail_visible {
            insert_snake_actor(&mut visible, snake);
        }
    }

    visible
}

fn all_managed_entities_in_room(
    room: RoomId,
    players: &[PlayerSnapshot],
    snakes: &[SnakeSnapshot],
    food: &[FoodPoint],
) -> EntityHashSet {
    let mut entities = EntityHashSet::default();
    entities.extend(
        players
            .iter()
            .filter(|player| player.room == room)
            .map(|player| player.entity),
    );
    entities.extend(
        snakes
            .iter()
            .filter(|snake| snake.room == room)
            .map(|snake| snake.entity),
    );
    entities.extend(
        food.iter()
            .filter(|food| food.room == room)
            .map(|food| food.entity),
    );
    entities
}

fn visibility_stats_for_client(
    room: RoomId,
    focus: Option<ClientFocus>,
    visible: &EntityHashSet,
    snakes: &[SnakeSnapshot],
    food: &[FoodPoint],
) -> InterestVisibilityStats {
    let focus_snake = focus.map(|focus| focus.snake);
    let visible_food = food
        .iter()
        .filter(|food| food.room == room && visible.contains(&food.entity))
        .count();
    let total_food = food.iter().filter(|food| food.room == room).count();
    let visible_remote_snakes = snakes
        .iter()
        .filter(|snake| {
            snake.room == room
                && Some(snake.entity) != focus_snake
                && visible.contains(&snake.entity)
        })
        .count();
    let total_remote_snakes = snakes
        .iter()
        .filter(|snake| snake.room == room && Some(snake.entity) != focus_snake)
        .count();

    InterestVisibilityStats {
        visible_food,
        total_food,
        visible_remote_snakes,
        total_remote_snakes,
    }
}

fn insert_snake_actor(visible: &mut EntityHashSet, snake: &SnakeSnapshot) {
    visible.insert(snake.entity);
    if let Some(player) = snake.player {
        visible.insert(player);
    }
}

#[derive(Clone, Copy, Debug)]
struct InterestRegion {
    center: Vec2,
    half_extents: Vec2,
}

impl InterestRegion {
    fn new(center: Vec2, half_extents: Vec2) -> Self {
        Self {
            center,
            half_extents: half_extents.max(Vec2::ZERO),
        }
    }

    fn min_x(self) -> f32 {
        self.center.x - self.half_extents.x
    }

    fn max_x(self) -> f32 {
        self.center.x + self.half_extents.x
    }

    fn min_y(self) -> f32 {
        self.center.y - self.half_extents.y
    }

    fn max_y(self) -> f32 {
        self.center.y + self.half_extents.y
    }

    fn contains_point(self, point: Vec2) -> bool {
        point.x >= self.min_x()
            && point.x <= self.max_x()
            && point.y >= self.min_y()
            && point.y <= self.max_y()
    }

    fn segment_aabb_overlaps(self, start: Vec2, end: Vec2) -> bool {
        ranges_overlap(
            start.x.min(end.x),
            start.x.max(end.x),
            self.min_x(),
            self.max_x(),
        ) && ranges_overlap(
            start.y.min(end.y),
            start.y.max(end.y),
            self.min_y(),
            self.max_y(),
        )
    }
}

fn entity_region(
    entity: Entity,
    previous_visible: Option<&EntityHashSet>,
    enter_region: InterestRegion,
    leave_region: InterestRegion,
) -> InterestRegion {
    if previous_visible.is_some_and(|visible| visible.contains(&entity)) {
        leave_region
    } else {
        enter_region
    }
}

fn tail_intersects_region(tail: &TailPolyline, region: InterestRegion) -> bool {
    tail.pairs_front_to_back()
        .any(|(start, end)| segment_intersects_region(start.0, end.0, region))
}

fn segment_intersects_region(start: Vec2, end: Vec2, region: InterestRegion) -> bool {
    if !region.segment_aabb_overlaps(start, end) {
        return false;
    }
    if region.contains_point(start) || region.contains_point(end) {
        return true;
    }

    let delta = end - start;
    let mut t_min = 0.0;
    let mut t_max = 1.0;
    clip_segment(-delta.x, start.x - region.min_x(), &mut t_min, &mut t_max)
        && clip_segment(delta.x, region.max_x() - start.x, &mut t_min, &mut t_max)
        && clip_segment(-delta.y, start.y - region.min_y(), &mut t_min, &mut t_max)
        && clip_segment(delta.y, region.max_y() - start.y, &mut t_min, &mut t_max)
}

fn clip_segment(p: f32, q: f32, t_min: &mut f32, t_max: &mut f32) -> bool {
    if p.abs() <= f32::EPSILON {
        return q >= 0.0;
    }
    let t = q / p;
    if p < 0.0 {
        if t > *t_max {
            return false;
        }
        *t_min = t.max(*t_min);
    } else {
        if t < *t_min {
            return false;
        }
        *t_max = t.min(*t_max);
    }
    true
}

fn ranges_overlap(left_min: f32, left_max: f32, right_min: f32, right_max: f32) -> bool {
    left_min <= right_max && right_min <= left_max
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use shared::network::protocol::prelude::Direction;

    use super::*;

    fn entity(index: u32) -> Entity {
        Entity::from_raw_u32(index).unwrap()
    }

    fn tail(points: impl IntoIterator<Item = (Vec2, Direction)>) -> TailPolyline {
        TailPolyline::new(VecDeque::from_iter(points))
    }

    #[test]
    fn interest_includes_snake_when_tail_crosses_screen_region() {
        let room = RoomId(1);
        let local_player = entity(1);
        let local_snake = entity(2);
        let remote_player = entity(3);
        let remote_snake = entity(4);
        let players = vec![
            PlayerSnapshot {
                entity: local_player,
                id: PeerId::Netcode(1),
                snake: Some(local_snake),
                room,
            },
            PlayerSnapshot {
                entity: remote_player,
                id: PeerId::Netcode(2),
                snake: Some(remote_snake),
                room,
            },
        ];
        let snakes = vec![
            SnakeSnapshot {
                entity: local_snake,
                player: Some(local_player),
                room,
                head_position: Vec2::ZERO,
                tail_length: 200.0,
                tail: tail([(Vec2::ZERO, Direction::Right), (Vec2::X, Direction::Right)]),
            },
            SnakeSnapshot {
                entity: remote_snake,
                player: Some(remote_player),
                room,
                head_position: Vec2::new(500.0, 500.0),
                tail_length: 200.0,
                tail: tail([
                    (Vec2::new(500.0, 500.0), Direction::Right),
                    (Vec2::new(20.0, 0.0), Direction::Right),
                    (Vec2::new(-20.0, 0.0), Direction::Left),
                ]),
            },
        ];
        let food = Vec::new();
        let food_index = FoodSpatialIndex::from_food([]);
        let tail_index = TailSpatialIndex::from_tails(
            snakes
                .iter()
                .map(|snake| (snake.entity, snake.room, &snake.tail)),
        );
        let mut config = GameConfig::default();
        config.network.interest.view_margin = 0.0;
        config.network.interest.hysteresis_margin = 0.0;
        let visible = visible_entities_for_client(
            room,
            Some(ClientFocus {
                player: local_player,
                snake: local_snake,
                position: Vec2::ZERO,
                tail_length: 200.0,
            }),
            UVec2::new(100, 100),
            None,
            &config,
            &players,
            &snakes,
            &food,
            &food_index,
            &tail_index,
        );

        assert!(visible.contains(&remote_snake));
        assert!(visible.contains(&remote_player));
    }

    #[test]
    fn interest_uses_margin_for_previously_visible_food() {
        let room = RoomId(1);
        let local_player = entity(1);
        let local_snake = entity(2);
        let food_entity = entity(3);
        let players = vec![PlayerSnapshot {
            entity: local_player,
            id: PeerId::Netcode(1),
            snake: Some(local_snake),
            room,
        }];
        let snakes = vec![SnakeSnapshot {
            entity: local_snake,
            player: Some(local_player),
            room,
            head_position: Vec2::ZERO,
            tail_length: 200.0,
            tail: tail([(Vec2::ZERO, Direction::Right), (Vec2::X, Direction::Right)]),
        }];
        let food = vec![FoodPoint {
            entity: food_entity,
            room,
            position: Vec2::new(55.0, 0.0),
        }];
        let food_index = FoodSpatialIndex::from_food(food.iter().copied());
        let tail_index = TailSpatialIndex::from_tails(
            snakes
                .iter()
                .map(|snake| (snake.entity, snake.room, &snake.tail)),
        );
        let mut previous = EntityHashSet::default();
        previous.insert(food_entity);
        let mut config = GameConfig::default();
        config.network.interest.view_margin = 0.0;
        config.network.interest.hysteresis_margin = 50.0;

        let visible = visible_entities_for_client(
            room,
            Some(ClientFocus {
                player: local_player,
                snake: local_snake,
                position: Vec2::ZERO,
                tail_length: 200.0,
            }),
            UVec2::new(100, 100),
            Some(&previous),
            &config,
            &players,
            &snakes,
            &food,
            &food_index,
            &tail_index,
        );

        assert!(visible.contains(&food_entity));
    }
}
