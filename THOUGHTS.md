---
date: 2026-05-22
topic: "Lightrider project design, implementation plan, and agent notes"
tags: [lightrider, bevy, lightyear, game-design, multiplayer]
status: active
---

# Lightrider Thoughts

This is the durable project document for Lightrider. Keep it up to date whenever code, architecture, tooling, gameplay rules, deployment assumptions, or debugging knowledge changes.

## Game Design Document

### Product Goal

Lightrider is a Rust, Bevy, and Lightyear clone of the online io game Powerline.io. The first milestone is a fully working online multiplayer prototype with intentionally barebones graphics. Cosmetics such as animation polish, sound, music, textures, font work, and richer colors come after the prototype proves that the gameplay and networking hold up when deployed online.

The game should stay as close as practical to Powerline.io. Use `/spare/ssd/cbournhonesque/src/other/snakegame/original/powerline.io` as inspiration for behavior, tuning, UI ideas, and later asset extraction. The early prototype should prefer readable primitives over asset fidelity.

### Core Gameplay

- Players control a continuously moving light snake on an orthogonal grid-like plane.
- A snake is represented as a head plus a tail polyline. Direction changes add turn points; the tail follows the head path and is shortened from the back to maintain length.
- Players steer up, down, left, and right. A 180-degree reversal is invalid.
- Hitting another snake, hitting yourself, or hitting the arena boundary kills the snake.
- Food spawns in the arena as very small dots. Nearby food is magnetically pulled toward close snake heads and is absorbed at the head.
- Eating food grows the snake and increases score.
- Score starts at 0 and counts growth beyond the initial snake length. The starting tail length is not part of the score.
- When a snake dies, the server drops food samples along the killed snake's trajectory.
- The Powerline signature mechanic is proximity speed boost: a snake accelerates when moving close and parallel enough to another snake trail, then decelerates back toward base speed when not close.
- The prototype should support respawning after death. The server enforces a short respawn cooldown; clients request respawn through the normal networked input action.
- Bots should use the same movement, collision, food, and scoring rules as players.

### Prototype Rendering

- Start with Bevy primitives and gizmos: dark arena, visible boundaries, simple line tails, simple square/circle heads, simple food dots.
- Make gameplay state legible before matching cosmetics.
- Keep rendering replaceable so later asset imports from the original Powerline source do not disturb simulation or networking logic.
- Later cosmetic backlog: glow, sparks on proximity boost, richer death effects, richer food pickup animation, sound effects from `sounds/out.ogg`, sprites from `images/sheet.png`, logo/menu assets, minimap, leaderboard styling.

### Arena And Rules

- Original Powerline client constants of interest:
  - `GAME_SCALE = 10.0`
  - `UPDATE_EVERY_N_TICKS = 3`
  - `INTERP_TIME = (1000 / 30) * UPDATE_EVERY_N_TICKS`
  - Default client-side arena values include `arenaWidth = 5000.0`, `arenaHeight = 1600.0`, centered at `(0.0, 0.0)`.
  - Kill reasons include left screen, killed, boundary, and suicide.
- Current default config uses the Powerline-like `5000.0 x 1600.0` arena. `config/test.ron` uses an `800.0 x 600.0` arena for faster local testing.
- Initial server prototype can use one arena per room. Room size, max players, bot count, and food count should come from data-driven config.
- Keep one default config that is as close as practical to original Powerline behavior. Most values should feel similar to the original, but exact replication is not required when the original uses esoteric or ad-hoc logic.
- Add smaller test configs, for example a much smaller arena for local collision, room, bot, and fake-client testing.

### Configuration

- Use data-driven config for gameplay and server settings. RON is a good initial format unless implementation shows a better fit.
- Maintain a `default` config tuned toward Powerline-like behavior.
- Maintain at least one `test` config with a small arena and low limits for quick local iteration.
- Config should cover arena dimensions, tick rate, spawn rules, respawn cooldowns, starting length, speed/boost tuning, food count, room capacity, bot count, fake-client defaults, render/debug camera knobs, and network/debug knobs.
- Game logic should consume typed config resources rather than scattering constants through systems.

### Multiplayer And Rooms

- The game will be served online through Edgegap.
- Users can join a random room or create a new room.
- A single server process should support multiple game rooms through the room system.
- The authoritative server owns room simulation, player spawning, bot spawning, food spawning, collision, scoring, and death.
- Clients render the world and send input. They do not decide kills, food pickups, or score.
- Room assignment can start as a simple service-level route or CLI/config field, then become production matchmaking later.
- Initial target room limit: 50 players per room, matching the intended scale of the original Powerline.io.

### Networking Model

- Use Bevy `0.18` and Lightyear from the `main` branch of `https://github.com/cBournhonesque/lightyear.git`. Cargo may still report Lightyear's crate version as `0.26.4`; the source of truth is the git dependency and commit in `Cargo.lock`.
- Use WebTransport as the only supported transport. UDP/WebSocket code paths were removed during Phase 2 to keep deployment and debugging focused.
- Client prediction is enabled only for the local player's snake.
- Other player snakes are interpolated, not predicted.
- The server remains authoritative and reconciles predicted local state.
- Movement is predicted, but deaths are server-authoritative. Clients may show provisional feedback later, but the server decides kill validity, kill reason, respawn state, and score effects.
- Inputs should be compact directional actions. Server-side simulation should consume the same input semantics as client prediction.
- Use Lightyear replication and interpolation APIs for snake state, especially tail interpolation. The old Lightyear `replication_groups` example remains useful as interpolation reference material, but main branch no longer exposes the old `ReplicationGroup` component used by earlier Lightrider code.
- Bots controlled by the server should use the same shared simulation code as real clients where possible, but they do not need network round trips.
- Fake clients should be able to connect to the server and send random or scripted inputs using the same protocol as real clients. Use them for latency/load tests and online deployment validation.
- Add artificial latency, jitter, loss, and client count controls for local and deployed tests.

