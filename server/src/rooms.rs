use bevy::prelude::*;
use bevy_turborand::prelude::*;
use lightyear::connection::client::{Connected, Disconnected};
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::{
    MessageReceiver, RemoteId, Room, RoomEvent, RoomPlugin as LightyearRoomPlugin, RoomTarget,
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
    pub(crate) lightyear_room: Entity,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct RoomState {
    pub(crate) game_room: RoomId,
    pub(crate) lightyear_room: Entity,
    human_count: usize,
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

    pub(crate) fn lightyear_room(&self, game_room: RoomId) -> Option<Entity> {
        self.rooms
            .iter()
            .find(|room| room.game_room == game_room)
            .map(|room| room.lightyear_room)
    }

    pub(crate) fn assign_auto(
        &mut self,
        commands: &mut Commands,
        config: &GameConfig,
        roll: usize,
    ) -> RoomAssignment {
        match select_auto_room(
            self.rooms.iter().map(|room| room.human_count),
            config.rooms.max_players_per_room,
            config.rooms.max_rooms,
            roll,
        ) {
            AutoRoomSelection::Existing(index) | AutoRoomSelection::Fallback(index) => {
                self.assignment_at(index)
            }
            AutoRoomSelection::Create => self.create_room(commands, config, None),
        }
    }

    pub(crate) fn assign_for_mode(
        &mut self,
        commands: &mut Commands,
        config: &GameConfig,
        mode: RoomJoinMode,
        roll: usize,
    ) -> RoomAssignment {
        match mode {
            RoomJoinMode::Auto => self.assign_auto(commands, config, roll),
            RoomJoinMode::New => {
                if self.rooms.len() < config.rooms.max_rooms.max(1) {
                    self.create_room(commands, config, None)
                } else {
                    self.assign_auto(commands, config, roll)
                }
            }
            RoomJoinMode::Specific(room_id) => {
                if let Some(existing) = self.rooms.iter().position(|room| room.game_room == room_id)
                {
                    self.assignment_at(existing)
                } else if self.rooms.len() < config.rooms.max_rooms.max(1) {
                    self.create_room(commands, config, Some(room_id))
                } else {
                    self.assign_auto(commands, config, roll)
                }
            }
        }
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
        config: &GameConfig,
        requested: Option<RoomId>,
    ) -> RoomAssignment {
        let game_room = requested.unwrap_or_else(|| RoomId(self.next_room_id));
        self.next_room_id = self.next_room_id.max(game_room.0.saturating_add(1));
        let lightyear_room = commands
            .spawn((Room::default(), Name::from(format!("Room {}", game_room.0))))
            .id();
        spawn_room_map(commands, config, game_room);
        self.rooms.push(RoomState {
            game_room,
            lightyear_room,
            human_count: 0,
        });
        RoomAssignment {
            game_room,
            lightyear_room,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoRoomSelection {
    Existing(usize),
    Create,
    Fallback(usize),
}

fn select_auto_room(
    occupancies: impl IntoIterator<Item = usize>,
    max_players_per_room: usize,
    max_rooms: usize,
    roll: usize,
) -> AutoRoomSelection {
    let max_rooms = max_rooms.max(1);
    let max_players_per_room = max_players_per_room.max(1);
    let occupancies = occupancies.into_iter().collect::<Vec<_>>();
    let candidates = occupancies
        .iter()
        .enumerate()
        .filter_map(|(index, count)| (*count < max_players_per_room).then_some(index))
        .collect::<Vec<_>>();

    if !candidates.is_empty() {
        return AutoRoomSelection::Existing(candidates[roll % candidates.len()]);
    }
    if occupancies.len() < max_rooms {
        return AutoRoomSelection::Create;
    }

    let fallback = occupancies
        .iter()
        .enumerate()
        .min_by_key(|(_, count)| **count)
        .map(|(index, _)| index)
        .unwrap_or(0);
    AutoRoomSelection::Fallback(fallback)
}

pub(crate) struct ServerRoomsPlugin;

impl Plugin for ServerRoomsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(LightyearRoomPlugin);
        app.init_resource::<RoomDirectory>();
        app.add_systems(Startup, ensure_initial_room);
        app.add_systems(Update, (handle_room_join_requests, update_player_ranks));
        app.add_observer(handle_disconnected);
    }
}

fn ensure_initial_room(
    mut commands: Commands,
    config: Res<GameConfig>,
    mut directory: ResMut<RoomDirectory>,
) {
    if directory.is_empty() {
        directory.create_room(&mut commands, &config, None);
    }
}

fn handle_room_join_requests(
    mut commands: Commands,
    config: Res<GameConfig>,
    mut directory: ResMut<RoomDirectory>,
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
            let assignment = directory.assign_for_mode(&mut commands, &config, request.mode, roll);
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
        if let Some(current_lightyear_room) = directory.lightyear_room(current_room) {
            commands.trigger(RoomEvent {
                room: current_lightyear_room,
                target: RoomTarget::RemoveSender(client_entity),
            });
        }
        directory.move_human(current_room, assignment.game_room);
    } else {
        directory.register_human(assignment.game_room);
    }

    commands.trigger(RoomEvent {
        room: assignment.lightyear_room,
        target: RoomTarget::AddSender(client_entity),
    });
    commands.entity(client_entity).insert(ClientRoom {
        room: assignment.game_room,
    });

    let Some((player_entity, player)) = players.iter().find(|(_, player)| player.id == client_id)
    else {
        return;
    };
    move_replicated_entity_to_room(
        commands,
        directory,
        player_entity,
        current_room,
        assignment.lightyear_room,
    );
    if let Ok(mut player_room) = room_components.get_mut(player_entity) {
        *player_room = assignment.game_room;
    }

    if let Some(snake_entity) = player.snake {
        if let Ok(mut snake_room) = room_components.get_mut(snake_entity) {
            move_replicated_entity_to_room(
                commands,
                directory,
                snake_entity,
                current_room,
                assignment.lightyear_room,
            );
            *snake_room = assignment.game_room;
        }
    }
}

pub(crate) fn add_replicated_entity_to_room(commands: &mut Commands, room: Entity, entity: Entity) {
    commands.trigger(RoomEvent {
        room,
        target: RoomTarget::AddEntity(entity),
    });
}

pub(crate) fn remove_replicated_entity_from_room(
    commands: &mut Commands,
    room: Entity,
    entity: Entity,
) {
    commands.trigger(RoomEvent {
        room,
        target: RoomTarget::RemoveEntity(entity),
    });
}

fn move_replicated_entity_to_room(
    commands: &mut Commands,
    directory: &RoomDirectory,
    entity: Entity,
    current_room: Option<RoomId>,
    lightyear_room: Entity,
) {
    if let Some(current_room) = current_room {
        if let Some(current_lightyear_room) = directory.lightyear_room(current_room) {
            remove_replicated_entity_from_room(commands, current_lightyear_room, entity);
        }
    }
    add_replicated_entity_to_room(commands, lightyear_room, entity);
}

fn handle_disconnected(
    trigger: On<Add, Disconnected>,
    clients: Query<(&RemoteId, Option<&ClientRoom>), With<ClientOf>>,
    players: Query<(Entity, &Player, &RoomId)>,
    mut directory: ResMut<RoomDirectory>,
    mut commands: Commands,
) {
    let Ok((remote_id, client_room)) = clients.get(trigger.entity) else {
        return;
    };
    if let Some(client_room) = client_room {
        directory.unregister_human(client_room.room);
    }

    let Some((player_entity, player, room)) = players
        .iter()
        .find(|(_, player, _)| player.id == remote_id.0)
    else {
        return;
    };
    if let Some(lightyear_room) = directory.lightyear_room(*room) {
        remove_replicated_entity_from_room(&mut commands, lightyear_room, player_entity);
        if let Some(snake_entity) = player.snake {
            remove_replicated_entity_from_room(&mut commands, lightyear_room, snake_entity);
        }
    }
    if let Some(snake_entity) = player.snake {
        commands.entity(snake_entity).try_despawn();
    }
    commands.entity(player_entity).try_despawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_room_selects_available_room_with_roll() {
        assert_eq!(
            select_auto_room([0, 2, 1], 2, 4, 1),
            AutoRoomSelection::Existing(2)
        );
    }

    #[test]
    fn auto_room_creates_when_all_rooms_are_full_but_limit_allows() {
        assert_eq!(select_auto_room([2, 2], 2, 3, 0), AutoRoomSelection::Create);
    }

    #[test]
    fn auto_room_falls_back_to_least_populated_at_room_limit() {
        assert_eq!(
            select_auto_room([4, 2, 3], 2, 3, 0),
            AutoRoomSelection::Fallback(1)
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
