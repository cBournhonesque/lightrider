use bevy::prelude::*;

use crate::config::ArenaConfig;
use crate::network::protocol::prelude::{Direction, TailPolyline};
use crate::utils::geometry::ray_segment_intersection;

const LOOKAHEAD_DISTANCE: f32 = 420.0;
const DANGER_DISTANCE: f32 = 140.0;
const MIN_SAFE_TURN_DISTANCE: f32 = 180.0;
const MIN_SEGMENT_BEFORE_VOLUNTARY_TURN: f32 = 140.0;
const MAX_TRACKED_TURNS: usize = 16;

#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct BotMarker;

#[derive(Component, Clone, Copy, Debug, Reflect)]
pub struct BotController {
    ticks_until_decision: u32,
    decision_interval_ticks: u32,
    mistake_chance_per_decision_percent: u8,
    tick: u32,
    recent_turn_ticks: [u32; MAX_TRACKED_TURNS],
    recent_turn_count: u8,
    seed: u64,
}

impl BotController {
    pub fn new(decision_interval_ticks: u32, seed: u64) -> Self {
        Self::new_with_mistakes(decision_interval_ticks, seed, 0)
    }

    pub fn new_with_mistakes(
        decision_interval_ticks: u32,
        seed: u64,
        mistake_chance_per_decision_percent: u8,
    ) -> Self {
        Self {
            ticks_until_decision: 0,
            decision_interval_ticks: decision_interval_ticks.max(1),
            mistake_chance_per_decision_percent: mistake_chance_per_decision_percent.min(100),
            tick: 0,
            recent_turn_ticks: [0; MAX_TRACKED_TURNS],
            recent_turn_count: 0,
            seed: seed | 1,
        }
    }

    pub fn choose_direction(&mut self, tail: &TailPolyline, arena: &ArenaConfig) -> Direction {
        self.choose_direction_avoiding(tail, arena, &[])
    }

    pub fn choose_direction_avoiding(
        &mut self,
        tail: &TailPolyline,
        arena: &ArenaConfig,
        obstacle_tails: &[&TailPolyline],
    ) -> Direction {
        self.choose_direction_avoiding_limited(tail, arena, obstacle_tails, u8::MAX, 1)
    }

    pub fn choose_direction_avoiding_limited(
        &mut self,
        tail: &TailPolyline,
        arena: &ArenaConfig,
        obstacle_tails: &[&TailPolyline],
        max_turns: u8,
        window_ticks: u32,
    ) -> Direction {
        self.tick = self.tick.wrapping_add(1);
        let current = tail.front().1;
        let direction = self.choose_direction_avoiding_unlimited(tail, arena, obstacle_tails);
        if direction == current {
            return direction;
        }
        if self.try_consume_turn(max_turns, window_ticks.max(1)) {
            direction
        } else {
            current
        }
    }

    fn choose_direction_avoiding_unlimited(
        &mut self,
        tail: &TailPolyline,
        arena: &ArenaConfig,
        obstacle_tails: &[&TailPolyline],
    ) -> Direction {
        let current = tail.front().1;
        let current_safety = safety_distance(tail, obstacle_tails, current, arena);
        if let Some(direction) = boundary_avoidance_direction(tail.front().0, current, arena) {
            self.reset_decision_timer();
            if self.should_make_mistake() {
                return self.mistake_direction(current);
            }
            return safest_direction(
                tail,
                arena,
                obstacle_tails,
                &[direction, legal_turns(current).0, legal_turns(current).1],
            )
            .unwrap_or(direction);
        }

        if current_safety < DANGER_DISTANCE {
            self.reset_decision_timer();
            if self.should_make_mistake() {
                return self.mistake_direction(current);
            }
            return safest_direction(tail, arena, obstacle_tails, &candidate_directions(current))
                .unwrap_or(current);
        }

        if self.ticks_until_decision > 0 {
            self.ticks_until_decision -= 1;
            return current;
        }

        self.reset_decision_timer();
        if front_segment_length(tail) < MIN_SEGMENT_BEFORE_VOLUNTARY_TURN {
            return current;
        }

        let (left, right) = legal_turns(current);
        if self.should_make_mistake() {
            return self.mistake_direction(current);
        }
        let preferred = match self.next_u32() % 24 {
            0 => left,
            1 => right,
            _ => current,
        };
        if preferred == current
            || safety_distance(tail, obstacle_tails, preferred, arena) >= MIN_SAFE_TURN_DISTANCE
        {
            preferred
        } else {
            current
        }
    }

