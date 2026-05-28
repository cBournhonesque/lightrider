# Lightrider Deployment Guide

This document is the operational checklist for running Lightrider locally and for deploying the current Bevygap/Edgegap production shape.

The current production shape uses two images:

- `lightrider-server`: the Edgegap game-server image. It runs the Bevy/Lightyear server with Bevygap enabled.
- `lightrider-matchmaker`: the public control/web image. It bundles NATS, `bevygap_matchmaker`, `bevygap_matchmaker_httpd`, nginx, and the browser WASM client files.

Do not put real credentials in this document. Store local secrets in `secrets/edgegap.env`, which is ignored by git.

## Prerequisites

Install or make available:

- Rust/Cargo with the repository toolchain.
- `just`.
- `podman` for NATS and production image builds.
- `curl`.
- `rg` for smoke-test log checks.
- `duckdb` if you want to run trace summaries.

The repository intentionally limits local Cargo work to two jobs through the `justfile`.

## Local Test Without Bevygap

This is the fastest development path. It does not use NATS, Bevygap, Edgegap, or the matchmaker. The native client connects directly to the local WebTransport server.

### One-Terminal Headless/Rendered Test

From the repo root:

```bash
just local 4 config/test.ron 5000 1 1001 auto
```

This starts:

- one local headless server on port `5000`,
- four headless bot clients,
- one rendered client with client id `1`.

Stop it with `Ctrl-C`.

### Separate Terminals

Terminal 1, server:

```bash
just server config/test.ron 5000
```

Terminal 2, optional bots:

```bash
just bots 4 1001 config/test.ron 127.0.0.1 5000 auto
```

Terminal 3, rendered client:

```bash
just client 1 config/test.ron 127.0.0.1 5000 auto
```

Debug rendered client:

```bash
just client-debug 1 config/test.ron 127.0.0.1 5000 auto
```

Single headless bot:

```bash
just bot 1001 config/test.ron 127.0.0.1 5000 auto
```

For a full-size arena, replace `config/test.ron` with `config/default.ron`.

### Runtime Trace Smoke

Use this when you want a non-visual check that the simulation connects and moves:

```bash
just trace-local 4 20 config/test.ron 5000 2001 auto
```

Output goes under `logs/debug/<timestamp>/`, and `logs/debug/latest` points at the most recent run. If `duckdb` is installed, the recipe writes a summary to `summary.txt`.

Rerun the summary on an existing trace:

```bash
just trace-summary logs/debug/latest
```

Expected result:

- server and client logs exist,
- `.ndjson` traces exist,
- the summary reports moving snake rows,
- stuck-snake and invariant-violation counts should be zero.

## Local Test With Bevygap

The local Bevygap path tests the production-style token flow without creating real Edgegap sessions:

1. NATS starts locally with JetStream.
2. Lightrider server starts with `--features bevygap -- --bevygap` and `BEVYGAP_CONTEXT_MODE=local`.
3. The server synthesizes local Edgegap context in-process.
4. The server publishes its context and WebTransport certificate digest into NATS.
5. `bevygap_matchmaker` runs in mock Edgegap mode.
6. `bevygap_matchmaker_httpd` exposes `/matchmaker/ws`.
7. A headless Lightrider bot client requests a token over WebSocket and connects to the game server with that token.

Local recipes default `BEVYGAP_NATS_NAMESPACE=lightrider_dev`. This prefixes Bevygap NATS subjects, streams, and KV buckets so local smoke tests do not collide with a real app/version using the same NATS server.

### One-Command Smoke

First pull the NATS image once:

```bash
just bevygap-nats-pull
```

Then run the smoke:

```bash
just bevygap-local-smoke 8 config/test.ron 7777 3000 9876 3001
```

Output goes under `logs/bevygap/<timestamp>/`, and `logs/bevygap/latest` points at the most recent run.

Expected terminal result:

```text
bevygap local smoke passed: logs/bevygap/<timestamp>
```

The smoke checks for:

- server certificate digest extraction,
- certificate digest publication to NATS,
- mock Edgegap session creation,
- matchmaker `Session Ready`,
- client matchmaker response,
- client connection attempt,
- server-side Lightyear connect event,
- server active-connection KV write,
- matchmaker active-connection watcher observation.

### Manual Bevygap Smoke

Use this when you want to inspect each process.

Terminal 1, NATS:

```bash
just bevygap-nats
```

Health check from another terminal:

```bash
just bevygap-nats-health
```

Terminal 2, Lightrider server with Bevygap:

```bash
just bevygap-server-local config/test.ron 7777
```

`bevygap-server-local` defaults `BEVYGAP_CONTEXT_MODE=local`, so it does not need a fake Edgegap context HTTP server.

Terminal 3, mock matchmaker:

```bash
just bevygap-matchmaker-mock-local lightrider dev 127.0.0.1 7777
```

Terminal 4, matchmaker HTTP/WebSocket frontend:

```bash
just bevygap-httpd-local 127.0.0.1:3000 http://localhost:8000 81.128.157.100
```

Terminal 5, headless token-path client:

```bash
just bevygap-client-bot 3001 config/test.ron ws://127.0.0.1:3000/matchmaker/ws lightrider dev auto
```

For a rendered native client through the matchmaker:

```bash
cargo run -j 2 -p client --features bevygap --bin lightrider-client -- \
  --client-id 1 \
  --config config/test.ron \
  --room auto \
  --matchmaker-url ws://127.0.0.1:3000/matchmaker/ws \
  --matchmaker-game lightrider \
  --matchmaker-version dev
```

### Local Bevygap With Real Edgegap Session Creation

This still runs NATS, HTTPD, and the client locally, but the matchmaker calls the real Edgegap API.

Requirements:

- `EDGEGAP_API_KEY`, `EDGEGAP_API_TOKEN`, or `EDGEGAP_TOKEN` exported, or present in `secrets/edgegap.env`.
- A valid Edgegap application and version matching the image you want to deploy.
- `LIGHTRIDER_PROTOCOL_ID` and `LIGHTRIDER_PRIVATE_KEY` exported if the real game server uses production netcode identity.
- A NATS instance reachable by both the local matchmaker and the Edgegap-hosted game server. A local `127.0.0.1:4222` NATS is not enough for this real-session flow unless it is exposed through a public TCP tunnel and the Edgegap app version points at that public address.

Start NATS and HTTPD as above, then run:

```bash
just bevygap-matchmaker-local lightrider <edgegap-version>
```

Then connect the client through:

```bash
just bevygap-client-bot 3001 config/default.ron ws://127.0.0.1:3000/matchmaker/ws lightrider <edgegap-version> auto
```

This creates real Edgegap sessions and should be used deliberately.

You do not need to activate Edgegap's managed matchmaker for this flow. `bevygap_matchmaker` is our matchmaker; it calls Edgegap's session API directly. You only need an active Edgegap app/version that can create sessions for the Lightrider server image.

The matchmaker start itself is a safe preflight: it verifies the app/version and listens for requests. The real Edgegap session/deployment is created only after a client request reaches `bevygap_matchmaker_httpd` and is forwarded to `bevygap_matchmaker`.

Current read-only preflight result, as of 2026-05-28:

- Edgegap app `lightrider` exists and is active.
- Active tested version is `v0.0.1`, not `dev`.
- Version `v0.0.1` points to `lightyear-6qgcf4w4mrq7/lightrider-server:dev`.
- Version `v0.0.1` currently has no app-version env vars configured, so it is not enough for the full real-session Bevygap flow. It still needs `NATS_HOST`, NATS credentials, and matching `LIGHTRIDER_PROTOCOL_ID`/`LIGHTRIDER_PRIVATE_KEY` before an Edgegap-hosted server can publish readiness and accept matchmaker-issued tokens.

