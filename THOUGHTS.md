---
date: 2026-05-22
topic: "Lightrider project design, implementation plan, and agent notes"
tags: [lightrider, bevy, lightyear, game-design, multiplayer]
status: active
---

# Lightrider Thoughts

This is the durable project document for Lightrider. Keep it up to date whenever code, architecture, tooling, gameplay rules, deployment assumptions, debugging knowledge, or operational findings change.

## Game Design Document

### Product Goal

Lightrider is a Rust, Bevy, and Lightyear clone of Powerline.io. The first milestone is a fully working online multiplayer prototype with intentionally barebones graphics. Cosmetics such as animation polish, sound, music, textures, fonts, richer colors, and original asset extraction come after gameplay and networking hold up online.

Use `/spare/ssd/cbournhonesque/src/other/snakegame/original/powerline.io` as inspiration for behavior, tuning, UI ideas, and later assets. The prototype should prioritize readable primitives and correctness over visual fidelity.

### Core Gameplay

- Players control a continuously moving light snake on an orthogonal grid-like plane.
- A snake is a head plus a tail polyline. Direction changes add turn points; the tail follows the head path and is shortened from the back to maintain length.
- Players steer up, down, left, and right. A 180-degree reversal is invalid.
- Hitting another snake, hitting yourself, or hitting the arena boundary kills the snake.
- Food spawns as very small dots. Nearby food is magnetically attracted toward close snake heads and is absorbed at the head.
- Eating food grows the snake, increases score, and gives a short smooth acceleration burst.
- Score starts at 0 and counts growth beyond the initial snake length. Starting tail length is not part of score.
- When a snake dies, the server drops food samples along the killed snake trajectory.
- The Powerline signature mechanic is proximity speed boost: a snake accelerates when moving close and parallel enough to another snake trail, then decelerates back toward base speed when not close.
- The server remains authoritative for deaths, kill reasons, food pickups, score, room assignment, and respawn cooldowns.
- Bots should use the same movement, collision, food, scoring, death, and respawn rules as players.

### Prototype Rendering

- Start with Bevy primitives and gizmos: dark arena, visible outline, simple line tails, simple heads, food dots, minimap, leaderboard, and death/stat overlays.
- Keep rendering replaceable so later asset imports from the original Powerline source do not disturb simulation or networking logic.
- The normal camera should be close to the local head and grow outward with snake size. Debug mode can toggle to a very zoomed-out camera and show shortcuts with `?`.
- Later cosmetic backlog: glow, sparks on proximity boost, richer death effects, food pickup animation, sound effects from `sounds/out.ogg`, sprites from `images/sheet.png`, logo/menu assets, minimap/leaderboard styling.

### Arena And Rules

- Original Powerline client constants of interest:
  - `GAME_SCALE = 10.0`
  - `UPDATE_EVERY_N_TICKS = 3`
  - `INTERP_TIME = (1000 / 30) * UPDATE_EVERY_N_TICKS`
  - Default client-side arena values include `arenaWidth = 5000.0`, `arenaHeight = 1600.0`, centered at `(0.0, 0.0)`.
  - Kill reasons include left screen, killed, boundary, and suicide.
- Current default config uses a Powerline-like `5000.0 x 1600.0` arena. `config/test.ron` uses an `800.0 x 600.0` arena for quick local testing.
- Initial target room limit is 50 players per room, matching the intended scale of the original Powerline.io.
- Exact original behavior is not required when the original uses ad-hoc logic, but defaults should produce a similar feel.

### Configuration

- Use typed, data-driven config loaded from RON.
- Keep one `config/default.ron` tuned toward Powerline-like behavior.
- Keep smaller test configs such as `config/test.ron` for local collision, room, bot, and fake-client testing.
- Config should cover arena dimensions, tick rate, spawn rules, respawn cooldowns, starting length, speed/boost tuning, food count, room capacity, bot count, fake-client defaults, render/debug camera knobs, tail width, map outline width, network input delay, and debug tracing.
- Game logic should consume typed config resources rather than scattering constants through systems.

### Multiplayer And Rooms

- The game will be served online through Edgegap.
- Users can join a random room or create a new room.
- Browser users can also enter a four-letter private room code. The code maps to a stable private `RoomId`; joining the same code should route clients to the same private room when capacity permits.
- A single server process should support multiple game rooms through the room system.
- The authoritative server owns room simulation, player spawning, bot spawning, food spawning, collision, scoring, death, and respawn.
- Clients render the world and send input. They do not decide kills, food pickups, or score.

### Networking Model

- Use Bevy `0.18` and Lightyear from the `main` branch of `https://github.com/cBournhonesque/lightyear.git`. Cargo may still report Lightyear's crate version as `0.26.4`; the source of truth is the git dependency and commit in `Cargo.lock`.
- The deployment plan has pivoted back to WebTransport so browser WASM clients can connect directly to Edgegap-hosted game servers.
- Bevygap is the planned matchmaking/control path: clients talk to `bevygap_matchmaker_httpd` over WebSocket, receive a Lightyear `ConnectToken` plus WebTransport certificate digest, and then connect to the game server over WebTransport.
- The direct Lightrider client/server path uses WebTransport and remains the default local/dev mode.
- The first Bevygap integration slice is runtime-tested behind optional Cargo features. A `client --features bevygap -- --matchmaker-url ...` run spawns an unconnected Lightyear client entity and lets `bevygap_client_plugin` request a token and attach token auth, peer address, and WebTransport IO. A `server --features bevygap -- --bevygap` run adds the server-side Bevygap/NATS plugin; direct mode does not read Edgegap or NATS env vars.
- Bevygap local mock validation works through `just bevygap-local-smoke`: local NATS + in-process `BEVYGAP_CONTEXT_MODE=local` server context + Lightrider WebTransport server + mock `bevygap_matchmaker` + `bevygap_matchmaker_httpd` + headless Lightrider bot client. The client receives a redacted `Session Ready` response, decodes the Lightyear `ConnectToken`, connects over WebTransport, and the server and matchmaker both observe the active-connection path.
- Production netcode identity is environment-driven. `LIGHTRIDER_PROTOCOL_ID` and `LIGHTRIDER_PRIVATE_KEY` must match between game server and matchmaker; `LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1` makes zero/dev defaults a startup error. `LIGHTRIDER_PRIVATE_KEY` accepts either 64 hex characters or the Bevygap-compatible comma-separated 32-byte form.
- Edgegap should be treated as coarse deployment/session capacity. Lightrider rooms are game-specific state and should be routed by Bevygap, not by Edgegap directly. A warm Edgegap deployment can host multiple public/private rooms until game-server capacity is reached.
- Client prediction is enabled only for the local player's snake.
- Other player snakes are interpolated, not predicted.
- Movement is predicted, but deaths are server-authoritative.
- Visual correction is intentionally not configured right now; frame interpolation is used.
- Bots controlled by the server should reuse shared steering/simulation where possible.
- Fake clients are the normal client binary with `--headless --mode bot`; they connect through the real protocol and send real movement/respawn actions.

## General Agent Notes

### Architecture Goals