    fn try_consume_turn(&mut self, max_turns: u8, window_ticks: u32) -> bool {
        if max_turns == 0 {
            return false;
        }
        self.prune_turn_history(window_ticks);
        let max_turns = usize::from(max_turns).min(MAX_TRACKED_TURNS);
        if usize::from(self.recent_turn_count) >= max_turns {
            return false;
        }
        self.recent_turn_ticks[usize::from(self.recent_turn_count)] = self.tick;
        self.recent_turn_count += 1;
        true
    }

    fn prune_turn_history(&mut self, window_ticks: u32) {
        let cutoff = self.tick.saturating_sub(window_ticks);
        let mut kept = 0;
        for index in 0..usize::from(self.recent_turn_count) {
            let turn_tick = self.recent_turn_ticks[index];
            if turn_tick > cutoff {
                self.recent_turn_ticks[kept] = turn_tick;
                kept += 1;
            }
        }
        self.recent_turn_count = kept as u8;
    }

    fn reset_decision_timer(&mut self) {
        self.ticks_until_decision = self.decision_interval_ticks.saturating_sub(1);
    }

    fn should_make_mistake(&mut self) -> bool {
        let chance = u32::from(self.mistake_chance_per_decision_percent);
        chance > 0 && self.next_u32() % 100 < chance
    }

    fn mistake_direction(&mut self, current: Direction) -> Direction {
        let (left, right) = legal_turns(current);
        if self.next_u32() & 1 == 0 {
            left
        } else {
            right
        }
    }

    fn next_u32(&mut self) -> u32 {
        self.seed = self
            .seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.seed >> 32) as u32
    }
}

pub fn direction_to_input(direction: Direction) -> Vec2 {
    direction.delta()
}

pub fn legal_turns(direction: Direction) -> (Direction, Direction) {
    match direction {
        Direction::Up | Direction::Down => (Direction::Left, Direction::Right),
        Direction::Left | Direction::Right => (Direction::Down, Direction::Up),
    }
}

fn candidate_directions(current: Direction) -> [Direction; 3] {
    let (left, right) = legal_turns(current);
    [current, left, right]
}

fn safest_direction(
    tail: &TailPolyline,
    arena: &ArenaConfig,
    obstacle_tails: &[&TailPolyline],
    directions: &[Direction],
) -> Option<Direction> {
    directions.iter().copied().max_by(|a, b| {
        safety_distance(tail, obstacle_tails, *a, arena)
            .partial_cmp(&safety_distance(tail, obstacle_tails, *b, arena))
            .unwrap_or(std::cmp::Ordering::Equal)
    })
}

fn safety_distance(
    tail: &TailPolyline,
    obstacle_tails: &[&TailPolyline],
    direction: Direction,
    arena: &ArenaConfig,
) -> f32 {
    let position = tail.front().0;
    boundary_distance(position, direction, arena)
        .min(self_collision_distance(tail, direction))
        .min(obstacle_collision_distance(tail, obstacle_tails, direction))
}

fn boundary_distance(position: Vec2, direction: Direction, arena: &ArenaConfig) -> f32 {
    let half_width = arena.width * 0.5;
    let half_height = arena.height * 0.5;
    match direction {
        Direction::Right => half_width - position.x,
        Direction::Left => position.x + half_width,
        Direction::Up => half_height - position.y,
        Direction::Down => position.y + half_height,
    }
    .max(0.0)
}

fn self_collision_distance(tail: &TailPolyline, direction: Direction) -> f32 {
    let origin = tail.front().0 + direction.delta() * 0.01;
    tail_collision_distance(origin, direction, tail, 0.5)
}

fn obstacle_collision_distance(
    tail: &TailPolyline,
    obstacle_tails: &[&TailPolyline],
    direction: Direction,
) -> f32 {
    let origin = tail.front().0 + direction.delta() * 0.01;
    obstacle_tails
        .iter()
        .map(|obstacle_tail| {
            tail_collision_distance(origin, direction, obstacle_tail, -f32::EPSILON)
        })
        .fold(LOOKAHEAD_DISTANCE, f32::min)
}