## Production Deployment

Production currently means:

- browser clients load the WASM page from the matchmaker/control host,
- browser clients talk to `bevygap_matchmaker_httpd` through WebSocket,
- the matchmaker creates Edgegap sessions,
- the Edgegap-hosted game server publishes readiness and certificate digest through NATS,
- the matchmaker returns a Lightyear `ConnectToken`,
- the browser connects to the Edgegap server over WebTransport/QUIC.

### Linode Control Host

Current control-host target:

- Public IPv4: `45.79.138.102`
- Linode id: `98268043`
- Label: `debian-us-east`
- Region: `us-east`
- Intended role: public website/WASM client, HTTPS reverse proxy, `bevygap_matchmaker_httpd`, `bevygap_matchmaker`, and NATS.

Local secrets are stored in ignored `secrets/linode.env`. The private SSH key stays on this development host at `~/.ssh/lightrider_linode_ed25519`; only the matching public key should be installed on the Linode.

Access status as of 2026-05-28:

- Linode API access works with the stored token.
- The Linode instance is running.
- The local public key fingerprint matches the Linode account key labeled `lightrider`.
- Shell access from this host over TCP/22 could not be verified because outbound TCP/22 is blocked or filtered from this environment. `github.com:22`, `lish-us-east.linode.com:22`, and `lish-us-east.linode.com:2200` also time out from here, while SSH over TCP/443 to `ssh.github.com` works.
- Direct SSH to `45.79.138.102:443` reaches a TCP listener, but it closes before sending an SSH banner. That port is not currently SSH.
- LISH SSH to `lish-us-east.linode.com:443` also reaches TCP, but closes during `kex_exchange_identification` before authentication. The key is not being rejected; the SSH protocol exchange never reaches auth.
- LISH SSH to `lish-us-east.linode.com:2200` times out from this network.
- Weblish/Glish browser console does not load from this network because the region console gateway ports are blocked or unreachable here. `us-east.webconsole.linode.com:8181` and `us-east.webconsole.linode.com:8080` both timed out from this host.
- The Linode profile reports `lish_auth_method=keys_only`, and the profile has the matching public key.

Linode Cloud Firewall status as of 2026-05-28:

- Firewall id `26277595`, label `default`, is enabled and attached to the public Linode interface.
- Inbound policy is currently `ACCEPT`, outbound policy is `ACCEPT`.
- Explicit inbound rules allow SSH on TCP/22 and ICMP from all IPv4/IPv6 sources.
- Because inbound policy is `ACCEPT`, this firewall is permissive. It does not currently explain the TCP/22 timeout, and it is not the final production security posture.

Recommended production firewall shape after shell access is confirmed:

- Inbound default `DROP`.
- Allow TCP/22 only from trusted admin IPs, or configure SSH on a reachable admin port and restrict that.
- Allow TCP/80 and TCP/443 from `0.0.0.0/0` and `::/0` for the website and TLS.
- Allow TCP/4222 for NATS only from Edgegap game-server egress ranges if known. If Edgegap source ranges are not stable, keep NATS TLS enabled with strong credentials and monitor it closely.
- Do not expose TCP/8222 publicly; access NATS monitoring through SSH tunnel or private network.
- Do not expose TCP/8080 publicly once a reverse proxy owns 80/443.
- Keep outbound policy `ACCEPT`.

### 1. Prepare Local Secrets

Create or update `secrets/edgegap.env`:

```bash
mkdir -p secrets
chmod 700 secrets
$EDITOR secrets/edgegap.env
chmod 600 secrets/edgegap.env
```

The file should define registry values for local image build/push:

```bash
EDGEGAP_REGISTRY_URL=registry.edgegap.com
EDGEGAP_REGISTRY_PROJECT=<registry-project>
EDGEGAP_REGISTRY_USERNAME=<registry-username>
EDGEGAP_REGISTRY_TOKEN=<registry-token>
```