- Keep a Cargo workspace.
- Target top-level organization is four main areas:
  - `client`: client-specific app logic, input sources, connection flow, local prediction setup, client modes, and headless/fake-client behavior.
  - `server`: authoritative simulation orchestration, room lifecycle, bot orchestration, Edgegap-facing runtime config, and production logging.
  - `render`: rendering-specific logic, camera, UI, debug views, visual interpolation display, and cosmetic systems. This is still folded into the client crate today.
  - `shared`: network protocol, deterministic movement, collision geometry, room-safe components, config types, simulation rules, and reusable bot decision helpers.
- Do not use Avian or another physics engine for core gameplay. Collision, proximity, friction/boost, food overlap, and boundaries should remain explicit geometry.
- Rust modules with nesting should use `a.rs` and `a/b.rs`, not `mod.rs`, when adding or reorganizing modules.
- Add focused unit tests for complicated logic: tail shortening, tail interpolation, turn validation, collision cases, proximity boost, food spawn constraints, room assignment, and bot/fake-client input generation.

### Current Repo Snapshot

- Workspace members: `client`, `server`, `shared`, and `web_client`.
- Dependency state: Bevy `0.18.1`, Lightyear git `main` at the commit locked in `Cargo.lock`, `bevy_enhanced_input = "0.24.4"`, `bevy_turborand = "0.13"`, `bevy-inspector-egui = "0.36"`, and no direct physics-engine dependency.
- Lightrider transport features currently enable Lightyear WebTransport, self-signed WebTransport certificates, and the dangerous native no-certificate-validation test path.
- Data-driven config lives in `shared/src/config.rs` with RON files at `config/default.ron` and `config/test.ron`.
- `MovementConfig::tick_duration()` is the source for the Lightyear fixed tick duration.
- Shared movement lives in `shared/src/movement/mod.rs` and includes BEI-driven turn handling, config-driven acceleration/speed integration, food-pickup acceleration boost decay, and tail shortening.
- Shared explicit geometry lives in `shared/src/utils/geometry.rs`, `shared/src/collision/collider.rs`, and `server/src/collision/collider.rs`.
- Simulation-critical movement, proximity, server collision/death, and food overlap/growth run in `FixedUpdate` around `SimulationSet::Movement`.
- `RoomId` is registered in the protocol and attached to maps, players, snakes, and food. Proximity boost, collision, food overlap, respawn, rank computation, and Lightyear visibility are room-scoped.
- Food spawning, magnetic attraction, overlap, growth, and death drops use `GameConfig`.
- Food uses Lightyear `Position` interpolation. Eaten food should be despawned in a way that lets Lightyear record delayed interpolated despawns.
- `PlayerScore`, `PlayerRank`, and `PlayerStatus` replicate with the player.
- Death flow uses server-internal `SnakeCollision` with `DeathReason` and sends a server-to-client `PlayerDeath` message.
- Client death view follows the killer after a collision death if possible, otherwise stays static. Respawn is requested with Enter or Space after cooldown.
- Networked inputs use Lightyear's BEI input plugin with replicated action entities. Client setup keys off Lightyear's `Controlled` marker.
- Client/server connection setup uses Lightyear 0.26 entity components (`ClientPlugins`, `ServerPlugins`, `NetcodeClient`, `NetcodeServer`, `WebTransportClientIo`, `WebTransportServerIo`).
- Direct native clients can pass `--cert-digest` or `--certificate-digest`; copied colon-separated server log digests are normalized before connection. Leaving it empty uses the native dangerous no-validation test path. Browser clients must receive the digest from matchmaking/bootstrap.
- Optional Cargo feature `bevygap` gates Lightrider's path dependencies on `bevygap_client_plugin` and `bevygap_server_plugin`. The default build must not compile or require Bevygap, NATS, Edgegap credentials, or matchmaker services.
- Optional client feature `bevygap-matchmaker-tls` forwards to `bevygap_client_plugin/matchmaker-tls` for TLS matchmaker WebSocket support.
- Client networking now has two startup modes: direct WebTransport/manual auth, or Bevygap token mode. In token mode the client entity starts with Lightyear base components and waits for the Bevygap plugin to connect it.
- Browser builds have a dedicated `lightrider-web` WASM entrypoint. It avoids Clap and defaults the matchmaker WebSocket to same-origin `/matchmaker/ws`, with `?matchmaker_url=` as an override.
- Browser UI now lives in the `web_client` workspace crate. It is a Leptos CSR shell using `leptos-bevy-canvas` to mount the existing Bevy client in the background canvas while web UI owns the Powerline-style join modal, player-name input, private room code input, and external links.
- Browser query parameters currently drive initial Bevy startup: `name`, `room`, `matchmaker_url`, `matchmaker_game`, and `matchmaker_version`. `room` accepts `auto`, `new`, a four-letter private room code, or a numeric room id. The modal writes settings back to the URL; replacing this URL reload with a Leptos-to-Bevy message bridge is the next polish step.
- Bevygap room-aware matchmaking is implemented in the local Bevygap repo. Clients include `RoomSelection` in `RequestSession`; HTTPD forwards the full request; game servers publish deployment room metrics to NATS KV; the matchmaker reuses an existing Edgegap deployment by setting `SessionModel.deployment_request_id` when capacity policy allows.
- Matchmaker capacity policy is CLI/env driven: `--max-players-per-deployment`, `--max-rooms-per-deployment`, and `--max-cpu-percent-per-deployment` are wired through local just recipes and the matchmaker/control container as `BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT`, `BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT`, and `BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT`.
- Client input systems tolerate the token-mode startup window before `LocalId` is available.
- Non-headless clients must spawn `ReplicationReceiver::default()` on the client entity, or the network connection can succeed without replicated game entities arriving.
- Custom snake interpolation uses `ConfirmedHistory<TailPoints>`, `ConfirmedHistory<TailLength>`, and `InterpolationSystems::Interpolate`, adapted from the old prototype and the Lightyear `replication_groups` example.
- Frame interpolation is enabled for predicted snake `TailPoints` through `FrameInterpolationPlugin<TailPoints>` and `FrameInterpolate<TailPoints>`.
- Server-owned bots live in `server/src/bots.rs` and reuse shared bot steering from `shared/src/bot.rs`.
- Runnable binaries are named `lightrider-server` and `lightrider-client`.
- The root `justfile` has `server`, `client`, `client-debug`, `bot`, `bots`, `local`, `trace-local`, `trace-summary`, and Edgegap image recipes.
- Bevygap local-smoke recipes now include `bevygap-help`, `bevygap-nats-pull`, `bevygap-nats`, `bevygap-nats-health`, `bevygap-fake-context`, `bevygap-server-local`, `bevygap-matchmaker-local`, `bevygap-matchmaker-mock-local`, `bevygap-matchmaker-httpd-local`, `bevygap-matchmaker-mock-stack-local`, `bevygap-client-bot`, and `bevygap-local-smoke`. Use `bevygap-help` for the required order.
- `bevygap-local-smoke` writes service logs under `logs/bevygap/<timestamp>/` and updates `logs/bevygap/latest`. It scans for cert digest publication, mock session creation, redacted session readiness, client connection attempt, server-side Lightyear connect event, server active-connection KV write, and matchmaker active-connection watcher observation.
- Web-server/control-host setup is scripted by `tools/setup_web_server_host.sh` and wrapped by `just deploy-web-server`, `just web-server-env-template`, `just web-server-install`, and `just web-server-health`. The installer runs the bundled matchmaker/control image as a systemd-managed Podman container with web on `80/tcp`, NATS on `4222/tcp`, and NATS monitoring bound to local `8222/tcp`.
- `Dockerfile.matchmaker` defaults to a balanced 32G+ RAM release build for the control image: two Cargo jobs, thin LTO, multiple codegen units, and `lld` for the native Bevygap binaries. Smaller hosts can override to `MATCHMAKER_CARGO_JOBS=1 MATCHMAKER_RELEASE_LTO=false MATCHMAKER_RELEASE_CODEGEN_UNITS=16 WEB_CARGO_JOBS=1`. The Dockerfile also installs `wasm32-unknown-unknown` after copying the repo so Rustup applies it to the active `rust-toolchain.toml` toolchain.
- Browser UI local testing is available through `just web-build` and `just web-serve`. The local browser path expects the manual Bevygap mock stack on NATS + `bevygap-server-local` + `bevygap-matchmaker-mock-stack-local`, then opens `http://localhost:8000/?matchmaker_url=ws://127.0.0.1:3000/matchmaker/ws&matchmaker_game=lightrider&matchmaker_version=dev`. Through SSH port forwarding, forward both web and matchmaker TCP ports; the full remote browser game still needs a separately reachable WebTransport/UDP game port.
- Hybrid Edgegap testing is possible with the game server on Edgegap and the matchmaker/web server local, but only if the Edgegap container and the local matchmaker share a NATS endpoint that Edgegap can reach. A plain local `127.0.0.1:4222` NATS cannot complete this flow.
- Production netcode currently uses `LIGHTRIDER_PROTOCOL_ID=1`; the private key is stored locally in ignored `secrets/prod-netcode.env` and must be copied into both the Edgegap game-server app version and the matchmaker/control host env.
- Matchmaker/control image builds can be done locally and pushed to the Edgegap registry; the VPS can then use `SKIP_IMAGE_BUILD=1 just deploy-web-server ...` to pull/run only. `matchmaker-build` and `deploy-web-server` accept Podman build resource kwargs such as `memory=24g`, `cpus=8`, `cpu_quota=...`, and `cpuset_cpus=...`.
- Edgegap app-version automation defaults `EDGEGAP_FORCE_CACHE=false` because the current Edgegap organization rejects enabled image cache with quota `0`. The Edgegap API accepts `req_cpu`/`req_memory` on app-version create but rejects them on update, so sync uses a PATCH-safe update payload and then verifies the refetched deploy-critical state. The verifier normalizes Edgegap-managed/default fields and checks deploy-critical state instead of byte-for-byte API echo.

