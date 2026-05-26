use bevy::prelude::*;
use bevy_turborand::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::*;

use crate::rooms::{add_replicated_entity_to_room, ClientRoom, RoomDirectory};
use crate::spawning::snake_spawn_pose;
use shared::config::GameConfig;
use shared::network::bundle::player::PlayerBundle;
use shared::network::bundle::snake::SnakeBundle;
use shared::network::protocol::prelude::*;

pub(crate) fn handle_new_client(trigger: On<Add, LinkOf>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert((ReplicationSender::default(), Name::from("Client")));
}

pub(crate) fn handle_new_client_of(trigger: On<Add, ClientOf>, mut commands: Commands) {
    commands
        .entity(trigger.entity)
        .insert((ReplicationSender::default(), Name::from("Client")));
}

pub(crate) fn handle_connected(
    _trigger: On<Add, Connected>,
    clients: Query<
        (Entity, &RemoteId, Option<&ClientRoom>),
        (With<ClientOf>, With<Connected>, With<ReplicationSender>),
    >,
    pending_clients: Query<(), (With<ClientOf>, Without<ReplicationSender>)>,
    config: Res<GameConfig>,
    mut room_allocator: ResMut<RoomAllocator>,
    mut directory: ResMut<RoomDirectory>,
    mut rng: ResMut<GlobalRng>,
    mut commands: Commands,
) {
    spawn_ready_clients(
        &clients,
        &pending_clients,
        &config,
        &mut room_allocator,
        &mut directory,
        &mut rng,
        &mut commands,
    );
}

pub(crate) fn handle_replication_sender_ready(
    _trigger: On<Add, ReplicationSender>,
    clients: Query<
        (Entity, &RemoteId, Option<&ClientRoom>),
        (With<ClientOf>, With<Connected>, With<ReplicationSender>),
    >,
    pending_clients: Query<(), (With<ClientOf>, Without<ReplicationSender>)>,
    config: Res<GameConfig>,
    mut room_allocator: ResMut<RoomAllocator>,
    mut directory: ResMut<RoomDirectory>,
    mut rng: ResMut<GlobalRng>,
    mut commands: Commands,
) {
    spawn_ready_clients(
        &clients,
        &pending_clients,
        &config,
        &mut room_allocator,
        &mut directory,
        &mut rng,
        &mut commands,
    );
}

fn spawn_ready_clients(
    clients: &Query<
        (Entity, &RemoteId, Option<&ClientRoom>),
        (With<ClientOf>, With<Connected>, With<ReplicationSender>),
    >,
    pending_clients: &Query<(), (With<ClientOf>, Without<ReplicationSender>)>,
    config: &GameConfig,
    room_allocator: &mut RoomAllocator,
    directory: &mut RoomDirectory,
    rng: &mut GlobalRng,
    commands: &mut Commands,
) {
    if !pending_clients.is_empty() {
        return;
    }

    let ready_clients = clients
        .iter()
        .filter_map(|(client_entity, client_id, client_room)| {
            client_room
                .is_none()
                .then_some((client_entity, client_id.0))
        })
        .collect::<Vec<_>>();
    for (client_entity, client_id) in ready_clients {
        spawn_ready_client(
            client_entity,
            client_id,
            config,
            room_allocator,
            directory,
            rng,
            commands,
        );
    }
}

fn spawn_ready_client(
    client_entity: Entity,
    client_id: PeerId,
    config: &GameConfig,
    room_allocator: &mut RoomAllocator,
    directory: &mut RoomDirectory,
    rng: &mut GlobalRng,
    commands: &mut Commands,
) {
    let assignment = directory.assign_auto(commands, room_allocator, config, rng.usize(..));
    directory.register_human(assignment.game_room);
    info!(
        "Client {client_id:?} connected to room {}",
        assignment.game_room.0,
    );
    add_replicated_entity_to_room(commands, assignment.lightyear_room, client_entity);
    commands.entity(client_entity).insert(ClientRoom {
        room: assignment.game_room,
    });

    let (spawn_position, spawn_direction) =
        snake_spawn_pose(config, assignment.game_room, client_id.to_bits());
    let head_entity = SnakeBundle::spawn_with_room_at(
        commands,
        client_id,
        &config.movement,
        assignment.game_room,
        spawn_position,
        spawn_direction,
    );
    let player_entity = PlayerBundle::new_in_room(
        Player {
            id: client_id,
            name: format!("Player {}", client_id.to_bits()),
            snake: Some(head_entity),
        },
        assignment.game_room,
    )
    .spawn(commands, client_id);
    commands
        .entity(player_entity)
        .insert(PlayerScore::from_length(
            config.movement.starting_tail_length,
        ));

    let controlled_by = ControlledBy {
        owner: client_entity,
        lifetime: Default::default(),
    };
    commands
        .entity(head_entity)
        .insert((HasPlayer(player_entity), controlled_by));
    commands.entity(player_entity).insert(controlled_by);
    add_replicated_entity_to_room(commands, assignment.lightyear_room, player_entity);
    add_replicated_entity_to_room(commands, assignment.lightyear_room, head_entity);
    spawn_snake_input_actions(commands, head_entity, client_id, true);
    spawn_player_input_actions(commands, player_entity, client_id, true);
}
