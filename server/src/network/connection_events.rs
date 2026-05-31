use bevy::prelude::*;
use lightyear::prelude::server::ClientOf;
use lightyear::prelude::*;

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