### Observability And Debugging

- Runtime debug tracing lives in `shared/src/debug.rs`.
- Resource policy for this repo: use Cargo with at most two jobs (`-j 2` or `CARGO_BUILD_JOBS=2`). Avoid starting overlapping Cargo builds unless explicitly needed. Dev builds keep incremental compilation enabled for iteration speed, but reduce disk pressure by keeping only line-table debug info for local crates and no debug info for dependencies/build scripts.
- When `GameConfig.debug.lightyear_debug` is true and `LIGHTYEAR_DEBUG_FILE` is set, client/server `LogPlugin`s install a Lightyear-compatible JSONL layer for `lightyear_debug::*` tracing targets and enable `lightyear_debug=trace`.
- `json_snapshots` emits `snake_head` rows and `snake_invariant_violation` rows for DuckDB analysis.
- `just trace-local` builds binaries once, runs a headless WebTransport server plus multiple staggered headless bot clients, writes `.ndjson` and `.log` files under `logs/debug/<timestamp>/`, and runs `tools/debug_trace_summary.sql`.
- `just trace-summary dir=...` reruns the DuckDB summary on an existing trace directory.
- `just bevygap-local-smoke` is the native token-path smoke. It uses local NATS and the server plugin's local context mode, then verifies the matchmaker WebSocket -> NATS -> mock Edgegap session -> Lightyear `ConnectToken` -> WebTransport connect -> active-session KV flow. The smoke intentionally checks redacted log text and should not emit raw connect tokens.
- AI agents should be able to autonomously run a headless server plus multiple headless bot clients, capture structured logs, load them into DuckDB, and identify simulation/replication bugs without manual visual inspection.
- When debugging deployed WebTransport, compare client timeout logs with server logs. The server should log `Starting WebTransport server`, the certificate digest, and Lightyear WebTransport startup before clients attempt to connect.

## Implementation Plan

### Phase 0: Baseline And Dependency Upgrade

Status: complete.

1. Upgraded the workspace to Bevy `0.18.1` and Lightyear main.
2. Introduced typed data-driven RON config loading with default and test configs.
3. Removed direct physics-engine usage and replaced gameplay checks with explicit geometry.
4. Restored workspace checks/tests.

### Phase 1: Deterministic Shared Simulation

Status: complete enough to build on.

1. Movement, speed, boost, spawn length, arena, food, room, and Lightyear fixed tick settings are config-driven.
2. Boundary collision, room-scoped snake collision, proximity boost, and food overlap are explicit geometry.
3. Simulation-critical systems run on fixed tick schedules.
4. Focused tests exist for boundary, boost helpers, room-scoped proximity, room-scoped snake collision, and room-scoped food overlap.

### Phase 2: Lightyear Protocol And Prediction

Status: complete enough for prototype validation.

1. Protocol registration is rebuilt against the upgraded Lightyear API.
2. Runtime replication covers player identity, snake state, tail points, food, room id, score, status, rank, and death messages.
3. WebTransport is now the active direct client/server transport; the online/browser path will add Bevygap token matchmaking on top.
4. Only the local player's snake is predicted. Remote snakes are interpolated.
5. Deaths, kill reasons, food pickups, scoring, and respawn state stay server-authoritative.
6. Remote snake interpolation and predicted-tail frame interpolation are in place.

### Phase 3: Authoritative Server Rooms

Status: complete enough for deployable prototype work.

1. Room entities/resources exist in `server/src/rooms.rs`.
2. Clients can auto-join a public room, create a new public room, request a numeric room id, or request a four-letter private room code through `RoomJoinRequest`.
3. Room config covers max rooms, max players per room, arena size, food target, and bot target counts.
4. Replication, collision, food, proximity, respawn, and ranking are room-scoped.
5. Public auto-join selection ignores private-code rooms so random players do not backfill private games.
6. Minimal leaderboard/rank state exists as replicated `PlayerRank`.

### Phase 4: Bots And Fake Clients

Status: complete enough for local load/latency smoke work.

1. Server-owned bots choose legal turns with shared deterministic steering and use normal simulation systems.
2. Fake clients run as `lightrider-client --headless --mode bot` and send real network inputs.
3. Local test modes are available through `just bot`, `just bots`, and `just local`.
4. Initial operational signals are connection logs, room assignment logs, replicated score/rank/status, and smoke-test log scanning.

### Phase 5: Deployable Prototype

Status: in progress.