### Architecture Goals

- Keep a Cargo workspace.
- Target top-level organization is four main folders/crates:
  - `client`: client-specific app logic, input sources, connection flow, local prediction setup, client modes, and headless/fake-client behavior.
  - `server`: server-specific authoritative simulation orchestration, room lifecycle, Edgegap-facing runtime config, production logging, and server-owned bots.
  - `render`: rendering-specific logic, camera, UI, debug views, visual interpolation display, and cosmetic systems.
  - `shared`: network protocol, deterministic movement, collision geometry, room-safe components, config types, simulation rules, and bot decision helpers reusable by server bots and fake clients.
- Client code should be able to run without rendering so a headless client can act autonomously or as a fake client.
- Bot logic likely belongs in `shared` when it can be reused by both server-owned bots and fake clients. Server-specific bot orchestration stays in `server`.
- Server crate owns authoritative simulation orchestration, room lifecycle, bot orchestration, fake-client harness entry points if kept in-process, Edgegap-facing config, and production logging.
- Do not use Avian. There is little physics; prefer explicit geometry for ray/segment collision, proximity boost detection, arena boundary tests, and food overlap.
- Do not reintroduce `bevy_xpbd_2d`/Avian-style physics for core gameplay. Collision, friction/proximity, and food overlap are now explicit geometry and should stay small and testable.
- Rust modules with nesting should use `a.rs` and `a/b.rs`, not `mod.rs`, when adding or reorganizing modules.
- Add focused unit tests for complicated logic: tail shortening, tail interpolation, turn validation, collision cases, proximity boost, food spawn constraints, room assignment, and bot/fake-client input generation.

### Observability And Agent-Friendly Tooling

- Add CLI flags for server mode, room count, bots per room, fake clients, listen address, tick rate, artificial network conditions, deterministic RNG seed, and WebTransport certificate digest handling.
- Add structured tracing spans for room id, player id, snake entity, tick, input sequence, prediction corrections, collisions, food pickups, and despawns.
- Add a debug snapshot command or endpoint that dumps room state as JSON: players, snakes, tail points, score, food count, bot state, tick, network stats.
- Add deterministic replay fixtures for simulation bugs: seed plus input stream should reproduce movement/collision outcomes.
- Investigate and use `lightyear_debug` for network debugging. It can store data in JSON form for later analysis with DuckDB, which fits the goal of agent-inspectable networking and prediction diagnostics.
- AI agents should be able to autonomously run a headless server plus multiple headless bot clients, capture structured `lightyear_debug` JSONL logs, load them into DuckDB, and identify simulation/replication bugs without requiring manual visual inspection.
- Runtime correctness probes should sample snake heads at `FixedUpdate` after movement, `FixedLast`, `PostUpdate` after frame interpolation, and `Last`, including tick, role, room, player id, prediction/interpolation/control markers, speed, acceleration, and tail length fields.
- Keep protocol and simulation docs close to code and link them from this file.

## Current Repo Snapshot

