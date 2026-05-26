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
    trigger: On<Add, Connected>,
    clients: Query<&RemoteId, With<ClientOf>>,
    config: Res<GameConfig>,
    mut directory: ResMut<RoomDirectory>,
    mut rng: ResMut<GlobalRng>,
    mut commands: Commands,
) {
    let Ok(client_id) = clients.get(trigger.entity) else {
        return;
    };
    let client_id = client_id.0;
    let assignment = directory.assign_auto(&mut commands, &config, rng.usize(..));
    directory.register_human(assignment.game_room);
    info!(
        "Client {client_id:?} connected to room {}",
        assignment.game_room.0,
    );
    commands.entity(trigger.entity).insert(ClientRoom {
        room: assignment.game_room,
    });
    commands.trigger(RoomEvent {
        room: assignment.lightyear_room,
        target: RoomTarget::AddSender(trigger.entity),
    });

    let (spawn_position, spawn_direction) =
        snake_spawn_pose(&config, assignment.game_room, client_id.to_bits());
    let head_entity = SnakeBundle::spawn_with_room_at(
        &mut commands,
        client_id,
        &config.movement,
        assignment.game_room,
        spawn_position,
        spawn_direction,
    );
    let player_entity = PlayerBundle::new_in_room(
        Player {
            id: client_id,
            name: "Player".to_string(),
            snake: Some(head_entity),
        },
        assignment.game_room,
    )
    .spawn(&mut commands, client_id);
    commands
        .entity(player_entity)
        .insert(PlayerScore::from_length(
            config.movement.starting_tail_length,
        ));

    let controlled_by = ControlledBy {
        owner: trigger.entity,
        lifetime: Default::default(),
    };
    commands
        .entity(head_entity)
        .insert((HasPlayer(player_entity), controlled_by));
    commands.entity(player_entity).insert(controlled_by);
    add_replicated_entity_to_room(&mut commands, assignment.lightyear_room, player_entity);
    add_replicated_entity_to_room(&mut commands, assignment.lightyear_room, head_entity);
    spawn_snake_input_actions(&mut commands, head_entity, client_id, true);
    spawn_player_input_actions(&mut commands, player_entity, client_id, true);
}
