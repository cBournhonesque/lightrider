use bevy::prelude::*;
use lightyear::connection::client::Connected;
use lightyear::prelude::Client;

use crate::matchmaker::LightriderMatchmakerState;
use crate::network::config::ClientConnectionStatus;

pub(crate) struct WebStatusPlugin;

const STATUS_ELEMENT_ID: &str = "lightrider-status";
const LOADING_ASSETS: &str = "Loading assets...";
const WAITING_EXISTING_SERVER: &str = "Waiting to connect to server";
const WAITING_NEW_SERVER: &str = "Waiting for new server to start";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DeploymentWaitMode {
    #[default]
    Unknown,
    Existing,
    Creating,
}

impl Plugin for WebStatusPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, sync_web_status);
    }
}

fn sync_web_status(
    state: Option<Res<LightriderMatchmakerState>>,
    connection_status: Option<Res<ClientConnectionStatus>>,
    connected_clients: Query<(), (With<Client>, With<Connected>)>,
    mut wait_mode: Local<DeploymentWaitMode>,
    mut last_status: Local<Option<String>>,
) {
    let status = web_status_text(
        state.as_deref(),
        connection_status
            .as_deref()
            .and_then(|status| status.disconnect_reason.as_deref()),
        connected_clients.iter().next().is_some(),
        &mut wait_mode,
    );
    if *last_status == status {
        return;
    }

    set_status_element(status.as_deref());
    *last_status = status;
}

fn web_status_text(
    state: Option<&LightriderMatchmakerState>,
    disconnect_reason: Option<&str>,
    connected: bool,
    wait_mode: &mut DeploymentWaitMode,
) -> Option<String> {
    if connected {
        return None;
    }

    if let Some(reason) = disconnect_reason {
        return Some(reason.to_string());
    }

    let Some(state) = state else {
        return Some(LOADING_ASSETS.to_string());
    };

    match state {
        LightriderMatchmakerState::Dormant => Some(WAITING_EXISTING_SERVER.to_string()),
        LightriderMatchmakerState::Waiting(message) => {
            update_wait_mode(message, wait_mode);
            match *wait_mode {
                DeploymentWaitMode::Creating => Some(WAITING_NEW_SERVER.to_string()),
                DeploymentWaitMode::Existing | DeploymentWaitMode::Unknown => {
                    Some(WAITING_EXISTING_SERVER.to_string())
                }
            }
        }
        LightriderMatchmakerState::Connecting | LightriderMatchmakerState::Finished => {
            Some(WAITING_EXISTING_SERVER.to_string())
        }
        LightriderMatchmakerState::Error(message) => Some(format!("Connection error: {message}")),
    }
}

fn update_wait_mode(message: &str, wait_mode: &mut DeploymentWaitMode) {
    let lower = message.to_ascii_lowercase();
    if lower.contains("routing to existing deployment") {
        *wait_mode = DeploymentWaitMode::Existing;
    } else if lower.contains("creating new deployment")
        || lower.contains("create a new deployment")
        || (lower.contains("session created") && *wait_mode == DeploymentWaitMode::Unknown)
    {
        *wait_mode = DeploymentWaitMode::Creating;
    }
}

fn set_status_element(status: Option<&str>) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(element) = document.get_element_by_id(STATUS_ELEMENT_ID) else {
        return;
    };

    match status {
        Some(status) => {
            element.set_inner_html(status);
            let _ = element.set_attribute("class", "loading-status");
        }
        None => {
            element.set_inner_html("");
            let _ = element.set_attribute("class", "loading-status hidden");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_distinguishes_new_and_existing_deployments() {
        let mut mode = DeploymentWaitMode::Unknown;

        assert_eq!(
            web_status_text(
                Some(&LightriderMatchmakerState::Waiting(
                    "routing to existing deployment abc".to_string()
                )),
                None,
                false,
                &mut mode,
            ),
            Some(WAITING_EXISTING_SERVER.to_string())
        );
        assert_eq!(mode, DeploymentWaitMode::Existing);

        let mut mode = DeploymentWaitMode::Unknown;
        assert_eq!(
            web_status_text(
                Some(&LightriderMatchmakerState::Waiting(
                    "creating new deployment".to_string()
                )),
                None,
                false,
                &mut mode,
            ),
            Some(WAITING_NEW_SERVER.to_string())
        );
        assert_eq!(mode, DeploymentWaitMode::Creating);
    }

    #[test]
    fn status_hides_after_lightyear_connects() {
        let mut mode = DeploymentWaitMode::Creating;

        assert_eq!(
            web_status_text(
                Some(&LightriderMatchmakerState::Waiting(
                    "creating new deployment".to_string()
                )),
                None,
                true,
                &mut mode,
            ),
            None
        );
    }

    #[test]
    fn status_shows_disconnect_reason_while_disconnected() {
        let mut mode = DeploymentWaitMode::Creating;

        assert_eq!(
            web_status_text(
                Some(&LightriderMatchmakerState::Waiting(
                    "creating new deployment".to_string()
                )),
                Some("Disconnected because network latency exceeded the prediction budget."),
                false,
                &mut mode,
            ),
            Some(
                "Disconnected because network latency exceeded the prediction budget.".to_string()
            )
        );
    }
}