- Workspace members today: `client`, `server`, `shared`. Target organization still adds a separate `render` folder/crate so rendering can be excluded from headless client modes.
- Dependency state: Bevy `0.18.1`, Lightyear from `https://github.com/cBournhonesque/lightyear.git` branch `main` at the commit locked in `Cargo.lock` (currently `64c71437`), `bevy_enhanced_input = "0.24.4"` aligned with Lightyear's `input_bei` integration, `bevy_turborand = "0.13"`, `bevy-inspector-egui = "0.36"`, and no direct physics-engine dependency. Bevy `dynamic_linking` is disabled because it broke test-binary links through `bevy_dylib` on this machine.
- Lightyear transport features are WebTransport-only: `webtransport`, `webtransport_self_signed`, and `webtransport_dangerous_configuration`. `udp` and `websocket` features are intentionally not enabled.
- Leafwing input usage has been removed from the active dependency tree. Keep the direct `bevy_enhanced_input` dependency aligned with the version pulled by Lightyear main's `input_bei` integration, otherwise the project gets two incompatible BEI action APIs.
- `.cargo/config.toml` no longer forces an Apple target; Linux workspace checks/tests now run on the host target.
- Data-driven config exists in `shared/src/config.rs` with RON files at `config/default.ron` and `config/test.ron`. It now covers movement, food, rooms, respawn, network/input delay, debug tracing, and barebones render knobs such as tail width, head size, map outline width, close-camera growth, and debug camera scale. `MovementConfig::tick_duration()` is the single source for the Lightyear fixed tick duration.
- Shared logic has a polyline snake model in `shared/src/network/protocol/components/snake.rs`.
- Movement logic lives in `shared/src/movement/mod.rs` and includes BEI-driven turn handling, config-driven acceleration/speed integration, food-pickup acceleration boost decay, and tail shortening.
- Collision/proximity now uses explicit geometry in `shared/src/utils/geometry.rs`, `shared/src/collision/collider.rs`, and `server/src/collision/collider.rs`. Simulation-critical proximity, server collision/death, and food overlap/growth systems run in `FixedUpdate` around `SimulationSet::Movement`.
- Shared `RoomId` is registered in the protocol and attached to maps, players, snakes, and food. Proximity boost, snake-vs-snake collision, food overlap, respawns, rank computation, and Lightyear room visibility are room-scoped.
- Server boundary death now uses explicit arena geometry from `GameConfig`.
- Server food spawning, magnetic attraction, overlap, growth, and death drops use `GameConfig` for target count, spawn interval, visual radius, absorption radius, magnet radius/speed, tail growth, and death-food spacing/limit. Food is replicated through room visibility and `Position` is interpolated for smoother magnetic movement.
- `PlayerScore`, `PlayerRank`, and `PlayerStatus` replicate with the player. Food pickups update score from tail growth beyond the initial length on the server, per-room ranks are recomputed on the server, and respawns reset score/status from config.
- Death flow now uses server-internal `SnakeCollision` with `DeathReason` and sends a server-to-client `PlayerDeath` message containing mapped player/snake entities, room id, and reason. The server remains authoritative for despawning snakes, setting player status, clearing `Player.snake`, and enforcing respawn cooldown through `RespawnReadyAt`.
- Client death state records the local player's death view: after a collision death, the follow camera tracks the killer snake if it still exists; after suicide or boundary death, the camera stays static. Respawn is requested with Enter or Space after the configured cooldown, and the server validates the timing.
- Client prediction now includes the shared collision/proximity plugin, so local predicted snakes can compute boost from nearby room-local trails before reconciliation.
- Lightyear protocol registration now uses the 0.26 explicit plugin style in `shared/src/network/protocol/mod.rs`.
- Networked inputs use Lightyear's BEI input plugin with replicated action entities. Client setup keys off Lightyear's `Controlled` marker and no longer uses a local `Owned` marker. `SpawnPlayer` is bound to Enter and Space.
- Client/server connection setup now uses Lightyear 0.26 entity components (`ClientPlugins`, `ServerPlugins`, `NetcodeClient`, `NetcodeServer`, WebTransport IO components) rather than old `ClientPlugin<GameProtocol>` / `ServerPlugin<GameProtocol>` config objects. Both client and server derive Lightyear's tick duration from `GameConfig`.
- Client CLI accepts `--certificate-digest <hex>` for WebTransport. Native dev builds can leave it empty because the local dev feature set enables dangerous certificate handling; browser/deployment paths should pass the digest printed by the server.
- Client and server CLIs accept `--config <path>` to load a RON `GameConfig`; without it they use `GameConfig::default()`.
- Non-headless client debug mode is enabled with `--debug` or the existing `--inspector` alias. In debug mode `T` toggles between normal close camera scale and the zoomed-out debug camera scale, and `?` toggles a local shortcuts overlay. The normal camera starts much closer to the head and grows outward with current tail growth up to a configured max scale.
- Existing client rendering is very barebones and uses gizmos in `client/src/render/snake.rs`.
- Custom snake interpolation has been restored against Lightyear 0.26 using `ConfirmedHistory<TailPoints>`, `ConfirmedHistory<TailLength>`, and `InterpolationSystems::Interpolate`. The helper is adapted from the old Lightrider prototype and the Lightyear `replication_groups` example.
- Frame interpolation is enabled for predicted snake `TailPoints` via `FrameInterpolationPlugin<TailPoints>` and `FrameInterpolate<TailPoints>`. Visual correction is intentionally not configured.
- Lightyear room visibility is the interest-management filter for game entities, but it is not a replacement for normal replication targets. On Lightyear main, room membership uses the `Rooms` component with ids from `RoomAllocator`; room-scoped entities still use `Replicate::to_clients(NetworkTarget::All)`, and food spawning pauses while any `ClientOf` lacks a `ReplicationSender` to avoid pending-handshake sender errors.
- Non-headless clients must spawn `ReplicationReceiver::default()` on the client entity; otherwise WebTransport connects but replicated game entities never arrive. Add Lightyear `ClientPlugins`/`ServerPlugins` before the shared `ProtocolPlugin`, matching the upstream examples.
- The client render baseline now draws a dark clear color, arena outline/axes, small food dots, and configurable snake head/tail gizmos so a connected or not-yet-connected window is visibly alive.
- Server-owned bots live in `server/src/bots.rs` and reuse shared bot steering from `shared/src/bot.rs`. Bot clients are `client --headless --mode bot` and drive BEI `ActionMock`s through the same input path as real clients. Bot steering now scores arena boundaries, its own tail, and same-room snake tails as lookahead obstacles; random voluntary turns are strongly throttled so bots do not immediately draw tiny self-trapping boxes.
- Server respawning uses safer spawn placement from `server/src/spawning.rs`, scoring candidate head/tail positions against same-room tails and arena bounds before falling back to the best deterministic candidate.
- Runnable binaries are explicitly named `lightrider-server` and `lightrider-client`; do not rely on both crates exposing a bin named `main`.
- A root `justfile` exists with `server`, `client`, `client-debug`, `bot`, `bots`, and `local` recipes for starting a local headless server, debug/normal clients, and headless bot clients.
- Runtime debug tracing lives in `shared/src/debug.rs`. When `GameConfig.debug.lightyear_debug` is true and `LIGHTYEAR_DEBUG_FILE` is set, client/server `LogPlugin`s install a Lightyear-compatible JSONL layer for `lightyear_debug::*` tracing targets and enable `lightyear_debug=trace`; `json_snapshots` emits `snake_head` rows and `snake_invariant_violation` rows for DuckDB analysis.
- `just trace-local` builds the binaries once, runs a headless WebTransport server plus multiple staggered headless bot clients, writes per-process `.ndjson` and `.log` files under `logs/debug/<timestamp>/`, and runs `tools/debug_trace_summary.sql` through DuckDB. `just trace-summary dir=...` reruns the summary on an existing trace directory.
- Current verification after switching to Lightyear main: `cargo check --workspace -j 4` passes, `cargo test --workspace -j 4` passes with client 1 test, server 20 tests, shared 25 tests, and `just trace-local 2 6 config/test.ron 5053` produced 14,946 `snake_head` rows, zero invariant violations, and no panic/error/replication-sender markers.