1. The game-server image is `Dockerfile.server`. It builds `lightrider-server --features bevygap`, defaults to `--bevygap`, and exposes `7777/udp` for WebTransport/QUIC.
2. The matchmaker/control image is `Dockerfile.matchmaker`. It bundles NATS, `bevygap_matchmaker`, `bevygap_matchmaker_httpd`, nginx, and the compiled `lightrider-web` WASM files.
3. Edgegap registry credentials and API settings are stored locally in ignored `secrets/edgegap.env`.
4. The server image tag shape is `registry.edgegap.com/lightyear-6qgcf4w4mrq7/lightrider-server:<tag>`.
5. The matchmaker image tag shape is `registry.edgegap.com/lightyear-6qgcf4w4mrq7/lightrider-matchmaker:<tag>`.
6. Current deployment direction: run the matchmaker/control image on a VPS or equivalent public host, and run the game-server image on Edgegap.

### Edgegap And Web Client Hosting Plan

Goal: publish a WebTransport Edgegap server image and a browser WASM client. The page starts the Bevy client, talks to Bevygap matchmaking over WebSocket, receives a Lightyear `ConnectToken` and certificate digest, then connects to the Edgegap game server over WebTransport.

Reference docs for this plan:

- Edgegap deployments: `https://docs.edgegap.com/docs/deployment/automated-deployment`
- Edgegap application versions: `https://docs.edgegap.com/learn/advanced-features/application-and-versions`
- Edgegap container registry: `https://docs.edgegap.com/docs/edgegap-container-registry`
- MDN WebTransport certificate hashes: `https://developer.mozilla.org/docs/Web/API/WebTransport/WebTransport`

1. Server container:
   - Build `lightrider-server --release --features bevygap` in a multi-stage container.
   - Runtime image should contain only the server binary, config, and entrypoint.
   - Entrypoint should run `lightrider-server --headless --bevygap --port ${PORT:-7777} --config ${LIGHTRIDER_CONFIG:-/app/config/default.ron}` by default.
   - Publish one Edgegap `gameport` mapping for WebTransport. The internal port can remain `7777`, but the Edgegap app version protocol must match the WebTransport-compatible deployment setup rather than raw UDP.
   - Required production env includes `LIGHTRIDER_PROTOCOL_ID`, `LIGHTRIDER_PRIVATE_KEY`, `LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1`, `NATS_HOST`, `NATS_USER`, `NATS_PASSWORD`, Edgegap/Arbitrium context env, and optional NATS TLS trust env.
   - Use Edgegap injected variables such as `ARBITRIUM_REQUEST_ID`, `ARBITRIUM_PUBLIC_IP`, `ARBITRIUM_PORTS_MAPPING`, and port-specific variables for diagnostics.

2. Registry and image publishing:
   - Keep Edgegap and registry credentials in ignored local/CI secrets.
   - Local image publishing uses `just edgegap-build`, `just edgegap-push`, and `just edgegap-build-push`.
   - Matchmaker/control publishing uses `just matchmaker-build`, `just matchmaker-push`, and `just matchmaker-build-push`.
   - `just prod-images-build` builds both production images locally; `just prod-images-push` pushes both.
   - The build context is generated under ignored `.edgegap-build/` because Lightrider currently depends on sibling `../lightyear` and optional sibling `../bevygap`.
   - Prefer immutable tags for future CI. The mutable `dev` tag is convenient but can confuse deployment diagnosis.
   - Before testing a deployment, verify the app version's `docker_image` and `docker_tag` match the intended Lightrider image.

3. Web client:
   - Current browser entrypoint is `lightrider-web`.
   - Browser entrypoint should read player name, room join mode, server host/port, and bootstrap data from URL params or fetched JSON.
   - The matchmaker/control image currently hosts static client files through nginx on `WEB_PORT` (default `8080`) and proxies `/matchmaker/*` to `bevygap_matchmaker_httpd`.
   - Raw UDP will not work in browser WASM; WebTransport is the chosen browser transport for this plan.

