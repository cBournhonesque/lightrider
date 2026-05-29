use bevy::prelude::*;
use bevy_turborand::prelude::*;
use lightyear::connection::client::{Connected, Disconnected};
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{
    MessageReceiver, RemoteId, RoomAllocator, RoomId as LightyearRoomId,
    RoomPlugin as LightyearRoomPlugin, Rooms,
};

use shared::config::GameConfig;
use shared::map::spawn_room_map;
use shared::network::protocol::prelude::*;

#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ClientRoom {
    pub(crate) room: RoomId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RoomAssignment {
    pub(crate) game_room: RoomId,
    pub(crate) lightyear_room: LightyearRoomId,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RoomState {
    pub(crate) game_room: RoomId,
    pub(crate) lightyear_room: LightyearRoomId,
    human_count: usize,
    private: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RoomMetrics {
    pub(crate) game_room: RoomId,
    pub(crate) human_count: usize,
    pub(crate) private: bool,
}

#[derive(Resource, Default, Debug)]
pub(crate) struct RoomDirectory {
    rooms: Vec<RoomState>,
    next_room_id: u64,
}

impl RoomDirectory {
    pub(crate) fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = RoomAssignment> + '_ {
        self.rooms.iter().map(|room| RoomAssignment {
            game_room: room.game_room,
            lightyear_room: room.lightyear_room,
        })
    }

    pub(crate) fn lightyear_room(&self, game_room: RoomId) -> Option<LightyearRoomId> {
        self.rooms
            .iter()
            .find(|room| room.game_room == game_room)
            .map(|room| room.lightyear_room)
    }

    pub(crate) fn metrics(&self) -> impl Iterator<Item = RoomMetrics> + '_ {
        self.rooms.iter().map(|room| RoomMetrics {
            game_room: room.game_room,
            human_count: room.human_count,
            private: room.private,
        })
    }

    pub(crate) fn assign_auto(
        &mut self,
        commands: &mut Commands,
        room_allocator: &mut RoomAllocator,
        config: &GameConfig,
        roll: usize,
    ) -> RoomAssignment {
        let max_players_per_room = config.rooms.max_players_per_room.max(1);
        let max_rooms = config.rooms.max_rooms.max(1);
        let public_indices = self
            .rooms
            .iter()
            .enumerate()
            .filter_map(|(index, room)| (!room.private).then_some(index))
            .collect::<Vec<_>>();
        let candidates = public_indices
            .iter()
            .copied()
            .filter(|index| self.rooms[*index].human_count < max_players_per_room)
            .collect::<Vec<_>>();

        if !candidates.is_empty() {
            return self.assignment_at(candidates[roll % candidates.len()]);
        }
        if self.rooms.len() < max_rooms {
            return self.create_room(commands, room_allocator, config, None);
        }

        if let Some(fallback) = public_indices
            .into_iter()
            .min_by_key(|index| self.rooms[*index].human_count)
        {
            return self.assignment_at(fallback);
        }

        // Preserve private-room isolation even when all configured room slots are private.
        self.create_room(commands, room_allocator, config, None)
    }

    pub(crate) fn assign_for_mode(
        &mut self,
        commands: &mut Commands,
        room_allocator: &mut RoomAllocator,
        config: &GameConfig,
        mode: RoomJoinMode,
        roll: usize,
    ) -> RoomAssignment {
        match mode {
            RoomJoinMode::Auto => self.assign_auto(commands, room_allocator, config, roll),
            RoomJoinMode::New => {
                if self.rooms.len() < config.rooms.max_rooms.max(1) {
                    self.create_room(commands, room_allocator, config, None)
                } else {
                    self.assign_auto(commands, room_allocator, config, roll)
                }
            }
            RoomJoinMode::Specific(room_id) => {
                self.assign_specific(commands, room_allocator, config, room_id, roll)
            }
            RoomJoinMode::Private(code) => {
                self.assign_private(commands, room_allocator, config, code.room_id())
            }
        }
    }

    fn assign_specific(
        &mut self,
        commands: &mut Commands,
        room_allocator: &mut RoomAllocator,
        config: &GameConfig,
        room_id: RoomId,
        roll: usize,
    ) -> RoomAssignment {
        if let Some(existing) = self.rooms.iter().position(|room| room.game_room == room_id) {
            self.assignment_at(existing)
        } else if self.rooms.len() < config.rooms.max_rooms.max(1) {
            self.create_room(commands, room_allocator, config, Some(room_id))
        } else {
            self.assign_auto(commands, room_allocator, config, roll)
        }
    }

    fn assign_private(
        &mut self,
        commands: &mut Commands,
        room_allocator: &mut RoomAllocator,
        config: &GameConfig,
        room_id: RoomId,
    ) -> RoomAssignment {
        if let Some(existing) = self.rooms.iter().position(|room| room.game_room == room_id) {
            return self.assignment_at(existing);
        }

        // A full server should not route a private-code request into a public room.
        // Treat max_rooms as a soft guard here; Bevygap/server capacity policy should
        // prevent this path in production.
        self.create_room(commands, room_allocator, config, Some(room_id))
    }

    pub(crate) fn register_human(&mut self, room_id: RoomId) {
        if let Some(room) = self.rooms.iter_mut().find(|room| room.game_room == room_id) {
            room.human_count = room.human_count.saturating_add(1);
        }
    }

    pub(crate) fn unregister_human(&mut self, room_id: RoomId) {
        if let Some(room) = self.rooms.iter_mut().find(|room| room.game_room == room_id) {
            room.human_count = room.human_count.saturating_sub(1);
        }
    }

    pub(crate) fn move_human(&mut self, from: RoomId, to: RoomId) {
        if from == to {
            return;
        }
        self.unregister_human(from);
        self.register_human(to);
    }

    fn assignment_at(&self, index: usize) -> RoomAssignment {
        let room = self.rooms[index];
        RoomAssignment {
            game_room: room.game_room,
            lightyear_room: room.lightyear_room,
        }
    }

    fn create_room(
        &mut self,
        commands: &mut Commands,
        room_allocator: &mut RoomAllocator,
        config: &GameConfig,
        requested: Option<RoomId>,
    ) -> RoomAssignment {
        let game_room = requested.unwrap_or_else(|| RoomId(self.next_room_id));
        if requested.is_none() || !RoomCode::is_private_room_id(game_room) {
            self.next_room_id = self.next_room_id.max(game_room.0.saturating_add(1));
        }
        let lightyear_room = room_allocator.allocate();
        commands.spawn(Name::from(format!("Room {}", game_room.0)));
        spawn_room_map(commands, config, game_room);
        self.rooms.push(RoomState {
            game_room,
            lightyear_room,
            human_count: 0,
            private: RoomCode::is_private_room_id(game_room),
        });
        RoomAssignment {
            game_room,
            lightyear_room,
        }
    }
}