## Implementation Plan

### Phase 0: Baseline And Dependency Upgrade

Status: complete enough to move to Phase 1.

1. Recorded the current behavior and test baseline before major edits.
2. Upgraded the workspace to Bevy `0.18.1` and Lightyear main. Cargo still shows Lightyear's package version as `0.26.4`, but the dependency is git branch `main`.
3. Introduced typed data-driven RON config loading with `default` and `test` configs.
4. Kept the current `client`, `server`, `shared` workspace shape; the separate `render` crate/folder remains a later structural task.
5. Removed direct `bevy_xpbd_2d` usage and replaced spatial-query gameplay checks with explicit geometry.
6. Restored `cargo test --workspace -j 4`.
7. Smoke-started the headless server over UDP during the early upgrade work; current server smoke is WebTransport-only.

### Phase 1: Deterministic Shared Simulation

Status: complete enough to move to Phase 2.

1. Define shared config resources for tick rate, arena size, spawn length, speed range, boost distance, food radius, and room limits. Initial typed resources exist.
2. Wire all movement constants to `GameConfig`. Spawn length, min/max speed, base acceleration, boost ratio, boost distance, and Lightyear fixed tick duration are wired.
3. Move boundary collision to explicit geometry. Server-authoritative arena boundary death is implemented and tested.
4. Initial proximity boost tuning is implemented with config-driven values. Exact original Powerline feel remains a later tuning pass after multiplayer is running.
5. Add unit tests for boundary collision, room-scoped collision, and more proximity boost cases. Boundary, boost helper, room-scoped proximity, room-scoped snake collision, and room-scoped food overlap tests exist.
6. Simulation-critical movement, proximity, server collision/death, and food overlap/growth run on fixed tick schedules.
7. Split rendering state from authoritative simulation state where useful. The larger render crate/folder split remains future work.

### Phase 2: Lightyear Protocol And Prediction

Status: complete enough to move to Phase 3/4 validation.

1. Protocol registration has been rebuilt against the upgraded Lightyear API.
2. Runtime replication shape now covers player identity, snake state, tail points, food, room id, score, player status, and death messages.
3. Score and room metadata replication are in place through `PlayerScore`, `PlayerStatus`, and `RoomId`.
4. WebTransport is the only supported transport. UDP/WebSocket code paths and Lightyear features were removed.
5. Predict only the local player's movement. Server snake bundles use `PredictionTarget::Single(owner)` and `InterpolationTarget::AllExceptSingle(owner)`.
6. Keep deaths, kill reasons, food pickups, scoring, and respawn state server-authoritative. `SnakeCollision` is server-internal; clients receive `PlayerDeath`.
7. Remote snake visual interpolation is restored with a custom `ConfirmedHistory` system using the tail path, and fixed-tick frame interpolation is enabled for predicted snake `TailPoints`.
8. Visual correction is intentionally not enabled. Connected WebTransport validation now exists through the headless bot-client smoke; `lightyear_debug` capture remains an observability task.

### Phase 3: Authoritative Server Rooms

Status: complete enough to move to deployable prototype work.

1. Room entities/resources exist in `server/src/rooms.rs`, and clients can auto-join, create a new room, or request a numeric room id through `RoomJoinRequest`.
2. Room config is data-driven through `GameConfig`: max rooms, max players per room, arena size, food target, and bot target counts are all loaded from RON.
3. Replication, collision, food, proximity, and ranking are room-scoped. Game entities rely on Lightyear room visibility, not global `NetworkTarget::All` replication.
4. Respawn flow preserves the player's current room and remains server-authoritative. Death state is tracked through replicated `PlayerStatus`.
5. Minimal leaderboard/rank state exists as replicated `PlayerRank`, computed per room from `PlayerScore`.

### Phase 4: Bots And Fake Clients

Status: complete enough for local load/latency smoke work.

1. Server-owned bots choose legal turns with shared deterministic steering and use the same movement, collision, food, score, death, and respawn systems as players. They avoid walls, their own tail, and same-room snake trails using conservative lookahead, and respawn after the configured bot cooldown.
2. Fake clients are implemented as the normal client binary with `--headless --mode bot`; they connect over WebTransport and send BEI movement/respawn actions through the real protocol. They use the same shared obstacle-aware steering when replicated tails are available.
3. Local test modes are available through `just bot`, `just bots`, and `just local`. Bot ids are deterministic CLI values; server bot ids start at `PeerId::Netcode(10000)`.
4. Initial operational signals are connection logs, room assignment logs, replicated score/rank/status, and smoke-test log scanning. Detailed bytes/correction metrics remain observability work.

### Phase 5: Deployable Prototype

1. Add container/build scripts for the headless server and web/native client path selected for first deployment.
2. Add Edgegap configuration and runtime env handling.
3. Run local soak tests with fake clients before deploying.
4. Deploy to Edgegap and test random room join, created room join, latency behavior, prediction quality, and server resource usage.

