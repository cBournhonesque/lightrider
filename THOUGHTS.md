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
- Food spawns in the arena. Eating food grows the snake and increases score.
- Score is derived from length for the prototype. Add richer score events later only if the original behavior requires them.
- The Powerline signature mechanic is proximity speed boost: a snake accelerates when moving close and parallel enough to another snake trail, then decelerates back toward base speed when not close.
- The prototype should support respawning after death.
- Bots should use the same movement, collision, food, and scoring rules as players.

### Prototype Rendering

- Start with Bevy primitives and gizmos: dark arena, visible boundaries, simple line tails, simple square/circle heads, simple food dots.
- Make gameplay state legible before matching cosmetics.
- Keep rendering replaceable so later asset imports from the original Powerline source do not disturb simulation or networking logic.
- Later cosmetic backlog: glow, sparks on proximity boost, death effects, food pickup animation, sound effects from `sounds/out.ogg`, sprites from `images/sheet.png`, logo/menu assets, minimap, leaderboard styling.

### Arena And Rules

- Original Powerline client constants of interest:
  - `GAME_SCALE = 10.0`
  - `UPDATE_EVERY_N_TICKS = 3`
  - `INTERP_TIME = (1000 / 30) * UPDATE_EVERY_N_TICKS`
  - Default client-side arena values include `arenaWidth = 5000.0`, `arenaHeight = 1600.0`, centered at `(0.0, 0.0)`.
  - Kill reasons include left screen, killed, boundary, and suicide.
- Current Lightrider code uses a square map size of `2000.0`. Treat this as temporary prototype tuning, not a final spec.
- Initial server prototype can use one arena per room. Room size, max players, bot count, and food count should come from data-driven config.
- Keep one default config that is as close as practical to original Powerline behavior. Most values should feel similar to the original, but exact replication is not required when the original uses esoteric or ad-hoc logic.
- Add smaller test configs, for example a much smaller arena for local collision, room, bot, and fake-client testing.

### Configuration

- Use data-driven config for gameplay and server settings. RON is a good initial format unless implementation shows a better fit.
- Maintain a `default` config tuned toward Powerline-like behavior.
- Maintain at least one `test` config with a small arena and low limits for quick local iteration.
- Config should cover arena dimensions, tick rate, spawn rules, starting length, speed/boost tuning, food count, room capacity, bot count, fake-client defaults, and network/debug knobs.
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