pub(crate) struct ServerRoomsPlugin;

impl Plugin for ServerRoomsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(LightyearRoomPlugin);
        app.init_resource::<RoomDirectory>();
        app.add_systems(Startup, ensure_initial_room);
        app.add_systems(
            Update,
            (
                handle_player_name_updates,
                handle_room_join_requests,
                update_player_ranks,
            ),
        );
        app.add_observer(handle_disconnected);
    }
}

fn ensure_initial_room(
    mut commands: Commands,
    config: Res<GameConfig>,
    mut room_allocator: ResMut<RoomAllocator>,
    mut directory: ResMut<RoomDirectory>,
) {
    if directory.is_empty() {
        directory.create_room(&mut commands, &mut room_allocator, &config, None);
    }
}

fn handle_player_name_updates(
    mut clients: Query<(&RemoteId, &mut MessageReceiver<PlayerNameUpdate>), With<Connected>>,
    mut players: Query<&mut Player>,
) {
    for (remote_id, mut receiver) in &mut clients {
        for message in receiver.receive() {
            let name = sanitize_player_name(&message.name);
            for mut player in &mut players {
                if player.id == remote_id.0 {
                    player.name = name.clone();
                    break;
                }
            }
        }
    }
}

fn sanitize_player_name(name: &str) -> String {
    let trimmed = name.trim();
    let sanitized = if trimmed.is_empty() {
        "Player"
    } else {
        trimmed
    };
    sanitized.chars().take(18).collect()
}

fn handle_room_join_requests(
    mut commands: Commands,
    config: Res<GameConfig>,
    mut directory: ResMut<RoomDirectory>,
    mut room_allocator: ResMut<RoomAllocator>,
    mut rng: ResMut<GlobalRng>,
    mut clients: Query<
        (
            Entity,
            &RemoteId,
            &mut MessageReceiver<RoomJoinRequest>,
            Option<&ClientRoom>,
        ),
        With<Connected>,
    >,
    players: Query<(Entity, &Player)>,
    mut room_components: Query<&mut RoomId>,
) {
    for (client_entity, remote_id, mut receiver, client_room) in &mut clients {
        let mut current_room = client_room.map(|room| room.room);
        for request in receiver.receive() {
            let roll = rng.usize(..);
            let assignment = directory.assign_for_mode(
                &mut commands,
                &mut room_allocator,
                &config,
                request.mode,
                roll,
            );
            if Some(assignment.game_room) == current_room {
                continue;
            }

            move_client_to_room(
                &mut commands,
                &mut directory,
                client_entity,
                remote_id.0,
                current_room,
                assignment,
                &players,
                &mut room_components,
            );
            current_room = Some(assignment.game_room);
        }
    }
}