fn tail_collision_distance(
    origin: Vec2,
    direction: Direction,
    tail: &TailPolyline,
    minimum_distance: f32,
) -> f32 {
    tail.pairs_front_to_back()
        .filter_map(|(segment_start, segment_end)| {
            ray_segment_intersection(
                origin,
                direction.delta(),
                LOOKAHEAD_DISTANCE,
                segment_start.0,
                segment_end.0,
            )
        })
        .filter(|distance| *distance > minimum_distance)
        .fold(LOOKAHEAD_DISTANCE, f32::min)
}

fn front_segment_length(tail: &TailPolyline) -> f32 {
    tail.0
        .get(1)
        .map(|(next, _)| tail.front().0.distance(*next))
        .unwrap_or(f32::INFINITY)
}

fn boundary_avoidance_direction(
    position: Vec2,
    current: Direction,
    arena: &ArenaConfig,
) -> Option<Direction> {
    let half_width = arena.width * 0.5;
    let half_height = arena.height * 0.5;
    let margin = boundary_margin(arena);

    match current {
        Direction::Right if position.x >= half_width - margin => {
            Some(vertical_toward_center(position, half_height, margin))
        }
        Direction::Left if position.x <= -half_width + margin => {
            Some(vertical_toward_center(position, half_height, margin))
        }
        Direction::Up if position.y >= half_height - margin => {
            Some(horizontal_toward_center(position, half_width, margin))
        }
        Direction::Down if position.y <= -half_height + margin => {
            Some(horizontal_toward_center(position, half_width, margin))
        }
        _ => None,
    }
}

fn boundary_margin(arena: &ArenaConfig) -> f32 {
    (arena.width.min(arena.height) * 0.08).clamp(30.0, 150.0)
}

fn vertical_toward_center(position: Vec2, half_height: f32, margin: f32) -> Direction {
    if position.y >= half_height - margin {
        Direction::Down
    } else if position.y <= -half_height + margin {
        Direction::Up
    } else if position.y >= 0.0 {
        Direction::Down
    } else {
        Direction::Up
    }
}