For real Edgegap session creation, also define one of:

```bash
EDGEGAP_API_KEY=<edgegap-api-token>
```

or:

```bash
EDGEGAP_API_TOKEN=<edgegap-api-token>
```

or:

```bash
EDGEGAP_TOKEN=<edgegap-api-token>
```

### 2. Generate Shared Netcode Identity

Generate the values once:

```bash
just netcode-secret
```

Store the printed values securely. The same values must be configured in both:

- the matchmaker/control container,
- the Edgegap game-server container.

The required variables are:

```bash
LIGHTRIDER_PROTOCOL_ID=<nonzero-u64>
LIGHTRIDER_PRIVATE_KEY=<64-hex-character-key>
LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1
```

Do not regenerate these independently for the matchmaker and server. If they differ, the matchmaker will issue tokens the game server cannot authenticate.

One convenient local option is to save them in an ignored shell file such as `secrets/prod-netcode.env` and source that file when running production image commands locally.

### 3. Build And Push Images

Choose a tag. Prefer an immutable tag such as a git SHA or date:

```bash
tag="$(git rev-parse --short HEAD)"
```

Build both images:

```bash
just prod-images-build "$tag"
```

Push both images:

```bash
just prod-images-push "$tag"
```

The recipes produce:

```text
registry.edgegap.com/<project>/lightrider-server:<tag>
registry.edgegap.com/<project>/lightrider-matchmaker:<tag>
```

You can build or push separately:

```bash
just edgegap-build "$tag"
just edgegap-push "$tag"
just matchmaker-build "$tag"
just matchmaker-push "$tag"
```

If disk pressure is high:

```bash
just clean-edgegap-cache
```

### 4. Run The Matchmaker/Control Image

Run this image on a public host. A VPS is the simplest first option.

The image exposes:

- `8080/tcp`: nginx static WASM client and `/matchmaker/*` WebSocket proxy.
- `4222/tcp`: NATS for Edgegap game servers.
- `8222/tcp`: NATS monitoring. Keep this private if possible.

Production should serve the web client over HTTPS. The image itself serves HTTP on `8080`, so put it behind a TLS reverse proxy such as Caddy, nginx, or your platform's load balancer. Browser WebTransport requires a secure browser context.

Example direct container run for initial testing before NATS TLS is configured:

```bash
source secrets/edgegap.env
source secrets/prod-netcode.env

podman run -d --name lightrider-matchmaker \
  -p 8080:8080 \
  -p 4222:4222 \
  -p 127.0.0.1:8222:8222 \
  -v lightrider-nats-data:/data/nats \
  -e EDGEGAP_API_KEY="$EDGEGAP_API_KEY" \
  -e EDGEGAP_APP_NAME=lightrider \
  -e EDGEGAP_APP_VERSION="$tag" \
  -e LIGHTRIDER_PROTOCOL_ID="$LIGHTRIDER_PROTOCOL_ID" \
  -e LIGHTRIDER_PRIVATE_KEY="$LIGHTRIDER_PRIVATE_KEY" \
  -e LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1 \
  -e NATS_ALLOW_INSECURE=1 \
  -e NATS_USER=<strong-nats-user> \
  -e NATS_PASSWORD=<strong-nats-password> \
  -e MATCHMAKER_CORS=https://<your-domain> \
  registry.edgegap.com/<project>/lightrider-matchmaker:"$tag"
```

`NATS_ALLOW_INSECURE=1` is an explicit temporary override. Remove it for production NATS TLS.

For a real public deployment, prefer:

- TLS in front of `8080`.
- TLS for NATS on `4222`, or at minimum a private network/firewall allowlist.
- A persistent volume mounted at `/data/nats`.
- Strong `NATS_USER` and `NATS_PASSWORD`.
- `8222` not exposed publicly.

If using NATS TLS inside the matchmaker image, mount the cert/key into the container and provide:

```bash
NATS_TLS_CERT=/path/in/container/cert.pem
NATS_TLS_KEY=/path/in/container/key.pem
BEVYGAP_REQUIRE_SECURE_NATS=1
```

With `BEVYGAP_REQUIRE_SECURE_NATS=1`, the matchmaker/control container refuses `NATS_INSECURE` and refuses default `lightrider/lightrider` NATS credentials. `NATS_INSECURE` is parsed as a truthy flag, so `NATS_INSECURE=0` and `NATS_INSECURE=false` do not disable TLS.

If Edgegap game servers connect to public NATS with TLS, configure their trust with `NATS_CA` or `NATS_CA_CONTENTS`.

Health checks:

```bash
curl -fsS http://<matchmaker-host>:8080/
curl -fsS http://<matchmaker-host>:8222/healthz
```

Container logs to inspect:

```bash
podman logs lightrider-matchmaker
podman exec lightrider-matchmaker tail -n 200 /var/log/nats.log
podman exec lightrider-matchmaker tail -n 200 /var/log/bevygap_matchmaker.log
podman exec lightrider-matchmaker tail -n 200 /var/log/bevygap_matchmaker_httpd.log
```

### 5. Configure The Edgegap Game-Server App Version

The preferred path is to let the local sync script create or update the app version:

```bash
source secrets/edgegap.env
source secrets/prod-netcode.env

export NATS_HOST=<public-matchmaker-or-nats-host>:4222
export NATS_USER=<same-nats-user>
export NATS_PASSWORD=<same-nats-password>

just edgegap-app-diff "$tag" "$tag" lightrider
just edgegap-app-sync "$tag" "$tag" lightrider
just edgegap-app-verify "$tag" "$tag" lightrider
```

The first argument is the server image tag. The second argument is the Edgegap app-version name. They can be the same value, but they do not have to be.

The sync script calls Edgegap's app-version API and manages:

- server image repository/image/tag,
- internal game port `7777`,
- protocol `UDP`,
- session config for Bevygap-created sessions,
- required server env vars,
- registry pull credentials when present in `secrets/edgegap.env`.

It redacts secrets in terminal output and writes a redacted desired manifest to `.edgegap-build/edgegap-app-version.json`.

Manual equivalent: create or update the Edgegap app/version to use:

```text
Image: registry.edgegap.com/<project>/lightrider-server:<tag>
Internal port: 7777
Protocol: UDP
```

The protocol is listed as UDP because WebTransport runs over QUIC/UDP. This is not the old raw-UDP gameplay path.

Configure these environment variables on the Edgegap app version:

```bash
PORT=7777
LIGHTRIDER_CONFIG=/app/config/default.ron
LIGHTRIDER_BEVYGAP=1
LIGHTRIDER_PROTOCOL_ID=<same-value-as-matchmaker>
LIGHTRIDER_PRIVATE_KEY=<same-value-as-matchmaker>
LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1
NATS_HOST=<public-matchmaker-or-nats-host>:4222
NATS_USER=<same-nats-user>
NATS_PASSWORD=<same-nats-password>
BEVYGAP_NATS_NAMESPACE=<app-or-app-version-namespace>
```

Optional Bevygap TTL tuning envs are `BEVYGAP_SESSION_MAPPING_TTL_MS`, `BEVYGAP_UNCLAIMED_SESSION_TTL_SECS`, `BEVYGAP_ACTIVE_CONNECTION_TTL_SECS`, and `BEVYGAP_CERT_DIGEST_TTL_SECS`. Defaults are suitable for the first deployment; set them only when you have a clear retention reason.

If NATS is not using TLS during early testing, also set:

```bash
NATS_INSECURE=1
```

For production NATS TLS, prefer leaving `NATS_INSECURE` unset and provide trust material:

```bash
BEVYGAP_REQUIRE_SECURE_NATS=1
NATS_CA_CONTENTS=<pem-ca-contents>
```