fn update_player_ranks(
    players: Query<(Entity, &RoomId, &PlayerScore), With<Player>>,
    mut ranks: Query<&mut PlayerRank, With<Player>>,
) {
    let mut rows = players
        .iter()
        .map(|(entity, room, score)| RankRow {
            entity,
            room: *room,
            score: score.value,
        })
        .collect::<Vec<_>>();
    let updates = compute_room_ranks(&mut rows);
    for (entity, rank) in updates {
        if let Ok(mut player_rank) = ranks.get_mut(entity) {
            *player_rank = rank;
        }
    }
}

#[derive(Clone, Copy)]
struct RankRow {
    entity: Entity,
    room: RoomId,
    score: u32,
}

fn compute_room_ranks(rows: &mut [RankRow]) -> Vec<(Entity, PlayerRank)> {
    rows.sort_by(|a, b| {
        a.room
            .0
            .cmp(&b.room.0)
            .then_with(|| b.score.cmp(&a.score))
            .then_with(|| a.entity.to_bits().cmp(&b.entity.to_bits()))
    });

    let mut updates = Vec::with_capacity(rows.len());
    let mut current_room = None;
    let mut rank = 0_u16;
    for row in rows {
        if current_room != Some(row.room) {
            current_room = Some(row.room);
            rank = 1;
        } else {
            rank = rank.saturating_add(1);
        }
        updates.push((row.entity, PlayerRank { value: rank }));
    }
    updates
}

pub(crate) fn move_client_to_room(
    commands: &mut Commands,
    directory: &mut RoomDirectory,
    client_entity: Entity,
    client_id: lightyear::prelude::PeerId,
    current_room: Option<RoomId>,
    assignment: RoomAssignment,
    players: &Query<(Entity, &Player)>,
    room_components: &mut Query<&mut RoomId>,
) {
    if let Some(current_room) = current_room {
        directory.move_human(current_room, assignment.game_room);
    } else {
        directory.register_human(assignment.game_room);
    }

    add_replicated_entity_to_room(commands, assignment.lightyear_room, client_entity);
    commands.entity(client_entity).insert(ClientRoom {
        room: assignment.game_room,
    });

    let Some((player_entity, player)) = players.iter().find(|(_, player)| player.id == client_id)
    else {
        return;
    };
    move_replicated_entity_to_room(commands, player_entity, assignment.lightyear_room);
    if let Ok(mut player_room) = room_components.get_mut(player_entity) {
        *player_room = assignment.game_room;
    }

    if let Some(snake_entity) = player.snake {
        if let Ok(mut snake_room) = room_components.get_mut(snake_entity) {
            move_replicated_entity_to_room(commands, snake_entity, assignment.lightyear_room);
            *snake_room = assignment.game_room;
        }
    }
}

pub(crate) fn add_replicated_entity_to_room(
    commands: &mut Commands,
    room: LightyearRoomId,
    entity: Entity,
) {
    commands.entity(entity).insert(Rooms::single(room));
}

pub(crate) fn remove_replicated_entity_from_room(
    commands: &mut Commands,
    _room: LightyearRoomId,
    entity: Entity,
) {
    commands.entity(entity).remove::<Rooms>();
}

fn move_replicated_entity_to_room(
    commands: &mut Commands,
    entity: Entity,
    lightyear_room: LightyearRoomId,
) {
    add_replicated_entity_to_room(commands, lightyear_room, entity);
}