4. Matchmaking/control service:
   - Do not call Edgegap APIs directly from the browser. The Edgegap token must stay server-side.
   - Use `/spare/ssd/cbournhonesque/src/other/bevygap` as the starting point.
   - All Bevygap/Edgegap integration in Lightrider must be behind an optional Cargo feature, initially named `bevygap`. The default build must continue to support direct local WebTransport without pulling in Bevygap, NATS, Edgegap API, or extra service dependencies.
   - Relevant Bevygap utilities:
     - `bevygap_matchmaker_httpd`: public HTTP/WebSocket frontend; clients connect to `/matchmaker/ws`.
     - `bevygap_matchmaker`: private NATS service that creates Edgegap sessions, waits for readiness, generates Lightyear `ConnectToken`s, and schedules deletion.
     - `bevygap_shared`: shared `RequestSession`, `SessionRequestFeedback`, NATS bucket/stream setup, and protocol structs.
     - `bevy_nfws`: Bevy WebSocket client wrapper.
     - `bevygap_client_plugin`: reference client state machine for matchmaking and token-based Lightyear connect.
     - `bevygap_server_plugin`: reference server-side Edgegap context/NATS integration and active connection tracking.
     - `bevygap_webhook_sink`: optional Edgegap webhook receiver.
     - `edgegap_async`: generated Edgegap API client used by the matchmaker.
   - Preferred flow:
     1. Game client connects to `bevygap_matchmaker_httpd` over WebSocket.
     2. Client sends `RequestSession { game, version, client_ip, room }`, where `room` is auto, new, a private code, or a numeric room id.
     3. HTTPD publishes a NATS request to `bevygap_matchmaker`.
     4. Matchmaker reads game-server deployment metrics from NATS KV. For public auto-join it chooses a not-full public room or a warm deployment with room capacity; for private codes and numeric room ids it prefers the deployment already hosting that room; otherwise it creates a room on a warm deployment if capacity allows. If no warm deployment passes policy, it creates a new Edgegap session/deployment.
     5. When a new deployment is needed, the matchmaker creates an Edgegap session, waits until ready, reads public IP/external port, assigns a Lightyear client id, builds a `ConnectToken`, stores session/client mappings in NATS KV, and streams feedback.
     6. Client receives `SessionReady`, decodes token, configures the Lightyear connection, and connects to the game server.
     7. Game server handles the room request through normal `RoomJoinRequest` logic and reports active connection state through NATS.
     8. Matchmaker deletes sessions after disconnect or after unclaimed-session timeout.
   - NATS role in current Bevygap:
     - NATS core request/reply links `bevygap_matchmaker_httpd` to `bevygap_matchmaker`.
     - NATS JetStream KV stores client-id to Edgegap-session mappings, active connections, and WebTransport certificate digests.
     - Edgegap-hosted game servers must reach the same public NATS service so they can publish their context/digest and active connection state.
     - Game servers publish a small room-capacity heartbeat to the `deployment_metrics` KV bucket: deployment/request id, endpoint, current rooms, public/private marker, current humans, max rooms, max players per room, and optional CPU percentage.
     - In the current packaging plan, this NATS server is bundled into the matchmaker/control image for the simplest first deployment; it can be split into managed NATS or a separate VPS service later.
   - Required env vars are `NATS_HOST`, `NATS_USER`, `NATS_PASSWORD`, and optionally `NATS_CA` or `NATS_CA_CONTENTS` for TLS trust.
   - Recommended env var `BEVYGAP_NATS_NAMESPACE` must match between matchmaker/control services and Edgegap-hosted game servers for a given app/version.
   - Bevygap priority work:
     1. P0 complete: server readiness publication. The server plugin must publish context and WebTransport certificate digest only after NATS is connected, Edgegap context is loaded, the certificate digest is extracted, and the NATS sender exists. This avoids the previous one-shot `ContextLoaded` race.
     2. P0 complete baseline: production NATS setup hardening. NATS insecure mode is now an explicit truthy flag, and production mode can reject insecure NATS and default development credentials. Real deployment still needs actual TLS certificates, DNS/SAN choices, and firewall rules.
     3. P1 baseline complete: automate Edgegap app/version creation and env updates so the server image, app version, port protocol, NATS env vars, and netcode identity are less manual.
        - Current shape: `tools/edgegap_app_version.sh` plus `just` wrappers. The operator supplies image tag, Edgegap app/version, public NATS endpoint, and secret env overlay; the script creates/updates the app version and verifies that Edgegap now points at the exact server image/tag, UDP/7777 WebTransport game port, session config, and required env names.
        - Inputs should be split between checked-in defaults and ignored secrets. Checked-in config can define app name, internal port, protocol, config path, and safe env defaults. Ignored env files provide `EDGEGAP_API_KEY`, registry project, `NATS_HOST`, `NATS_USER`, `NATS_PASSWORD`, `LIGHTRIDER_PROTOCOL_ID`, `LIGHTRIDER_PRIVATE_KEY`, and optional NATS CA material.
        - Tool modes include `desired`, `show`, `diff`, `sync`, and `verify`. `diff` redacts secret values but shows missing/changed env keys, image repository/tag, port protocol, session config, and runtime env assumptions.
        - Output writes a redacted manifest under `.edgegap-build/edgegap-app-version.json` with app, version, image, tag, game port, protocol, env key list, and desired-payload hash so agents can compare deployments without printing secrets.
        - Current recipes: `edgegap-app-show`, `edgegap-app-desired`, `edgegap-app-diff`, `edgegap-app-sync`, and `edgegap-app-verify`. `edgegap-real-smoke` remains pending until public NATS is available.
     4. P1 complete: key certificate digests by deployment/request id first, endpoint `ip:port` second, and legacy public-IP keys last. This handles same-IP different-port deployments while preserving compatibility with older local data.
     5. P1 complete: add a cleaner local/mock mode for `bevygap_server_plugin`. `BEVYGAP_CONTEXT_MODE=local` synthesizes Arbitrium context from env/defaults instead of calling `ARBITRIUM_CONTEXT_URL`.
     6. P1 complete: NATS namespacing and session TTL polish. `BEVYGAP_NATS_NAMESPACE` now scopes Bevygap request subjects, gameserver announcements, delete-session streams, and JetStream KV buckets. Local recipes default to `lightrider_dev`; Edgegap app-version automation defaults to `<app>_<version>`. TTL envs exist for session mappings, unclaimed sessions, active connections, and certificate digests.
     7. P1 complete: recoverable async/runtime errors in the main Bevygap path now log or stream errors instead of panicking. The server plugin retries NATS connect/watch setup, NATS event sends handle closed channels, malformed KV values are rejected cleanly, and matchmaker/HTTPD request handling avoids request-path `unwrap`/`expect` panics.
     8. P1 complete: generated Edgegap client hygiene improved. `utils/gen-edgegap-client.sh` uses a pinned OpenAPI generator image by default, supports Podman or Docker, stores the downloaded spec under `target/`, applies only a small Cargo metadata post-process, and documents that generated source files should not be hand-edited.
     9. P1 complete: room-aware deployment packing. Lightrider servers publish per-room deployment metrics; the matchmaker uses configurable max players, max rooms, and max CPU policy before deciding whether to set `deployment_request_id` on a new Edgegap session or let Edgegap create a fresh deployment.
     10. P1 pending: run the real Edgegap-session smoke once the public NATS/control host is reachable and the app version has production env vars.
     11. P2 pending: extend `bevygap-local-smoke` with DuckDB movement assertions and add production browser bootstrap/player-name UI polish.
   - Implementation slices:
     1. Complete: add optional path dependencies under feature `bevygap`: client gates `bevygap_client_plugin`; server gates `bevygap_server_plugin`; shared code remains transport-only.
     2. Complete: add client CLI mode `--matchmaker-url <ws://...>` only when `bevygap` is enabled. Direct `--server-addr/--server-port` remains the default and stays available for local tests.
     3. Complete: refactor client network startup so it can spawn an unconnected Lightyear client entity, then either connect directly with manual auth or let Bevygap insert `NetcodeClient(Authentication::Token)`, `PeerAddr`, and `WebTransportClientIo`.
     4. Complete: add server CLI mode `--bevygap` only behind the feature. In direct mode, do not read Edgegap/NATS env vars. In Bevygap mode, add `BevygapServerPlugin` alongside the server WebTransport setup so the cert digest can be published.
     5. Complete: move `PROTOCOL_ID` and private key out of source constants for production. Direct dev can keep zero defaults, but Bevygap matchmaker and game server read the same nonzero values from env.
     6. Complete: extend `justfile` with feature-gated recipes. Local direct WebTransport remains unchanged; Bevygap recipes run NATS, `bevygap_matchmaker_httpd`, `bevygap_matchmaker`, and Lightrider with `--features bevygap`; the old fake context helper is retained but no longer needed by the default smoke.
     7. Complete: update Docker packaging/context for Bevygap. `edgegap-context` includes sibling `../bevygap`; `Dockerfile.server` builds the Bevygap-enabled game server; `Dockerfile.matchmaker` builds the matchmaker/control image with NATS and WASM assets.
     8. Partial: add a headless integration smoke that requests a token from matchmaker HTTPD and starts a bot client through the token path. `just bevygap-local-smoke` verifies the connect-token path through logs; DuckDB movement-row checks remain to be added.
     9. Complete: extend Bevygap protocol and placement for multi-room deployments. Room intent flows from browser/native client to matchmaker, room metrics flow from game server to NATS KV, and the matchmaker can reuse a warm Edgegap deployment for another room.
     10. Decide whether to keep NATS long term. If replacing it, preserve the same responsibilities: request/reply between HTTPD and matchmaker, session/client mappings with TTL/watch semantics, cert digest storage, deployment metrics, active connection tracking, and cleanup triggers.
   - Missing work:
     - Run a token-path smoke against real Edgegap session creation instead of the local mock session mode.
     - Build and run the new production images in containers; source checks, Docker dry-runs, context generation, and local native smoke pass, but full image builds were not run yet to avoid a large disk/CPU spike.
     - Extend `bevygap-local-smoke` to emit Lightyear debug JSONL and verify connection plus movement rows in DuckDB.
     - Browser UI now has a first-pass player-name/private-room modal. It still needs true spectator preview and no-reload Leptos-to-Bevy updates.
     - Stand up a public production NATS/control host with TLS, strong credentials, persistent storage, and restricted monitoring access.
   - Remaining server-side validation gap:
     - Real Edgegap active-connection validation still requires a public NATS/control host. The local mock flow now validates the server active-connection KV write and the matchmaker watcher, but it cannot prove Edgegap's real session cleanup until Edgegap-hosted game servers can reach NATS.
   - Historical runtime boundary fixes:
     - Bevygap native runtime boundaries needed fixes for this repo: the vendored Tokio task helper must not call `Runtime::block_on` when Bevy is already running inside a Tokio runtime, and native `bevy_nfws` WebSocket connections need their own Tokio runtime thread instead of Bevy's `IoTaskPool`.