### Phase 6: Cosmetics And Original Asset Parity

1. Import and verify original assets only after gameplay/networking are stable.
2. Add visual polish incrementally: glow, boost sparks, food animation, death animation, minimap, leaderboard, menus, fonts, and sound.
3. Keep each cosmetic feature optional or isolated from simulation logic.

## General Notes For AI Agents

- Read this file first before changing code.
- After every meaningful change, update the changelog and any stale design/plan notes here.
- Prefer `rg` and `rg --files` for code search.
- Use the original Powerline source as behavioral reference, not as code to copy directly.
- Keep code clean and maintainable over matching old JavaScript structure.
- Preserve user changes in the worktree. Do not revert unrelated dirty files.
- For Rust work, run focused tests first, then broader `cargo test` when the change touches shared behavior.
- Limit cargo jobs on this machine if builds are heavy.
- When changing networking behavior, add enough tracing for another agent to inspect what happened from logs.
- When changing simulation behavior, add unit tests in `shared` where possible.
- Avoid introducing physics engines. Geometry for this game should remain small, explicit, and testable.
- Keep bots and fake clients separate concepts: bots are server-owned simulated players; fake clients are network clients used for load and latency testing.
- Keep client prediction scoped to the owner. Do not predict other clients.
- Keep server authority for kills, food pickups, score, and room lifecycle.

## Open Questions

- Deployment packaging is still open. Once a prototype exists, choose the easiest working path. Embedding client/static assets in the server binary is acceptable if it simplifies deployment.
- Exact default config values still need to be derived from the original Powerline behavior and adjusted by feel.
- Decide whether `render` should be a separate workspace crate immediately or first a folder/module split inside the client while the upgrade is in flight.

## Update / Changelog

### 2026-05-26