fn handle_disconnected(
    trigger: On<Add, Disconnected>,
    clients: Query<(&RemoteId, Option<&ClientRoom>), With<ClientOf>>,
    players: Query<(Entity, &Player)>,
    mut directory: ResMut<RoomDirectory>,
    mut commands: Commands,
) {
    let Ok((remote_id, client_room)) = clients.get(trigger.entity) else {
        return;
    };
    if let Some(client_room) = client_room {
        directory.unregister_human(client_room.room);
    }

    let Some((player_entity, player)) = players.iter().find(|(_, player)| player.id == remote_id.0)
    else {
        return;
    };
    if let Some(snake_entity) = player.snake {
        commands.entity(snake_entity).try_despawn();
    }
    commands.entity(player_entity).try_despawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_room_does_not_assign_private_rooms() {
        #[derive(Resource, Default)]
        struct Results {
            private: Option<RoomAssignment>,
            public: Option<RoomAssignment>,
        }

        fn assign_rooms(
            mut commands: Commands,
            config: Res<GameConfig>,
            mut room_allocator: ResMut<RoomAllocator>,
            mut directory: ResMut<RoomDirectory>,
            mut results: ResMut<Results>,
        ) {
            let private = directory.assign_for_mode(
                &mut commands,
                &mut room_allocator,
                &config,
                RoomJoinMode::Private(RoomCode::parse("ABCD").unwrap()),
                0,
            );
            let public = directory.assign_for_mode(
                &mut commands,
                &mut room_allocator,
                &config,
                RoomJoinMode::Auto,
                0,
            );
            results.private = Some(private);
            results.public = Some(public);
        }

        let mut app = App::new();
        app.init_resource::<RoomAllocator>();
        app.init_resource::<RoomDirectory>();
        app.init_resource::<Results>();
        app.insert_resource(GameConfig::default());
        app.add_systems(Update, assign_rooms);

        app.update();

        let results = app.world().resource::<Results>();
        assert_eq!(
            results.private.unwrap().game_room,
            RoomCode::parse("ABCD").unwrap().room_id()
        );
        assert_eq!(results.public.unwrap().game_room, RoomId(0));
    }

    #[test]
    fn private_room_does_not_fall_back_to_public_when_room_limit_is_full() {
        #[derive(Resource, Default)]
        struct Results {
            public: Option<RoomAssignment>,
            private: Option<RoomAssignment>,
        }

        fn assign_rooms(
            mut commands: Commands,
            config: Res<GameConfig>,
            mut room_allocator: ResMut<RoomAllocator>,
            mut directory: ResMut<RoomDirectory>,
            mut results: ResMut<Results>,
        ) {
            let public = directory.assign_for_mode(
                &mut commands,
                &mut room_allocator,
                &config,
                RoomJoinMode::Auto,
                0,
            );
            let private = directory.assign_for_mode(
                &mut commands,
                &mut room_allocator,
                &config,
                RoomJoinMode::Private(RoomCode::parse("WXYZ").unwrap()),
                0,
            );
            results.public = Some(public);
            results.private = Some(private);
        }

        let mut config = GameConfig::default();
        config.rooms.max_rooms = 1;

        let mut app = App::new();
        app.init_resource::<RoomAllocator>();
        app.init_resource::<RoomDirectory>();
        app.init_resource::<Results>();
        app.insert_resource(config);
        app.add_systems(Update, assign_rooms);

        app.update();

        let results = app.world().resource::<Results>();
        assert_eq!(results.public.unwrap().game_room, RoomId(0));
        assert_eq!(
            results.private.unwrap().game_room,
            RoomCode::parse("WXYZ").unwrap().room_id()
        );
    }

    #[test]
    fn player_ranks_are_computed_per_room_by_score() {
        let mut app = App::new();
        app.add_systems(Update, update_player_ranks);

        let low = app
            .world_mut()
            .spawn((
                Player {
                    id: lightyear::prelude::PeerId::Netcode(1),
                    name: "low".to_string(),
                    snake: None,
                },
                PlayerScore { value: 10 },
                PlayerRank::default(),
                RoomId(0),
            ))
            .id();
        let high = app
            .world_mut()
            .spawn((
                Player {
                    id: lightyear::prelude::PeerId::Netcode(2),
                    name: "high".to_string(),
                    snake: None,
                },
                PlayerScore { value: 20 },
                PlayerRank::default(),
                RoomId(0),
            ))
            .id();
        let other_room = app
            .world_mut()
            .spawn((
                Player {
                    id: lightyear::prelude::PeerId::Netcode(3),
                    name: "other".to_string(),
                    snake: None,
                },
                PlayerScore { value: 1 },
                PlayerRank::default(),
                RoomId(1),
            ))
            .id();

        app.update();

        assert_eq!(
            app.world().entity(high).get::<PlayerRank>(),
            Some(&PlayerRank { value: 1 })
        );
        assert_eq!(
            app.world().entity(low).get::<PlayerRank>(),
            Some(&PlayerRank { value: 2 })
        );
        assert_eq!(
            app.world().entity(other_room).get::<PlayerRank>(),
            Some(&PlayerRank { value: 1 })
        );
    }
}