or:

```bash
NATS_CA=<path-in-container>
```

Edgegap should inject the Arbitrium context variables used by Bevygap, including request id, context URL, public IP, and port mapping. The server entrypoint defaults to:

```bash
/app/lightrider-server --headless --bevygap --port "$PORT" --config "$LIGHTRIDER_CONFIG"
```

### 6. Verify Production Flow

Open the web client:

```text
https://<your-domain>/
```

If the matchmaker WebSocket is not same-origin, pass it explicitly:

```text
https://<your-domain>/?matchmaker_url=wss://<matchmaker-domain>/matchmaker/ws
```

Expected flow:

1. Browser loads `index.html` and `pkg/lightrider_web.js`.
2. Browser connects to `/matchmaker/ws`.
3. Matchmaker creates or reuses an Edgegap session.
4. Edgegap starts a `lightrider-server` deployment if needed.
5. Game server publishes its context and certificate digest to NATS.
6. Matchmaker returns `SessionReady` with a Lightyear connect token and certificate digest.
7. Browser connects to the Edgegap external game port over WebTransport.
8. Game server logs a Lightyear connect event.

Useful native token-path test:

```bash
cargo run -j 2 -p client --features bevygap,bevygap-matchmaker-tls --bin lightrider-client -- \
  --client-id 1 \
  --config config/default.ron \
  --room auto \
  --matchmaker-url wss://<matchmaker-domain>/matchmaker/ws \
  --matchmaker-game lightrider \
  --matchmaker-version "$tag"
```

If testing against a non-TLS local/public matchmaker endpoint, use only `--features bevygap` and `ws://...`.

Logs that should appear:

- Matchmaker: session request accepted, Edgegap session ready, `SessionReady` sent.
- Game server: context loaded, certificate digest extracted, Bevygap client id admitted, Lightyear connect event.
- Client: matchmaker response received, connecting to server, connected.

### 7. Production Failure Checklist

If the browser does not load:

- Confirm HTTPS is configured in front of the matchmaker/control host.
- Confirm nginx is serving `/` and `/pkg/lightrider_web.js`.
- Check browser console for WASM load errors.

If the client cannot reach the matchmaker:

- Confirm `/matchmaker/ws` is proxied with WebSocket upgrade headers.
- Confirm `MATCHMAKER_CORS` matches the public web origin.
- Check `bevygap_matchmaker_httpd.log`.

If sessions are not created:

- Confirm `EDGEGAP_API_KEY`.
- Confirm `EDGEGAP_APP_NAME` and `EDGEGAP_APP_VERSION`.
- Confirm the Edgegap app version points at the pushed `lightrider-server:<tag>` image.
- Check `bevygap_matchmaker.log`.

If the matchmaker waits forever for readiness:

- Confirm the Edgegap server has `--bevygap` enabled through `LIGHTRIDER_BEVYGAP=1`.
- Confirm the game server can reach `NATS_HOST`.
- Confirm `NATS_USER` and `NATS_PASSWORD` match.
- Confirm `BEVYGAP_NATS_NAMESPACE` matches between the matchmaker/control service and the Edgegap game-server app version.
- Confirm NATS TLS/insecure settings match on both sides.
- Check Edgegap game-server logs for context and certificate digest publication.

If the token is rejected:

- Confirm `LIGHTRIDER_PROTOCOL_ID` and `LIGHTRIDER_PRIVATE_KEY` are identical on matchmaker and game server.
- Confirm `LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1` is set on both in production.
- Confirm the Bevygap server admission watcher logs the issued client id before the client connects.

If WebTransport connection fails:

- Confirm the Edgegap app version exposes internal port `7777` with UDP protocol.
- Confirm Edgegap reports an external UDP port for the deployment/session.
- Confirm the server published a certificate digest and the client received it in `SessionReady`.
- Confirm the browser page is running in a secure context.