- Tightened normal camera behavior: `normal_camera_scale` is now close to the head by default, `debug_camera_scale` matches the previous far normal view, and the normal camera grows with current tail growth using `normal_camera_growth_per_tail_length` up to `normal_camera_max_scale`.
- Made food smaller and added server-authoritative magnetic attraction. Food now has separate `visual_radius`, absorption `radius`, `magnet_radius`, and `magnet_speed` config fields; the server moves nearby food toward the closest same-room snake head before pickup checks, and `Position` is interpolated for food so the pull is visible on clients.
- Changed score semantics so initial tail length is worth 0 points. Respawns and initial spawns reset `PlayerScore` to default; food pickups use `PlayerScore::from_tail_length(total_tail_length, starting_tail_length)`.
- Added death food drops. Before despawning a killed snake, the server samples positions along its `TailPoints` using `death_food_spacing` and `death_food_max` and spawns room-scoped replicated food at those positions.
- Verification for close-camera/magnetic-food/score/death-food changes: `cargo fmt --all`, `CARGO_INCREMENTAL=0 cargo check --workspace -j 4`, `CARGO_INCREMENTAL=0 cargo test --workspace --lib -j 4`, and `CARGO_INCREMENTAL=0 just trace-local 1 4 config/test.ron 5064` pass/complete. The trace at `logs/debug/20260526-203810` showed zero `snake_invariant_violation` and zero `server_late_input_mismatch` rows, no panic/error markers, and several server food pickup logs.
- Added data-driven render knobs to `GameConfig`: snake tail width, head size, map outline width, normal camera scale/growth/max, and debug camera scale. The default/test configs now use a narrower `tail_width = 3.0` and draw a configurable map outline.
- Added local client debug mode through `--debug` plus the existing `--inspector` alias. Debug clients spawn local-only shortcuts: `T` toggles between the normal close camera and zoomed-out debug camera, while `?` toggles an on-screen shortcut help panel. Added `just client-debug` as the convenient launch recipe.
- Food pickups now add a short acceleration burst through a replicated/predicted `FoodBoost` component instead of directly changing speed. `shared::movement` layers food boost over either the current proximity acceleration or the base deceleration and then decays it each fixed tick, so snakes keep momentum briefly and then settle back toward the configured minimum speed through the existing negative base acceleration.
- Verification for render/debug-camera/food-boost changes: `cargo fmt --all`, `CARGO_INCREMENTAL=0 cargo check --workspace -j 4`, `CARGO_INCREMENTAL=0 cargo test --workspace --lib -j 4`, `target/debug/lightrider-client --help`, `just --list`, and `CARGO_INCREMENTAL=0 just trace-local 1 4 config/test.ron 5063` pass/complete. DuckDB showed zero `snake_invariant_violation` and zero `server_late_input_mismatch` rows for `logs/debug/20260526-201421`, and the logs had no panic/error markers.
- Fixed a Lightyear-main runtime regression where authoritative server snakes were not moving. Main now adds replication/interpolation marker components to source entities, so the old shared `Simulated` filter excluded server-owned snakes. Added a local `SimulationAuthority` marker to server snake sources and updated the simulation filter to include it while still excluding remote interpolated client copies.
- Added regression tests for this marker behavior: authoritative replicated/interpolated snakes move, remote interpolated copies do not, and replicated authoritative snakes can move into and ingest food.
- Added data-driven client input-delay settings under `network.input_delay` in the RON configs. The current default is minimum input delay 0 ticks, maximum delay before prediction 3 ticks, maximum prediction 7 ticks. At the current 30 Hz simulation rate that means no forced baseline input delay, up to about 100 ms of adaptive input delay before prediction, and up to about 233 ms of prediction. Also made input packet redundancy data-driven through `network.input_packet_redundancy_ticks`; the current default is 3 ticks so the initial input packet does not include already-simulated ticks in local smoke tests.
- Camera follow now runs after `FrameInterpolationSystems::Interpolate`, so the follow target uses the same frame-interpolated `TailPoints` that snake rendering sees.
- Added a barebones HUD render module. It displays a top-right leaderboard capped at 10 rows by combining the top 5 room players with up to 5 rows around the local player's rank, and a bottom-right minimap showing the local player and current room leader positions.
- Added player names and name labels. Human clients now send a sanitized name update after connecting, defaulting to `Player <client_id>` when no `--name` is provided; bot-mode clients default to `Bot Client <client_id>`. Server-created bots keep their `Bot N` names. The client renders `Text2d` name labels next to each alive snake head after frame interpolation, and the leaderboard uses the replicated `Player.name`.
- Added replicated per-life `PlayerStats` for average speed, time alive, kills, time as leader, and food eaten. The server updates stats authoritatively, increments killer kills on collision deaths, increments food eaten on food pickup, and resets life stats on respawn.
- Expanded `PlayerDeath` with killed/killer names plus a death-stat snapshot. The local death overlay now displays `Killed by X` for deaths caused by another player and shows score, average speed, time alive, kills, time as leader, food eaten, and respawn countdown.
- Added data-driven bot imperfection through `bots.mistake_chance_per_decision_percent` and `fake_clients.mistake_chance_per_decision_percent`. Server bots and bot-mode clients still avoid imminent collisions most of the time, but can occasionally make a bad turn so long-running rooms are not perfectly stable.
- Expanded `tools/debug_trace_summary.sql` with `snake_movement_by_entity` and `stationary_server_snakes` sections so frozen authoritative simulation is visible in DuckDB summaries instead of only counting `snake_head` rows.
- Verification for name/stat/death-HUD changes: `CARGO_INCREMENTAL=0 cargo check --workspace -j 4`, `CARGO_INCREMENTAL=0 cargo test --workspace --lib -j 4`, and `CARGO_INCREMENTAL=0 just trace-local 1 5 config/test.ron 5061` pass/complete. The trace emitted moving server bot/player rows, zero `stationary_server_snakes`, zero invariant violations, and zero `server_late_input_mismatch` rows.
- Verification for camera/HUD/bot-risk changes: `CARGO_INCREMENTAL=0 cargo check --workspace -j 4`, `CARGO_INCREMENTAL=0 cargo test -p shared --lib bot::tests -j 4`, `CARGO_INCREMENTAL=0 cargo test -p client --lib render::hud::tests -j 4`, `CARGO_INCREMENTAL=0 cargo test -p shared --lib config::tests -j 4`, and `CARGO_INCREMENTAL=0 just trace-local 1 5 config/test.ron 5060` pass/complete. The trace emitted moving server bot/player rows, zero `stationary_server_snakes`, zero invariant violations, and zero `server_late_input_mismatch` rows.
- Verification: `CARGO_INCREMENTAL=0 cargo check --workspace -j 4`, `CARGO_INCREMENTAL=0 cargo test -p shared --lib config::tests::default_config_file_matches_expected_powerline_shape -j 1`, and `CARGO_INCREMENTAL=0 just trace-local 1 3 config/test.ron 5058` pass/complete. The trace emitted moving server bot/player rows, zero `stationary_server_snakes`, zero invariant violations, and zero `server_late_input_mismatch` rows. Note: the local filesystem was full during testing; removing generated Cargo incremental artifacts freed enough space to continue.
- Switched the workspace Lightyear dependency from crates.io `0.26.4` to `https://github.com/cBournhonesque/lightyear.git` branch `main`, currently locked to commit `64c71437` in `Cargo.lock`.
- Aligned the direct `bevy_enhanced_input` dependency to `0.24.4`, matching Lightyear main's `input_bei` integration, and replaced deprecated `ActionState` usage with `TriggerState`.
- Adapted to Lightyear main's room-visibility API: the server now uses `RoomAllocator` plus `Rooms::single(...)` membership components instead of the removed `Room`/`RoomEvent`/`RoomTarget` API. The old `ReplicationGroup::new_from_entity()` inserts were removed because main no longer exposes that component.
- Added `StatesPlugin` to the headless server plugin set because Lightyear main initializes states that require Bevy's `StateTransition` schedule even when the app uses `MinimalPlugins`.
- Verification after the dependency switch: `cargo check --workspace -j 4`, `cargo test --workspace -j 4`, and `just trace-local 2 6 config/test.ron 5053` pass/complete. The trace emitted 14,946 `snake_head` rows, zero invariant violations, and no panic/error/replication-sender markers.
- Added data-driven `RespawnConfig` and server-side `RespawnReadyAt` gating. Human players and server-owned bots no longer respawn immediately after death; the default and test configs currently use a 1-second cooldown for both.
- Added dead-camera behavior for the local client. While alive the follow camera tracks the local predicted snake; after a collision death it follows the killer snake; after suicide or boundary death it stays static. `SpawnPlayer` is now bound to Enter and Space.
- Made respawning safer by scoring deterministic spawn candidates against room-local snake tails and arena bounds before falling back to the best available candidate.
- Made bots more conservative by increasing lookahead/danger thresholds and reducing voluntary turn frequency. In the latest 12-second `trace-local` run with four headless bot clients plus four server bots, server bots moved continuously through tick 539 and no collision/death churn appeared in the server log.
- Investigated the reported panic string `Received a message ack for a single message but message is a fragmented message`. It comes from Lightyear's `lightyear_transport::channel::senders::reliable::ReliableSender::receive_ack` when a reliable fragmented message receives an ack without a fragment id. This run did not reproduce it; keep it as an upstream transport bug candidate and preserve backtraces/logs if it appears again.
- Verification: `cargo check --workspace -j 4`, `cargo test --workspace -j 4`, `just --list`, and `just trace-local 4 12 config/test.ron 5051` pass/complete. The trace emitted 65,448 `snake_head` rows, zero invariant violations, and no panic/error/replication-sender markers.
- Improved shared bot steering so bots no longer rely on frequent random turns. `BotController` now prefers safe straight movement, avoids short self-trapping segments, turns away from imminent wall/self-tail collisions, and can score same-room snake tails as obstacles.
- Updated server-owned bots and headless bot clients to pass room-local tail snapshots into the shared obstacle-aware steering logic.
- Added bot unit tests for avoiding self-tail and other-tail lookahead collisions.
- Verification: `cargo test -p shared -j 4`, `cargo check --workspace -j 4`, and `cargo test --workspace -j 4` pass. A 60-second headless server smoke on `config/test.ron` with four server bots reported zero `Collision event` lines and five food pickups.
- Fixed blank non-headless clients after `just server` plus `just client`: the client now has `ReplicationReceiver::default()`, protocol registration happens after Lightyear client/server plugins, room-scoped entities use `NetworkTarget::All` plus room visibility, and food spawning waits out pending client handshakes without a `ReplicationSender`.
- Improved first-pass rendering visibility with an arena outline/axes, dark clear color, brighter food, thicker snake trails, and a one-time client log when gameplay entities reach the render world.
- Verification for the render/replication fix: connected WebTransport smoke with `config/test.ron` logs `Client render received gameplay entities snake_count=1 food_count=20` and no server `No ReplicationSender`/`ClientOf ... not found` errors. Remaining follow-up: client prediction can warn about rollback spans above the current 100-tick cap after joining a long-running server.
- Added runtime observability for autonomous agent checks: `RuntimeDebugPlugin` samples snake heads at fixed and frame schedules via `lightyear_debug::manual`, checks basic snake invariants in `FixedLast`, wires a Lightyear-compatible JSONL debug layer from config plus `LIGHTYEAR_DEBUG_FILE`, and adds DuckDB summary tooling through `just trace-local`, `just trace-summary`, and `tools/debug_trace_summary.sql`.
- The first `trace-local` smoke exposed a Lightyear pending-sender race when multiple clients connected in the same narrow window. Server client spawning now waits for ready `ReplicationSender` clients where possible, and the local bot/trace recipes stagger client starts to keep routine smoke traces clean while the deeper simultaneous-connect behavior remains worth tracking against Lightyear.
- Verification: a short `just trace-local 2 4 config/test.ron 5049` run produced 11,578 `snake_head` JSONL rows, zero `snake_invariant_violation` rows in DuckDB, and no `ERROR`/panic/replication-sender markers in the per-process logs.