## Known Bugs

- Low priority: client logs can emit `client::render::snake: DIAGONAL`. This should be investigated later; current gameplay is not blocked.
- High priority QOL: camera can be slightly jittery, especially when turning. Consider smoothing camera movement and confirm it follows the frame-interpolated visual position where appropriate.
- Historical deployment blocker: Edgegap deployment `60c89b835fed` started the correct raw UDP prototype server, but external native clients timed out and the server did not log incoming datagrams. The active plan has moved to WebTransport plus Bevygap.

## Update / Changelog

- Started this document with game design, general agent notes, and changelog sections.
- Upgraded the project to Bevy `0.18.1` and Lightyear main/current git dependency.
- Added typed RON config and default/test config files.
- Replaced physics-engine collision with explicit geometry.
- Added room-scoped movement, collision, food, scoring, respawn, rank, and visibility logic.
- Added BEI input integration through Lightyear and use of `Controlled` for locally controlled entities.
- Added custom snake interpolation and frame interpolation for predicted tail state.
- Added server bots and headless fake clients.
- Added rendering baseline: dark clear color, arena outline/axes, food dots, configurable snake head/tail gizmos, death view, minimap, leaderboard, names, and debug camera shortcuts.
- Added runtime debug tracing and `just trace-local`/`trace-summary` tooling for DuckDB analysis.
- Switched the active native deployment prototype from WebTransport to UDP.
- Added `Dockerfile.server`, container entrypoint, and podman-based `just edgegap-*` recipes.
- Stored Edgegap/registry/S3 settings in ignored `secrets/edgegap.env`; the S3 secret access key still needs to be supplied before log uploads can authenticate.
- Built and pushed `registry.edgegap.com/lightyear-6qgcf4w4mrq7/lightrider-server:dev`.
- Verified the UDP/Edgegap image work with `cargo fmt --all`, `CARGO_INCREMENTAL=0 cargo check --workspace -j 4`, `CARGO_INCREMENTAL=0 cargo test --workspace --lib -j 4`, `CARGO_INCREMENTAL=0 just trace-local 1 3 config/test.ron 5067`, container `--help`, local container startup, `just edgegap-build dev`, and `just edgegap-push dev`.
- Investigated deployment `390e2136cbf7`: Edgegap status had UDP mapping, but the app version pointed at old image `lightyear-6qgcf4w4mrq7/simple_box:a82d0d64b384e558639963c7ab5bc32c4669611b`; logs showed WebTransport certificate generation, explaining UDP client failure.
- Investigated deployment `60c89b835fed`: Edgegap status showed `139.177.195.29:30090 -> 7777/udp`, app version correctly pointed at `lightyear-6qgcf4w4mrq7/lightrider-server:dev`, and container logs showed `Starting UDP server on 0.0.0.0:7777` plus `Server UDP socket bound to 0.0.0.0:7777`. Repeated native headless clients timed out with `ConnectionRequestTimedOut`. A plain Python UDP datagram sent to `139.177.195.29:30090` also produced no server-side `Received UDP packet from new address ...` log. Current diagnosis: image and bind are correct, but UDP datagrams are not reaching the server process through the deployment path, or the Edgegap log endpoint is not returning live post-start logs. Next step is Edgegap/network-path diagnostics or a temporary UDP echo/probe build.
- Started the WebTransport + Bevygap path. Updated `/spare/ssd/cbournhonesque/src/other/bevygap` to Bevy `0.18` and local Lightyear `../lightyear/lightyear` with `client`, `server`, `netcode`, `replication`, and WebTransport features. Removed the stale `bevy_async_task` dependency from `bevy_nfws`, moved websocket tasks onto Bevy's `IoTaskPool`, updated observer APIs, updated matchmaker `ConnectToken` imports, changed the client plugin to insert `NetcodeClient`/`PeerAddr`/`WebTransportClientIo` on the existing Lightyear client entity, and changed the server plugin to observe current `Connected`/`Disconnected` ECS markers and extract certificate digest from `WebTransportServerIo`.
- Bevygap verification: `cargo check --workspace -j 4`, `cargo test --workspace --lib -j 4`, and `cargo tree -i bevy@0.18.1` pass in the Bevygap repo. `cargo tree -i bevy@0.15.1` reports no matching package, confirming the duplicate old Bevy dependency is gone. Remaining Bevygap warnings are unused/dead-code warnings in existing helper code plus one unused `Serialize` import in the HTTPD binary.
- Replaced Lightrider's direct Lightyear raw UDP IO with WebTransport. The server now generates a self-signed WebTransport identity at startup, logs its certificate digest, and includes local/Edgegap/`SELF_SIGNED_SANS` subject names. The direct client inserts `WebTransportClientIo`, accepts `--cert-digest`/`--certificate-digest`, and normalizes copied colon-separated digest logs; native local tests can leave the digest empty because the dangerous native test feature is enabled.
- WebTransport verification: `cargo check --workspace -j 4`, `cargo test --workspace --lib -j 4`, and `just trace-local 1 5 config/test.ron 5067` pass. The trace run wrote `logs/debug/20260527-132304`, logged WebTransport server startup, connected the native client with no certificate validation, produced movement rows, and reported zero stuck-snake rows and zero invariant violations.
- Added the Bevygap integration plan: Lightrider integration must be optional behind a `bevygap` feature, preserve direct local WebTransport as the default, gate client/server Bevygap dependencies separately, and use feature-gated recipes/tests for matchmaker-token smoke coverage.
- Implemented the first Lightrider Bevygap integration slice. `client` now has optional `bevygap` and `bevygap-matchmaker-tls` features, a feature-gated `--matchmaker-url` path, and split direct-vs-token connection startup. `server` now has an optional `bevygap` feature and feature-gated `--bevygap` mode. Default direct WebTransport builds do not require Bevygap/NATS/Edgegap services.
- Bevygap integration verification: `cargo fmt --all`, `cargo check --workspace -j 4`, `cargo check -p client --features bevygap -j 4`, `cargo check -p server --features bevygap -j 4`, `cargo test --workspace --lib -j 4`, default client dependency-tree check for Bevygap/NATS/Edgegap, and `just trace-local 1 4 config/test.ron 5068` pass. The feature builds are compile-checked only; the real matchmaker/NATS token flow is not yet runtime-tested.
- Reduced local build pressure: repo Cargo defaults now cap builds at two jobs, `justfile` recipes use `-j 2` plus `CARGO_INCREMENTAL=1`, dev profile keeps incremental compilation while lowering debug-info volume, dependencies/build scripts compile without dev debuginfo, Docker release builds use `-j 2` and remove build caches from the image build layer, and cleanup recipes exist for `cargo clean`, targeted incremental-cache cleanup, and `podman builder prune -f`.
- Pulled the local NATS container image with `podman pull nats:latest` and added Bevygap local-smoke recipes. Local NATS runs with JetStream, monitoring on `8222`, and development credentials `lightrider/lightrider`; the Lightrider server smoke uses a fake local Edgegap context endpoint before testing NATS context/cert-digest publication.
- Added mock Edgegap session mode to `/spare/ssd/cbournhonesque/src/other/bevygap/bevygap_matchmaker` and verified the native local Bevygap flow with `just bevygap-local-smoke 8 config/test.ron 7777 3000 9876 3001`. The passing run at `logs/bevygap/20260528-004801` covered NATS, fake Edgegap context, server cert-digest publication, mock `Session Ready`, Lightyear `ConnectToken` decode, WebTransport client connect, and server-side Lightyear connect observation.
- Hardened Bevygap local testing: native `bevy_nfws` now uses a dedicated Tokio runtime thread, the Bevygap server task helper skips nested `block_on` when Bevy already runs inside Tokio, raw connect tokens are redacted from matchmaker/HTTPD/client logs, and Lightrider disconnect cleanup no longer queues room-removal commands before immediate player/snake despawn.
- Added production netcode identity configuration. Lightrider server/client and Bevygap matchmaker now read `LIGHTRIDER_PROTOCOL_ID` plus `LIGHTRIDER_PRIVATE_KEY`, accept hex or comma-separated keys, and reject zero/dev identity when `LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1`. `just netcode-secret` prints a fresh env block.
- Wired Bevygap admissions into Lightyear. The local Lightyear fork exposes a netcode connection-request handler setter; `bevygap_server_plugin` watches Bevygap NATS KV for issued client ids, installs the handler before starting the server, and rejects tokens whose client id was not issued by the matchmaker.
- Added production packaging for the two-image plan. `Dockerfile.server` builds the Bevygap-enabled game server image, while `Dockerfile.matchmaker` bundles NATS, `bevygap_matchmaker`, `bevygap_matchmaker_httpd`, nginx, and the `lightrider-web` WASM client files.
- Added browser entrypoint and static client shell. `lightrider-web` defaults to same-origin `/matchmaker/ws` and accepts `?matchmaker_url=`; `web/index.html` loads the generated wasm-bindgen package.
- Added image recipes for the new plan: `matchmaker-build`, `matchmaker-push`, `matchmaker-build-push`, `prod-images-build`, and `prod-images-push`. `edgegap-context` now stages sibling `../bevygap` along with Lightrider and local Lightyear for container builds.
- Verification for this slice: `cargo fmt --all` in Lightrider/Bevygap/Lightyear; `cargo check -p server --features bevygap -j 2`; `cargo check -p client --features bevygap -j 2`; wasm `cargo check` for `lightrider-web`; Bevygap plugin/matchmaker/httpd checks; `cargo test -p shared --lib -j 2`; `just --dry-run` for the production image recipes; `just edgegap-context`; `bash -n` on both entrypoints; `git diff --check`; and `just bevygap-local-smoke 8 config/test.ron 7777 3000 9876 3001`, with the latest passing logs under `logs/bevygap/20260528-102335`.
- Added `DEPLOYMENT.md` with full operator steps for direct local WebTransport testing, local Bevygap mock smoke testing, real Edgegap-session local testing, and the current two-image production deployment plan.
- Clarified the real Edgegap-session local test: it uses real Edgegap sessions/deployments through our `bevygap_matchmaker`, does not require Edgegap's managed matchmaker, and needs a NATS endpoint reachable by both the local matchmaker and the Edgegap-hosted game server. The local recipes now also accept `EDGEGAP_API_TOKEN` from `secrets/edgegap.env`.
- Real Edgegap read-only preflight on 2026-05-28 verified `lightrider/v0.0.1` is active and the matchmaker can start/listen against the real Edgegap API without creating a session. The full real-session test is blocked until the Edgegap game-server app version has public NATS env vars and matching production netcode identity; the current app version has no env vars configured.
- Fixed Bevygap matchmaker's Edgegap API base path to avoid generated `//v1/...` URLs.
- Stored Linode control-host credentials in ignored `secrets/linode.env`. Linode API access works for instance `98268043` at `45.79.138.102`, the instance is running, and the local `lightrider_linode_ed25519.pub` fingerprint matches the Linode account key. Shell SSH could not be verified from this environment because outbound TCP/22 is blocked or filtered here; `github.com:22` also times out.
- Linode firewall check: firewall `default`/`26277595` is attached to the public Linode interface, but inbound policy is `ACCEPT`, so it is permissive and not a production-ready restriction. Recommended final posture is inbound default `DROP`, allow 80/443, restricted admin SSH, NATS 4222 only as narrowly as Edgegap allows, and no public 8222/8080.
- LISH/SSH analysis on 2026-05-28: direct node SSH on 22 times out from this host; node 443 and LISH 443 both establish TCP but close before SSH banner/auth (`kex_exchange_identification`), while LISH 2200 times out. Linode profile has `lish_auth_method=keys_only` with the expected key, so the 443 LISH failure is not a rejected key. Need web LISH/Cloud Manager console or a network path that can use TCP/22 to repair/inspect sshd.
- Weblish/Glish analysis on 2026-05-28: Cloud Manager HTTPS loads, but `us-east.webconsole.linode.com:8181` and `:8080` time out from this host. The browser console failure is consistent with local/corporate network blocking the Weblish/Glish gateway ports.
- Added prioritized Bevygap work tracking to this document. P0 items are server readiness publication and production NATS setup hardening; app/version automation, cert-digest keying, local/mock server-plugin mode, real Edgegap smoke, and richer smoke assertions remain queued.
- Hardened `bevygap_server_plugin` readiness publication so context and WebTransport certificate digest publication retries until NATS, Edgegap context, cert digest, and the NATS sender all exist, then publishes once.
- Hardened Bevygap NATS configuration: `NATS_INSECURE` is now parsed as a truthy flag instead of presence-only, `BEVYGAP_REQUIRE_SECURE_NATS=1` rejects insecure NATS and default development credentials, and the matchmaker/control entrypoint fails fast for production NATS setups without TLS unless explicitly allowed for temporary testing.
- Documented the hybrid Edgegap test path where the game server runs as a real Edgegap session while the matchmaker and web client run locally. This path does not need Edgegap's managed matchmaker, but it does need a NATS endpoint reachable by both the local matchmaker and Edgegap game-server container.
- Built and pushed the latest Bevygap-enabled game-server image as `registry.edgegap.com/lightyear-6qgcf4w4mrq7/lightrider-server:webtest-20260529-110706`.
- Set the production netcode protocol id to `1`, generated a 64-hex-character `LIGHTRIDER_PRIVATE_KEY`, and stored both in ignored `secrets/prod-netcode.env`.
- Fixed the matchmaker image build recipe so empty no-cache args do not trip `set -u`, exposed optional Podman build resource kwargs, and exposed opt-in Docker build args for release incremental compilation.
- Fixed Edgegap app-version automation against the live API: removed unsupported `build_type`, disabled forced image cache by default, stopped sending `req_cpu`/`req_memory` on PATCH updates, ignored Edgegap-normalized `verify_image`/port TLS defaults, normalized `session_config` ordering, and synced/verified app version `lightrider/webtest-20260529-110706` with server image tag `webtest-20260529-110706`.
- Verification for the Bevygap P0 slice: `cargo check --manifest-path ../bevygap/Cargo.toml -p bevygap_server_plugin -p bevygap_shared -j 2`, `cargo check -p server --features bevygap -j 2`, `bash -n tools/matchmaker-container-entrypoint.sh`, `git diff --check` in Lightrider and Bevygap, entrypoint fail-fast probes, and `just bevygap-local-smoke 8 config/test.ron 7777 3000 9876 3001` passed. The latest passing smoke logs are under `logs/bevygap/20260528-133903`.
- Added `tools/edgegap_app_version.sh` and `just` wrappers for Edgegap app-version automation. The script can print desired state, show current Edgegap state, redacted-diff desired vs current, sync by creating/updating the app version, and verify deploy-critical fields. It manages server image/tag, UDP/7777 game port, Bevygap session config, required Lightrider/NATS env vars, optional registry credentials, and writes a redacted `.edgegap-build/edgegap-app-version.json` manifest.
- Fixed the three Bevygap server-side gaps that were not blocked by public NATS: `bevygap_server_plugin` now supports `BEVYGAP_CONTEXT_MODE=local` for in-process local Edgegap context, WebTransport cert digests are published and resolved by deployment request id plus endpoint `ip:port` with legacy public-IP fallback, and the local mock flow now verifies active connection reporting through both the server KV write and matchmaker active-connection watcher. Verification: `cargo check -j 2 -p bevygap_server_plugin -p bevygap_matchmaker`, `cargo test -j 2 -p bevygap_shared --features nats`, `cargo test -j 2 -p bevygap_server_plugin local_context_contains_expected_endpoint`, `just --dry-run` for affected Bevygap recipes, and `just bevygap-local-smoke 3 config/test.ron 7777 3000 9876 3001` passed with logs under `logs/bevygap/20260528-144045`.
- Improved Bevygap P1 items 2-7: added `BEVYGAP_NATS_NAMESPACE` and shared subject/bucket helpers, exposed Bevygap TTL envs, routed matchmaker request subjects through shared helpers, hardened server-plugin NATS connect/watch/event handling, converted matchmaker/HTTPD runtime panics into logged or streamed errors, added unit tests for namespace naming, updated local/prod recipes and Edgegap app-version automation, and tightened the Edgegap OpenAPI client regeneration script. Verification: `cargo test -j 2 -p bevygap_shared --features nats`, `cargo check -j 2 -p bevygap_server_plugin -p bevygap_matchmaker -p bevygap_matchmaker_httpd`, script `bash -n` checks, `just --dry-run` for affected recipes, `cargo test -j 2 -p bevygap_server_plugin local_context_contains_expected_endpoint`, `git diff --check`, and `just bevygap-local-smoke 3 config/test.ron 7777 3000 9876 3001` passed with logs under `logs/bevygap/20260528-150923`.
- Started the browser UI implementation. Added the `web_client` workspace crate using Leptos CSR plus `leptos-bevy-canvas`, moved the `lightrider-web` WASM binary out of the native client crate, mounted Bevy on a fixed canvas selector, added the Powerline-style join modal, player-name field, four-letter private room field, and URL-driven browser settings. Added `RoomCode` and `RoomJoinMode::Private`, made server room assignment map codes to stable private rooms, and fixed public auto-join so it ignores private rooms. Updated `Dockerfile.matchmaker` to build `-p web_client --bin lightrider-web`. Verification: `cargo fmt --all`, `cargo test -j 2 -p shared --lib room_codes`, `cargo test -j 2 -p client --lib rooms`, rustup cargo wasm check for `web_client --features bevygap`, `cargo check -j 2 -p server --features bevygap`, `cargo check -j 2 -p client --features bevygap`, `cargo test -j 2 -p server --lib auto_room_does_not_assign_private_rooms`, and `cargo test -j 2 -p server --lib private_room_does_not_fall_back_to_public_when_room_limit_is_full`.
- Added multi-room Bevygap deployment packing. `RequestSession` now carries optional room intent, `bevygap_client_plugin` sends it, HTTPD forwards the full request, Lightrider clients map `RoomJoinMode` into Bevygap `RoomSelection`, game servers publish `BevygapDeploymentMetrics` from `RoomDirectory`, and `bevygap_matchmaker` reads deployment metrics from NATS KV to decide whether to reuse a warm Edgegap deployment or create a new one. Verification: `cargo test -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_shared`, `cargo test -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_matchmaker`, `cargo check -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_matchmaker -p bevygap_matchmaker_httpd -p bevygap_client_plugin -p bevygap_server_plugin`, `cargo test -j 2 -p shared --lib room_codes`, `cargo test -j 2 -p server --features bevygap room_metrics`, `cargo check -j 2 -p client --features bevygap`, and wasm `cargo check -j 2 -p web_client --target wasm32-unknown-unknown --features bevygap`.
- Added web-server/control-host automation. `tools/setup_web_server_host.sh` installs Podman, pulls the matchmaker/control image, writes a systemd service, persists NATS data, and exposes web/NATS ports. `just deploy-web-server host=<vps-ip>` builds/pushes the image, writes/refreshes an ignored shell env file from local Edgegap/netcode secrets, copies the env and script to the VPS, runs setup, removes the temporary uploaded env file, and health-checks the web endpoint. Verification: `bash -n tools/setup_web_server_host.sh`, `just --dry-run deploy-web-server host=45.79.138.102`, `just --dry-run web-server-env-template`, `just --dry-run web-server-install`, and `git diff --check`.
- Tuned the matchmaker/control image build after local `bevygap_matchmaker` release builds were killed by SIGKILL and a later WASM build missed `wasm32-unknown-unknown`. `Dockerfile.matchmaker` now installs the WASM target both before tool installation and after the repo copy, defaults to a 36GB-friendly profile (`jobs=2`, thin LTO, codegen units 8 for native; `jobs=2`, size opt, no LTO for WASM), and still supports low-memory overrides; `SKIP_IMAGE_BUILD=1 just deploy-web-server host=<vps-ip> tag=<tag>` can reinstall an already-pushed image without rebuilding locally, and `NO_CACHE=1 just matchmaker-build <tag>` forces a clean container build if the Rust target/toolchain cache is suspect.
- Added local browser UI test instructions and recipes. `just web-build` installs/checks the WASM target, keeps `wasm-bindgen-cli 0.2.122` under ignored `.edgegap-build/tools`, builds `web_client --features bevygap` with `rustup run nightly cargo` to avoid non-rustup Cargo wrappers missing the WASM sysroot, and writes `web/pkg`; `just web-serve` serves the static browser shell on `localhost:8000` for testing against the local mock Bevygap WebSocket endpoint. Renamed the ambiguous local gateway recipe to `bevygap-matchmaker-httpd-local` and added `bevygap-matchmaker-mock-stack-local` so the mock matchmaker worker and WebSocket gateway can be started together.
- Cleaned up local Bevygap logging and remote-browser docs. Game-server deployment metric writes are now debug logs, matchmaker unclaimed-session age logs use minute/second formatting and the periodic session-age line is debug-level, and `DEPLOYMENT.md` now calls out that SSH `-L` forwards only TCP, not the WebTransport/QUIC game-server UDP port.
