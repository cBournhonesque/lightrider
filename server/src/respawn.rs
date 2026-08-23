use bevy::prelude::*;

use shared::config::GameConfig;

#[derive(Component, Clone, Copy, Debug, PartialEq, Reflect)]
pub(crate) struct RespawnReadyAt {
    seconds: f64,
}

impl RespawnReadyAt {
    pub(crate) fn from_now(now_seconds: f64, delay_seconds: f32) -> Self {
        Self {
            seconds: now_seconds + f64::from(delay_seconds.max(0.0)),
        }
    }

    pub(crate) fn is_ready(&self, now_seconds: f64) -> bool {
        now_seconds >= self.seconds
    }

    #[cfg(test)]
    fn seconds_remaining(&self, now_seconds: f64) -> f32 {
        (self.seconds - now_seconds).max(0.0) as f32
    }
}

pub(crate) fn respawn_delay_seconds(config: &GameConfig, is_bot: bool) -> f32 {
    if is_bot {
        config.respawn.bot_cooldown_seconds
    } else {
        config.respawn.player_cooldown_seconds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn respawn_ready_tracks_remaining_time() {
        let ready_at = RespawnReadyAt::from_now(10.0, 1.5);

        assert!(!ready_at.is_ready(11.0));
        assert_eq!(ready_at.seconds_remaining(11.0), 0.5);
        assert!(ready_at.is_ready(11.5));
        assert_eq!(ready_at.seconds_remaining(12.0), 0.0);
    }

    #[test]
    fn negative_respawn_delays_are_ready_immediately() {
        let ready_at = RespawnReadyAt::from_now(10.0, -5.0);

        assert!(ready_at.is_ready(10.0));
    }
}