### 2026-05-22

- Created this project thoughts document.
- Confirmed target intent: Powerline.io-like game in Rust with Bevy and Lightyear, starting with barebones graphics and prioritizing working online multiplayer.
- Inspected existing Lightrider workspace: client/server/shared crates, Bevy 0.13-era code, Lightyear git dependency, current snake tail interpolation/movement/collision/food prototype.
- Inspected original Powerline source tree for constants, assets, and behavior references.
- Noted initial target dependency direction: Bevy 0.18 and the then-current Lightyear release line. This was superseded on 2026-05-26 by the explicit Lightyear git-main dependency.
- Added implementation plan emphasizing upgrade, deterministic geometry, owner-only prediction, interpolation for remote snakes, rooms, bots, fake clients, Edgegap deployment, and later cosmetics.
- Added design decisions for RON-style data-driven config, default/test configs, multi-room single-server support, 50-player initial room target, WebTransport-only networking, server-authoritative deaths, headless clients, shared bot logic, and `lightyear_debug`/DuckDB-oriented diagnostics.
- Fixed `.cargo/config.toml` so local Linux cargo commands no longer try to build `aarch64-apple-darwin`.
- Updated the locked `time` crate to a current compatible patch release after the old version failed on the active Rust toolchain.
- Added typed `GameConfig` loading in `shared/src/config.rs` and RON configs in `config/default.ron` and `config/test.ron`.
- Wired map sizing through `GameConfig`.
- Removed direct `bevy_xpbd_2d` usage and replaced collision/proximity/food checks with explicit geometry.
- Added ray/segment geometry helpers and unit tests.
- Historical intermediate upgrade: moved to Bevy `0.18.1`, the then-current Lightyear package line, and matching Bevy ecosystem crate versions. Later input work removed Leafwing, and 2026-05-26 switched Lightyear to git main.
- Migrated Lightyear protocol setup away from removed macros (`protocolize!`, `component_protocol`, `message_protocol`) to explicit registration in `ProtocolPlugin`.
- Migrated client/server network startup to Lightyear 0.26 entity-based connection setup with `ClientPlugins`, `ServerPlugins`, `NetcodeClient`, `NetcodeServer`, and transport IO components.
- Converted Bevy local events to Bevy 0.18 messages where needed.
- Temporarily stubbed the old custom snake interpolation plugin; remote snake smoothing is now an explicit next task.
- Verified `cargo test --workspace -j 4` passes: client 0 tests, server 7 tests, shared 7 tests, doc tests 0.
- Verified `git diff --check` passes.
- Smoke-started the early headless server before the WebTransport-only decision; it ran until the timeout without immediate panic.
- Replaced Leafwing network inputs with Lightyear BEI inputs: `SnakeInput`/`MoveSnake` for movement and `PlayerInput`/`SpawnPlayer` for respawn.
- Added deterministic pre-spawned BEI action entities for client/server input mapping, following the Lightyear `examples/bevy_enhanced_inputs` pattern.
- Client input setup now uses Lightyear's replicated `Controlled` marker to identify locally controlled player/snake contexts.
- Removed the local `Owned` marker and switched local camera toggle to a BEI-only local input context.
- Added `--config <path>` support on client and server for loading RON game config files.
- Wired snake spawn length/speed, movement acceleration/speed clamps, boost distance, food target count/spawn interval/radius/growth, and server boundary checks through `GameConfig`.
- Added server-authoritative arena boundary death and tests for boundary collision, BEI movement direction selection, invalid reverse turns, and boost acceleration scaling.
- Initially changed food replication away from bare `Replicate::default()` after a headless server smoke exposed `No ReplicationSender` errors. Later validation showed Lightyear room visibility is only a filter, so room-scoped entities must still target connected clients through `NetworkTarget::All`.
- Verified `cargo tree -i leafwing-input-manager -p shared` reports nothing to print after the input migration.
- Verified BEI resolved as a single shared dependency after the input migration. The current BEI version is tracked in the 2026-05-26 dependency-switch entry.
- Verified `cargo check --workspace -j 4`, `cargo test --workspace -j 4`, and `git diff --check` pass after the input migration and Phase 1 simulation/config changes.
- Smoke-started the early headless server with `config/test.ron` before the WebTransport-only decision; it ran until the timeout without immediate panic.
- Finished Phase 1 fixed-tick cleanup: Lightyear client/server tick duration now comes from `GameConfig`, and proximity/collision/death/food overlap logic runs in `FixedUpdate` around movement.
- Added replicated `RoomId` and attached it to maps, players, snakes, and food so simulation queries can ignore entities from other rooms before the full room manager exists.
- Made proximity boost, server snake collision, and food overlap room-scoped, with focused tests for cross-room ignore behavior.
- Added client-side shared collision/proximity setup so local prediction computes boost state instead of leaving that entirely server-side.
- Verified Phase 1 close-out with `cargo check --workspace -j 4`, `cargo test --workspace -j 4` (server 10 tests, shared 12 tests), `git diff --check`, and a headless UDP server smoke using `config/test.ron`.
- Removed UDP/WebSocket support from active code paths and Lightyear features. Phase 2 networking is WebTransport-only, with self-signed and dangerous certificate features enabled for local development.
- Removed transport selection from CLI/config. Client now accepts `--certificate-digest` for WebTransport browser/deployment paths; server logs its generated self-signed digest on startup.
- Added replicated `PlayerScore` and `PlayerStatus`, updated food pickups to set score from tail length, and reset score/status on respawn.
- Split server-internal snake collisions from client death notifications: `SnakeCollision` now carries `DeathReason`, and clients receive `PlayerDeath` with mapped player/snake entities plus room id.
- Restored custom snake interpolation against Lightyear 0.26 using confirmed history for `TailPoints` and `TailLength`.
- Enabled frame interpolation for predicted snake `TailPoints` and intentionally did not enable VisualCorrection.
- Added tail interpolation tests and a food-pickup score test. Verification now passes with `cargo check --workspace -j 4`, `cargo test --workspace -j 4` (server 11 tests, shared 18 tests), `git diff --check`, no `lightyear_udp`/`lightyear_websocket` dependency path, and a headless WebTransport server smoke using `config/test.ron`.
- Implemented Phase 3 server rooms with `RoomDirectory`, Lightyear `Room` entities, auto/new/specific room join requests, room-scoped map spawning, room-scoped replication visibility, and room-preserving respawns.
- Implemented minimal per-room leaderboard state with replicated `PlayerRank`, recomputed from `PlayerScore`.
- Implemented Phase 4 bots: shared deterministic bot steering, server-owned bots, and headless fake clients using `--mode bot` with BEI `ActionMock`s.
- Added explicit binary names `lightrider-server` and `lightrider-client` to avoid Cargo output collisions between the server and client crates.
- Added a root `justfile` with `server`, `client`, `bot`, `bots`, and `local` recipes for local server/client/bot runs.
- Removed Bevy `dynamic_linking` from the workspace dependency because `cargo test --workspace -j 4` failed to link test binaries through `bevy_dylib`.
- Changed room-scoped replicated entities to use `Replicate::to_clients(NetworkTarget::All)` plus Lightyear room visibility, added `ReplicationReceiver` to the client connection entity, and moved shared protocol registration after the Lightyear client/server plugin groups. Food spawning now skips pending `ClientOf` entities until their `ReplicationSender` is present.
- Added a client-side render baseline: arena border/axes, non-black clear color, brighter food, thicker snake trails, and a one-time log when gameplay entities are available to render.
- Added stable connection logs for server startup, server room assignment, and client connection.
- Verified Phase 3/4 close-out with `cargo check --workspace -j 4`, `cargo test --workspace -j 4` (client 1 test, server 16 tests, shared 22 tests), `just --list`, a combined build of `lightrider-server` and `lightrider-client`, and a WebTransport smoke connecting one headless bot client to a headless server with no panic/protocol/error markers.