fn horizontal_toward_center(position: Vec2, half_width: f32, margin: f32) -> Direction {
    if position.x >= half_width - margin {
        Direction::Left
    } else if position.x <= -half_width + margin {
        Direction::Right
    } else if position.x >= 0.0 {
        Direction::Left
    } else {
        Direction::Right
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    fn tail(position: Vec2, direction: Direction) -> TailPolyline {
        TailPolyline::new(VecDeque::from([
            (position, direction),
            (position - direction.delta() * 100.0, direction),
        ]))
    }

    #[test]
    fn legal_turns_never_reverse() {
        assert_eq!(
            legal_turns(Direction::Up),
            (Direction::Left, Direction::Right)
        );
        assert_eq!(
            legal_turns(Direction::Down),
            (Direction::Left, Direction::Right)
        );
        assert_eq!(
            legal_turns(Direction::Left),
            (Direction::Down, Direction::Up)
        );
        assert_eq!(
            legal_turns(Direction::Right),
            (Direction::Down, Direction::Up)
        );
    }

    #[test]
    fn boundary_avoidance_turns_inward() {
        let arena = ArenaConfig {
            width: 200.0,
            height: 100.0,
        };

        assert_eq!(
            boundary_avoidance_direction(Vec2::new(98.0, 10.0), Direction::Right, &arena),
            Some(Direction::Down)
        );
        assert_eq!(
            boundary_avoidance_direction(Vec2::new(-98.0, -10.0), Direction::Left, &arena),
            Some(Direction::Up)
        );
        assert_eq!(
            boundary_avoidance_direction(Vec2::new(-10.0, 48.0), Direction::Up, &arena),
            Some(Direction::Right)
        );
        assert_eq!(
            boundary_avoidance_direction(Vec2::new(10.0, -48.0), Direction::Down, &arena),
            Some(Direction::Left)
        );
    }

    #[test]
    fn bot_decisions_are_deterministic_for_same_seed() {
        let arena = ArenaConfig {
            width: 500.0,
            height: 500.0,
        };
        let tail = tail(Vec2::ZERO, Direction::Up);
        let mut first = BotController::new(1, 42);
        let mut second = BotController::new(1, 42);

        let first_choices = (0..10)
            .map(|_| first.choose_direction(&tail, &arena))
            .collect::<Vec<_>>();
        let second_choices = (0..10)
            .map(|_| second.choose_direction(&tail, &arena))
            .collect::<Vec<_>>();

        assert_eq!(first_choices, second_choices);
    }

    #[test]
    fn bot_keeps_short_segments_in_open_space() {
        let arena = ArenaConfig {
            width: 500.0,
            height: 500.0,
        };
        let tail = TailPolyline::new(VecDeque::from([
            (Vec2::new(10.0, 0.0), Direction::Right),
            (Vec2::ZERO, Direction::Right),
            (Vec2::new(0.0, -100.0), Direction::Up),
        ]));
        let mut bot = BotController::new(1, 1);

        assert_eq!(bot.choose_direction(&tail, &arena), Direction::Right);
    }

    #[test]
    fn bot_turns_away_from_self_collision() {
        let arena = ArenaConfig {
            width: 500.0,
            height: 500.0,
        };
        let tail = TailPolyline::new(VecDeque::from([
            (Vec2::ZERO, Direction::Up),
            (Vec2::new(0.0, -50.0), Direction::Up),
            (Vec2::new(50.0, -50.0), Direction::Left),
            (Vec2::new(50.0, 20.0), Direction::Down),
            (Vec2::new(-50.0, 20.0), Direction::Right),
        ]));
        let mut bot = BotController::new(1, 1);

        assert_ne!(bot.choose_direction(&tail, &arena), Direction::Up);
    }

    #[test]
    fn bot_turns_away_from_other_tail_collision() {
        let arena = ArenaConfig {
            width: 500.0,
            height: 500.0,
        };
        let own_tail = tail(Vec2::ZERO, Direction::Up);
        let obstacle_tail = TailPolyline::new(VecDeque::from([
            (Vec2::new(50.0, 20.0), Direction::Right),
            (Vec2::new(-50.0, 20.0), Direction::Right),
        ]));
        let mut bot = BotController::new(1, 1);

        assert_ne!(
            bot.choose_direction_avoiding(&own_tail, &arena, &[&obstacle_tail]),
            Direction::Up
        );
    }

    #[test]
    fn bot_can_be_configured_to_make_mistakes() {
        let arena = ArenaConfig {
            width: 500.0,
            height: 500.0,
        };
        let tail = TailPolyline::new(VecDeque::from([
            (Vec2::ZERO, Direction::Up),
            (Vec2::new(0.0, -200.0), Direction::Up),
        ]));
        let mut bot = BotController::new_with_mistakes(1, 1, 100);

        assert_ne!(bot.choose_direction(&tail, &arena), Direction::Up);
    }

    #[test]
    fn bot_turn_budget_limits_turns_within_window() {
        let arena = ArenaConfig {
            width: 200.0,
            height: 100.0,
        };
        let tail = tail(Vec2::new(98.0, 10.0), Direction::Right);
        let mut bot = BotController::new(1, 1);

        assert_ne!(
            bot.choose_direction_avoiding_limited(&tail, &arena, &[], 2, 4),
            Direction::Right
        );
        assert_ne!(
            bot.choose_direction_avoiding_limited(&tail, &arena, &[], 2, 4),
            Direction::Right
        );
        assert_eq!(
            bot.choose_direction_avoiding_limited(&tail, &arena, &[], 2, 4),
            Direction::Right
        );
        assert_eq!(
            bot.choose_direction_avoiding_limited(&tail, &arena, &[], 2, 4),
            Direction::Right
        );
        assert_ne!(
            bot.choose_direction_avoiding_limited(&tail, &arena, &[], 2, 4),
            Direction::Right
        );
    }

    #[test]
    fn direction_to_input_matches_protocol_direction() {
        assert_eq!(direction_to_input(Direction::Up), Vec2::Y);
        assert_eq!(direction_to_input(Direction::Down), -Vec2::Y);
        assert_eq!(direction_to_input(Direction::Right), Vec2::X);
        assert_eq!(direction_to_input(Direction::Left), -Vec2::X);
    }
}