- Use the latest compatible Bevy and Lightyear release line. As of 2026-05-22, Bevy is on the 0.18 release line and docs.rs lists Lightyear 0.26.4. Version references: [Bevy 0.18 release](https://bevy.org/news/bevy-0-18/), [Lightyear latest docs.rs crate](https://docs.rs/crate/lightyear/latest).
- Use WebTransport as the only supported transport. UDP/WebSocket code paths were removed during Phase 2 to keep deployment and debugging focused.
- Client prediction is enabled only for the local player's snake.
- Other player snakes are interpolated, not predicted.
- The server remains authoritative and reconciles predicted local state.
- Movement is predicted, but deaths are server-authoritative. Clients may show provisional feedback later, but the server decides kill validity, kill reason, respawn state, and score effects.
- Inputs should be compact directional actions. Server-side simulation should consume the same input semantics as client prediction.
- Use Lightyear replication groups for snake state, especially tail interpolation. The Lightyear `replication_groups` example is the reference for snake interpolation logic.
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
- Keep protocol and simulation docs close to code and link them from this file.

## Current Repo Snapshot

- Workspace members today: `client`, `server`, `shared`. Target organization still adds a separate `render` folder/crate so rendering can be excluded from headless client modes.
- Dependency state after Phase 4: Bevy `0.18.1`, Lightyear `0.26.4`, `bevy_enhanced_input = "0.22"` through Lightyear's `input_bei` integration, `bevy_turborand = "0.13"`, `bevy-inspector-egui = "0.36"`, and no direct physics-engine dependency. Bevy `dynamic_linking` is disabled because it broke test-binary links through `bevy_dylib` on this machine.
- Lightyear transport features are WebTransport-only: `webtransport`, `webtransport_self_signed`, and `webtransport_dangerous_configuration`. `udp` and `websocket` features are intentionally not enabled.
- Leafwing input usage has been removed from the active dependency tree. Note: Cargo reports `bevy_enhanced_input` latest stable as `0.25.0` and latest pre-release as `0.26.0-rc.1`, but Lightyear `0.26.4`'s `input_bei` integration depends on the `0.22` action trait graph. Use the Lightyear-compatible BEI version until Lightyear updates its integration, otherwise the project gets two incompatible BEI action APIs.
- `.cargo/config.toml` no longer forces an Apple target; Linux workspace checks/tests now run on the host target.
- Data-driven config exists in `shared/src/config.rs` with RON files at `config/default.ron` and `config/test.ron`. `MovementConfig::tick_duration()` is the single source for the Lightyear fixed tick duration.
- Shared logic has a polyline snake model in `shared/src/network/protocol/components/snake.rs`.
- Movement logic lives in `shared/src/movement/mod.rs` and includes BEI-driven turn handling, config-driven acceleration/speed integration, and tail shortening.
- Collision/proximity now uses explicit geometry in `shared/src/utils/geometry.rs`, `shared/src/collision/collider.rs`, and `server/src/collision/collider.rs`. Simulation-critical proximity, server collision/death, and food overlap/growth systems run in `FixedUpdate` around `SimulationSet::Movement`.
- Shared `RoomId` is registered in the protocol and attached to maps, players, snakes, and food. Proximity boost, snake-vs-snake collision, food overlap, respawns, rank computation, and Lightyear room visibility are room-scoped.
- Server boundary death now uses explicit arena geometry from `GameConfig`.
- Server food spawning, overlap, and growth use `GameConfig` for target count, spawn interval, radius, and tail growth. Food is replicated through room visibility.
- `PlayerScore`, `PlayerRank`, and `PlayerStatus` replicate with the player. Food pickups update score from tail length on the server, per-room ranks are recomputed on the server, and respawns reset score/status from config.
- Death flow now uses server-internal `SnakeCollision` with `DeathReason` and sends a server-to-client `PlayerDeath` message containing mapped player/snake entities, room id, and reason. The server remains authoritative for despawning snakes, setting player status, and clearing `Player.snake`.
- Client prediction now includes the shared collision/proximity plugin, so local predicted snakes can compute boost from nearby room-local trails before reconciliation.
- Lightyear protocol registration now uses the 0.26 explicit plugin style in `shared/src/network/protocol/mod.rs`.
- Networked inputs use Lightyear's BEI input plugin with replicated action entities. Client setup keys off Lightyear's `Controlled` marker and no longer uses a local `Owned` marker.
- Client/server connection setup now uses Lightyear 0.26 entity components (`ClientPlugins`, `ServerPlugins`, `NetcodeClient`, `NetcodeServer`, WebTransport IO components) rather than old `ClientPlugin<GameProtocol>` / `ServerPlugin<GameProtocol>` config objects. Both client and server derive Lightyear's tick duration from `GameConfig`.
- Client CLI accepts `--certificate-digest <hex>` for WebTransport. Native dev builds can leave it empty because the local dev feature set enables dangerous certificate handling; browser/deployment paths should pass the digest printed by the server.
- Client and server CLIs accept `--config <path>` to load a RON `GameConfig`; without it they use `GameConfig::default()`.
- Existing client rendering is very barebones and uses gizmos in `client/src/render/snake.rs`.
- Custom snake interpolation has been restored against Lightyear 0.26 using `ConfirmedHistory<TailPoints>`, `ConfirmedHistory<TailLength>`, and `InterpolationSystems::Interpolate`. The helper is adapted from the old Lightrider prototype and the Lightyear `replication_groups` example.
- Frame interpolation is enabled for predicted snake `TailPoints` via `FrameInterpolationPlugin<TailPoints>` and `FrameInterpolate<TailPoints>`. Visual correction is intentionally not configured.
- Lightyear room visibility is the interest-management filter for game entities, but it is not a replacement for normal replication targets. Room-scoped entities use `Replicate::to_clients(NetworkTarget::All)` plus `RoomEvent::AddEntity/AddSender`, and food spawning pauses while any `ClientOf` lacks a `ReplicationSender` to avoid pending-handshake sender errors.
- Non-headless clients must spawn `ReplicationReceiver::default()` on the client entity; otherwise WebTransport connects but replicated game entities never arrive. Add Lightyear `ClientPlugins`/`ServerPlugins` before the shared `ProtocolPlugin`, matching the upstream examples.
- The client render baseline now draws a dark clear color, arena border/axes, larger food circles, and brighter/thicker snake gizmos so a connected or not-yet-connected window is visibly alive.
- Server-owned bots live in `server/src/bots.rs` and reuse shared bot steering from `shared/src/bot.rs`. Bot clients are `client --headless --mode bot` and drive BEI `ActionMock`s through the same input path as real clients. Bot steering now scores arena boundaries, its own tail, and same-room snake tails as short lookahead obstacles; random voluntary turns are throttled so bots do not immediately draw tiny self-trapping boxes.
- Runnable binaries are explicitly named `lightrider-server` and `lightrider-client`; do not rely on both crates exposing a bin named `main`.
- A root `justfile` exists with `server`, `client`, `bot`, `bots`, and `local` recipes for starting a local headless server, headless bot clients, and a normal client.
- Phase 4 verification: `cargo check --workspace -j 4` passes, `cargo test --workspace -j 4` passes with client 1 test, server 16 tests, shared 22 tests, `just --list` passes, the named server/client binaries build together, and a WebTransport smoke connects a headless bot client to a headless server with no panic/protocol/error markers.

## Implementation Plan

### Phase 0: Baseline And Dependency Upgrade

Status: complete enough to move to Phase 1.

1. Recorded the current behavior and test baseline before major edits.
2. Upgraded the workspace to Bevy `0.18.1` and Lightyear `0.26.4`, the latest compatible line found for this repo on 2026-05-22.
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

1. Server-owned bots choose legal turns with shared deterministic steering and use the same movement, collision, food, score, death, and respawn systems as players. They avoid walls, their own tail, and same-room snake trails using a short raycast lookahead.
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

- Improved shared bot steering so bots no longer rely on frequent random turns. `BotController` now prefers safe straight movement, avoids short self-trapping segments, turns away from imminent wall/self-tail collisions, and can score same-room snake tails as obstacles.
- Updated server-owned bots and headless bot clients to pass room-local tail snapshots into the shared obstacle-aware steering logic.
- Added bot unit tests for avoiding self-tail and other-tail lookahead collisions.
- Verification: `cargo test -p shared -j 4`, `cargo check --workspace -j 4`, and `cargo test --workspace -j 4` pass. A 60-second headless server smoke on `config/test.ron` with four server bots reported zero `Collision event` lines and five food pickups.
- Fixed blank non-headless clients after `just server` plus `just client`: the client now has `ReplicationReceiver::default()`, protocol registration happens after Lightyear client/server plugins, room-scoped entities use `NetworkTarget::All` plus room visibility, and food spawning waits out pending client handshakes without a `ReplicationSender`.
- Improved first-pass rendering visibility with an arena outline/axes, dark clear color, brighter food, thicker snake trails, and a one-time client log when gameplay entities reach the render world.
- Verification for the render/replication fix: connected WebTransport smoke with `config/test.ron` logs `Client render received gameplay entities snake_count=1 food_count=20` and no server `No ReplicationSender`/`ClientOf ... not found` errors. Remaining follow-up: client prediction can warn about rollback spans above the current 100-tick cap after joining a long-running server.

### 2026-05-22

- Created this project thoughts document.
- Confirmed target intent: Powerline.io-like game in Rust with Bevy and Lightyear, starting with barebones graphics and prioritizing working online multiplayer.
- Inspected existing Lightrider workspace: client/server/shared crates, Bevy 0.13-era code, Lightyear git dependency, current snake tail interpolation/movement/collision/food prototype.
- Inspected original Powerline source tree for constants, assets, and behavior references.
- Noted target dependency direction: current upstream release line is Bevy 0.18 and Lightyear 0.26.4 as of this date.
- Added implementation plan emphasizing upgrade, deterministic geometry, owner-only prediction, interpolation for remote snakes, rooms, bots, fake clients, Edgegap deployment, and later cosmetics.
- Added design decisions for RON-style data-driven config, default/test configs, multi-room single-server support, 50-player initial room target, WebTransport-only networking, server-authoritative deaths, headless clients, shared bot logic, and `lightyear_debug`/DuckDB-oriented diagnostics.
- Fixed `.cargo/config.toml` so local Linux cargo commands no longer try to build `aarch64-apple-darwin`.
- Updated the locked `time` crate to a current compatible patch release after the old version failed on the active Rust toolchain.
- Added typed `GameConfig` loading in `shared/src/config.rs` and RON configs in `config/default.ron` and `config/test.ron`.
- Wired map sizing through `GameConfig`.
- Removed direct `bevy_xpbd_2d` usage and replaced collision/proximity/food checks with explicit geometry.
- Added ray/segment geometry helpers and unit tests.
- Upgraded to Bevy `0.18.1`, Lightyear `0.26.4`, `leafwing-input-manager = "0.20"`, and matching Bevy ecosystem crate versions.
- Migrated Lightyear protocol setup away from removed macros (`protocolize!`, `component_protocol`, `message_protocol`) to explicit registration in `ProtocolPlugin`.
- Migrated client/server network startup to Lightyear 0.26 entity-based connection setup with `ClientPlugins`, `ServerPlugins`, `NetcodeClient`, `NetcodeServer`, and transport IO components.
- Converted Bevy local events to Bevy 0.18 messages where needed.
- Temporarily stubbed the old custom snake interpolation plugin; remote snake smoothing is now an explicit next task.
- Verified `cargo test --workspace -j 4` passes: client 0 tests, server 7 tests, shared 7 tests, doc tests 0.
- Verified `git diff --check` passes.
- Smoke-started the headless server with `cargo run -j 4 -p server --bin main -- --headless --transport udp --port 0`; it ran until the timeout without immediate panic.
- Replaced Leafwing network inputs with Lightyear BEI inputs: `SnakeInput`/`MoveSnake` for movement and `PlayerInput`/`SpawnPlayer` for respawn.
- Added deterministic pre-spawned BEI action entities for client/server input mapping, following the Lightyear `examples/bevy_enhanced_inputs` pattern.
- Client input setup now uses Lightyear's replicated `Controlled` marker to identify locally controlled player/snake contexts.
- Removed the local `Owned` marker and switched local camera toggle to a BEI-only local input context.
- Added `--config <path>` support on client and server for loading RON game config files.
- Wired snake spawn length/speed, movement acceleration/speed clamps, boost distance, food target count/spawn interval/radius/growth, and server boundary checks through `GameConfig`.
- Added server-authoritative arena boundary death and tests for boundary collision, BEI movement direction selection, invalid reverse turns, and boost acceleration scaling.
- Initially changed food replication away from bare `Replicate::default()` after a headless server smoke exposed `No ReplicationSender` errors. Later validation showed Lightyear room visibility is only a filter, so room-scoped entities must still target connected clients through `NetworkTarget::All`.
- Verified `cargo tree -i leafwing-input-manager -p shared` reports nothing to print.
- Verified `cargo tree -i bevy_enhanced_input -p shared` resolves a single BEI version, `0.22.2`, shared by Lightyear's BEI integration and the `shared` crate.
- Verified `cargo check --workspace -j 4`, `cargo test --workspace -j 4`, and `git diff --check` pass after the input migration and Phase 1 simulation/config changes.
- Smoke-started the headless server with `cargo run -j 4 -p server --bin main -- --headless --transport udp --port 0 --config config/test.ron`; it ran until the timeout without immediate panic.
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
