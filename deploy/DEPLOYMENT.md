# Lightrider Deployment Guide

This guide is the operator checklist for running Lightrider locally and deploying
the production images.

The deployable images are:

- `lightrider-server`: game-server image used by the VPS/static provider,
  Edgegap, and GameFlow.
- `lightrider-matchmaker`: control image that runs NATS and
  `lightyear_matchmaker_server`.
- `lightrider-webclient`: static browser client image served by nginx.

The root `justfile` imports the deployment recipes under `deploy/`, so run the
commands below from the repository root.

## Quickstart Local

Use this path for local development. It runs the native server and a rendered
native client directly, without building container images.

Terminal 1:

```bash
just server config/test.ron 5000
```

Terminal 2:

```bash
just client 1 config/test.ron 127.0.0.1 5000 auto
```

For a larger arena, use the default config:

```bash
just server config/default.ron 5000
just client 1 config/default.ron 127.0.0.1 5000 auto
```

Optional browser asset smoke:

```bash
just web-serve 127.0.0.1 8000 false
```

## Quickstart Prod

Set the common variables once:

```bash
export TAG="$(just deploy-tag)"
export GAME_VERSION="dev"
export VPS_HOST="<vps-ip-or-hostname>"
export DOMAIN="<public-domain>"
export SSH_KEY="$HOME/.ssh/<key>"
```

`just deploy-tag` returns `<git-sha>-<utc timestamp>`. Use a fresh tag for each
production deploy so the VPS and Edgegap cannot keep running an older image that
was pushed with the same mutable tag.

Keep `GAME_VERSION` Edgegap-safe, such as `dev` or `prod`. It is the public
matchmaker/app version. It does not need to change when `TAG` changes.

Create the secret files once. `secrets/edgegap.env` must contain the registry
credentials used by the build/push recipes, and the Edgegap API token if you
deploy the Edgegap provider.

```bash
mkdir -p secrets
test -f secrets/prod-netcode.env || just netcode-secret > secrets/prod-netcode.env
"${EDITOR:-vi}" secrets/edgegap.env
```

Build and upload the VPS/static, Edgegap, matchmaker, and web-client images:

```bash
just prod-images-build-push tag="$TAG"
```

Build the GameFlow server upload zip. This archive has `Dockerfile` at the root,
as required by GameFlow, and builds the game server on port `7898/udp`.

```bash
just gameflow-server-zip output=.edgegap-build/gameflow/server.zip port=7898
```

Deploy the VPS/static local provider. This installs or updates the control host,
web client, matchmaker, NATS, and a static game-server service on the VPS.

```bash
SKIP_IMAGE_BUILD=1 just control-host-deploy \
  host="$VPS_HOST" \
  domain="$DOMAIN" \
  https=1 \
  ssh_key="$SSH_KEY" \
  tag="$TAG" \
  allocation_source=nats_static \
  game_version="$GAME_VERSION" \
  manage_static_server=1 \
  run_static_server=1
```

After HTTPS is working, optionally enable TLS for the public NATS endpoint.
Edgegap-hosted and GameFlow-hosted game servers should use this instead of
plaintext NATS once the command has succeeded.

```bash
just web-server-enable-nats-tls-from-caddy \
  host="$VPS_HOST" \
  domain="$DOMAIN" \
  ssh_key="$SSH_KEY"
```

Deploy the Edgegap provider. This syncs the Edgegap app version to the already
pushed `lightrider-server:$TAG` image.

```bash
# Plaintext NATS mode. Use this when the control host has NATS_ALLOW_INSECURE=1.
EDGEGAP_NATS_INSECURE=1 just edgegap-release-sync \
  tag="$TAG" \
  nats_host="$VPS_HOST:4222" \
  app=lightrider \
  version="$GAME_VERSION" \
  env_file=secrets/web-server.env

# TLS NATS mode. Use this only after verifying the public NATS endpoint reports
# "tls_required":true in its INFO line.
EDGEGAP_NATS_INSECURE=0 just edgegap-release-sync \
  tag="$TAG" \
  nats_host="$DOMAIN:4222" \
  app=lightrider \
  version="$GAME_VERSION" \
  env_file=secrets/web-server.env
```

Deploy the GameFlow provider. Upload `.edgegap-build/gameflow/server.zip` in
GameFlow, configure the primary game port as UDP `7898`, and set the server
runtime environment from `secrets/web-server.env`.

```bash
grep -E '^(LIGHTRIDER_PROTOCOL_ID|LIGHTRIDER_PRIVATE_KEY|NATS_USER|NATS_PASSWORD|LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE)=' secrets/web-server.env
printf 'LIGHTYEAR_MATCHMAKER_NATS_URL=tls://%s:4222\n' "$DOMAIN"
printf 'PORT=7898\nLIGHTRIDER_MATCHMAKER=1\nLIGHTRIDER_MATCHMAKER_PROVIDER=gameflow\n'
printf 'LIGHTRIDER_MATCHMAKER_GAME=lightrider\nLIGHTRIDER_MATCHMAKER_VERSION=%s\n' "$GAME_VERSION"
printf 'LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1\n'
```

Run one control-host matchmaker that routes between the static VPS server,
Edgegap, and GameFlow. Clients can force a provider with
`?provider=static`, `?provider=edgegap`, or `?provider=gameflow`; when no
provider is requested the router uses static first, then Edgegap, then GameFlow.

```bash
source secrets/edgegap.env

export GAMEFLOW_GAME_ID="<gameflow-game-id>"
export GAMEFLOW_API_KEY="<gameflow-api-key>"
export GAMEFLOW_MODE="fleet"
# Optional for standalone-style GameFlow setups:
# export GAMEFLOW_BUILD_ID="<gameflow-build-id>"
# export GAMEFLOW_REGION="<region>"

SKIP_IMAGE_BUILD=1 just gameflow-deploy-host \
  host="$VPS_HOST" \
  domain="$DOMAIN" \
  https=1 \
  ssh_key="$SSH_KEY" \
  tag="$TAG" \
  edgegap_version="$GAME_VERSION" \
  game_version="$GAME_VERSION" \
  allocation_source=provider_router \
  provider_router_default=nats_static \
  provider_router_fallback=edgegap,gameflow \
  manage_static_server=1 \
  run_static_server=1
```

Verify the host:

```bash
curl -fsS "https://$DOMAIN/" >/dev/null
ssh -i "$SSH_KEY" "root@$VPS_HOST" 'systemctl --no-pager --full status lightrider-matchmaker lightrider-webclient || true'
ssh -i "$SSH_KEY" "root@$VPS_HOST" 'podman logs --tail=200 lightrider-matchmaker'
```

## Provider Model

The matchmaker can run one allocation backend or a provider router:

- `nats_static`: allocate to a static game server that publishes capacity to
  NATS. This is the VPS/static local provider.
- `edgegap`: create or reuse Edgegap sessions.
- `gameflow`: create or reuse GameFlow sessions.
- `provider_router`: route between multiple backends. With the standard deploy
  command, the default is `nats_static` and the fallback order is
  `edgegap,gameflow`.

Clients can request a backend with `provider=static`, `provider=edgegap`, or
`provider=gameflow` in the browser URL. Without `provider=`, the router uses its
configured default and fallback order.

## Secrets

Keep secrets under `secrets/`; the directory is ignored by git.

`secrets/prod-netcode.env` is generated with:

```bash
just netcode-secret > secrets/prod-netcode.env
```

`secrets/edgegap.env` is sourced by the image build/push and Edgegap sync
recipes. A typical file contains:

```bash
EDGEGAP_REGISTRY_URL=registry.edgegap.com
EDGEGAP_REGISTRY_PROJECT=<project>
EDGEGAP_REGISTRY_USERNAME=<username>
EDGEGAP_REGISTRY_TOKEN=<registry-token>
EDGEGAP_API_TOKEN='token <edgegap-api-token>'
```

For GameFlow, export the API variables before deploying the control host, and
set the game-server runtime variables in the GameFlow dashboard:

```bash
export GAMEFLOW_GAME_ID=<gameflow-game-id>
export GAMEFLOW_API_KEY=<gameflow-api-key>
export GAMEFLOW_MODE=fleet
```

## Images

Build context is staged by:

```bash
just edgegap-context
```

That context includes:

- the current `lightrider` checkout,
- `../lightyear`,
- `../lightyear-matchmaker`,
- `../bevy_replicon`.

Normal production builds should use:

```bash
just prod-images-build-push tag="$TAG"
```

GameFlow does not consume the pushed game-server image in this path. It builds
the game-server image from a zip upload with `Dockerfile` at the archive root:

```bash
just gameflow-server-zip output=.edgegap-build/gameflow/server.zip port=7898
```

Upload `.edgegap-build/gameflow/server.zip` to GameFlow and configure the
primary game port as UDP `7898`. The zip Dockerfile builds the server with the
`gameflow` feature, which enables Agones ready/health calls and uses
`PORT=7898` by default.

If the Docker builder is the bottleneck, build the Linux server and matchmaker
binaries locally with `cargo-zigbuild`, build the web WASM locally, then create
runtime-only images that copy those artifacts:

```bash
brew install zig
cargo install cargo-zigbuild
just prebuilt-images-build-push tag="$TAG"
```

For a separate build/stage/push flow:

```bash
just prebuilt-artifacts
just prebuilt-images-build tag="$TAG"
just prebuilt-images-push tag="$TAG"
```

The default prebuilt target is Linux AMD64:

```text
zig_target=x86_64-unknown-linux-gnu.2.36
platform=linux/amd64
```

For Linux ARM64:

```bash
just prebuilt-images-build-push \
  tag="$TAG" \
  zig_target=aarch64-unknown-linux-gnu.2.36 \
  platform=linux/arm64
```

Use `cargo-zigbuild` here instead of `cross` because this path is specifically
for avoiding Rust compilation inside Docker/Podman. `cross` is useful in CI and
when you want a Linux container build environment, but it still compiles inside
a Docker-compatible container. On macOS that means the same Podman/Docker VM,
disk, cache, and `/var/tmp` failure modes can still be on the critical path.
`cargo-zigbuild` runs Cargo and Rust compilation locally, uses Zig for the Linux
linker/sysroot, and then the Docker image build only copies finished artifacts.
The prebuilt script also configures `cc-rs` build scripts to use `zig cc`,
which avoids failures from C dependencies such as `ring` looking for
`x86_64-linux-gnu-gcc`.

If the final native link fails with `ProcessFdQuotaExceeded`, the macOS shell
open-file limit is too low for the Rust link step. The prebuilt recipe raises
the limit to `8192` and uses one native Cargo job by default. You can ask for a
higher limit explicitly:

```bash
just prebuilt-images-build-push tag="$TAG" nofile_limit=16384 native_jobs=1
```

If the shell cannot raise the limit itself, run this first in the same terminal:

```bash
ulimit -n 16384
```

If a registry upload fails with a broken pipe while writing a blob, retry the
push phase without rebuilding:

```bash
just prebuilt-images-push tag="$TAG"
```

Pushes are retried four times by default. To use more attempts:

```bash
PODMAN_PUSH_RETRIES=8 PODMAN_PUSH_RETRY_SLEEP=15 just prebuilt-images-push tag="$TAG"
```

If a headless server cross-build fails in `wayland-sys` with a `pkg-config has
not been configured to support cross-compilation` panic, a windowing/rendering
feature leaked into the server build graph. The server prebuilt path should not
compile Wayland/X11 dependencies; keep Bevy's `default_platform` feature on the
rendered client only, not on the workspace-wide Bevy dependency used by the
server.

The native artifacts must still be Linux binaries. A normal macOS `cargo build`
produces Mach-O binaries and cannot run in the Debian runtime images. Check with:

```bash
file .edgegap-build/context/prebuilt/bin/lightrider-server
file .edgegap-build/context/prebuilt/bin/lightyear_matchmaker_server
```

Both should report `ELF`, and the architecture should match the image platform.

Build only one image when needed:

```bash
just game-server-build-push tag="$TAG"
just matchmaker-build-push tag="$TAG"
just webclient-build-push tag="$TAG"
```

Pull and restart already-pushed images on an existing VPS:

```bash
just control-host-pull-game-server host="$VPS_HOST" ssh_key="$SSH_KEY" tag="$TAG"
just control-host-pull-matchmaker host="$VPS_HOST" ssh_key="$SSH_KEY" tag="$TAG"
just control-host-pull-webclient host="$VPS_HOST" ssh_key="$SSH_KEY" tag="$TAG"
```

## VPS/Static Provider

Use this provider when the game server should run directly on the control VPS.
It is useful for low-cost production testing and for a fixed regional server.

Deploy or update:

```bash
SKIP_IMAGE_BUILD=1 just control-host-deploy \
  host="$VPS_HOST" \
  domain="$DOMAIN" \
  https=1 \
  ssh_key="$SSH_KEY" \
  tag="$TAG" \
  allocation_source=nats_static \
  manage_static_server=1 \
  run_static_server=1
```

Useful service commands on the VPS:

```bash
systemctl status lightrider-matchmaker --no-pager
systemctl status lightrider-webclient --no-pager
systemctl status lightrider-static-server --no-pager
journalctl -u lightrider-matchmaker -f
podman logs -f lightrider-static-server
```

## Edgegap Provider

Required local inputs:

- pushed `lightrider-server:$TAG` image,
- `secrets/edgegap.env` with registry and API credentials,
- `secrets/prod-netcode.env`,
- a public NATS endpoint reachable by Edgegap game servers.

Sync the app version:

```bash
# Plaintext NATS mode.
EDGEGAP_NATS_INSECURE=1 just edgegap-release-sync \
  tag="$TAG" \
  nats_host="$VPS_HOST:4222" \
  app=lightrider \
  version="$GAME_VERSION" \
  env_file=secrets/web-server.env

# TLS NATS mode, only after public NATS TLS is enabled and verified.
EDGEGAP_NATS_INSECURE=0 just edgegap-release-sync \
  tag="$TAG" \
  nats_host="$DOMAIN:4222" \
  app=lightrider \
  version="$GAME_VERSION" \
  env_file=secrets/web-server.env
```

If you call the lower-level script directly, pass plain values after the flags:

```bash
deploy/edgegap_app_version.sh sync \
  --app lightrider \
  --version "$GAME_VERSION" \
  --tag "$TAG"
```

Do not pass `--app "app=lightrider"` to the lower-level script. The `app=...`
syntax is for `just` recipe variables, not shell flags.

For an Edgegap-only matchmaker, switch the control host to Edgegap allocation:

```bash
SKIP_IMAGE_BUILD=1 just control-host-deploy \
  host="$VPS_HOST" \
  domain="$DOMAIN" \
  https=1 \
  ssh_key="$SSH_KEY" \
  tag="$TAG" \
  edgegap_version="$GAME_VERSION" \
  game_version="$GAME_VERSION" \
  allocation_source=edgegap \
  manage_static_server=1 \
  run_static_server=0
```

## GameFlow Provider

Required local inputs:

- `.edgegap-build/gameflow/server.zip`,
- GameFlow game/API credentials,
- a public NATS endpoint reachable by GameFlow game servers.

Build the GameFlow upload zip:

```bash
just gameflow-server-zip output=.edgegap-build/gameflow/server.zip port=7898
```

Upload `.edgegap-build/gameflow/server.zip` to GameFlow. In the GameFlow
dashboard, configure the primary game port as UDP `7898`, and set these runtime
environment variables for the game server:

```bash
PORT=7898
LIGHTRIDER_MATCHMAKER=1
LIGHTRIDER_MATCHMAKER_PROVIDER=gameflow
LIGHTRIDER_MATCHMAKER_GAME=lightrider
LIGHTRIDER_MATCHMAKER_VERSION=$GAME_VERSION
LIGHTYEAR_MATCHMAKER_NATS_URL=tls://$DOMAIN:4222
NATS_USER=<from secrets/web-server.env>
NATS_PASSWORD=<from secrets/web-server.env>
LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE=<from secrets/web-server.env>
LIGHTRIDER_PROTOCOL_ID=<from secrets/web-server.env>
LIGHTRIDER_PRIVATE_KEY=<from secrets/web-server.env>
LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1
```

For a GameFlow-only matchmaker, deploy the control host with the GameFlow
allocation backend:

```bash
export GAMEFLOW_GAME_ID="<gameflow-game-id>"
export GAMEFLOW_API_KEY="<gameflow-api-key>"
export GAMEFLOW_MODE="fleet"

SKIP_IMAGE_BUILD=1 just gameflow-deploy-host \
  host="$VPS_HOST" \
  domain="$DOMAIN" \
  https=1 \
  ssh_key="$SSH_KEY" \
  tag="$TAG" \
  allocation_source=gameflow \
  manage_static_server=1 \
  run_static_server=0
```

Use `GAMEFLOW_BUILD_ID` and `GAMEFLOW_REGION` when your GameFlow mode requires
an explicit build or region.

## Matchmaker URLs

With HTTPS enabled, the default browser matchmaker URL is:

```text
wss://<domain>/matchmaker/ws
```

Without HTTPS, the direct matchmaker URL is:

```text
ws://<vps-ip>:3000/ws
```

The deploy recipe writes the URL into the web-client environment as
`LIGHTRIDER_MATCHMAKER_URL`.

## Debugging Deployment Logs

Start with the control host. The matchmaker container writes separate logs for
NATS and the Lightyear matchmaker process:

```bash
ssh -i "$SSH_KEY" "root@$VPS_HOST" \
  'systemctl --no-pager --full status lightrider-matchmaker lightrider-static-server || true'

ssh -i "$SSH_KEY" "root@$VPS_HOST" \
  'podman exec lightrider-matchmaker tail -n 200 /var/log/lightyear_matchmaker_server.log'

ssh -i "$SSH_KEY" "root@$VPS_HOST" \
  'podman exec lightrider-matchmaker tail -n 200 /var/log/nats.log'

ssh -i "$SSH_KEY" "root@$VPS_HOST" \
  'podman logs --tail=200 lightrider-static-server'
```

Redact and inspect the generated deployment environment and matchmaker config:

```bash
ssh -i "$SSH_KEY" "root@$VPS_HOST" \
  'sed -E "s/(PASSWORD|PRIVATE_KEY|TOKEN|API_KEY|SECRET|KEY)=.*/\1=<redacted>/" /etc/lightrider/lightrider-matchmaker.env | sort'

ssh -i "$SSH_KEY" "root@$VPS_HOST" \
  'podman exec lightrider-matchmaker sh -lc '\''sed -E "s/(password = ).*/\1\"<redacted>\"/; s/(private_key = ).*/\1\"<redacted>\"/" /run/lightrider-matchmaker.toml'\'''
```

Interpret the matchmaker log by stage:

- `assignment.created` means the provider returned capacity and the matchmaker
  persisted an assignment for a specific `server_id`.
- `state_to="preparing"` means the client was told to wait while the selected
  game server polls NATS.
- `state_to="timed_out"` after `preparing` means the selected game server did
  not acknowledge the assignment. Check the hosted game-server env/logs for
  NATS URL, credentials, namespace, protocol id, private key, and server id.
- `provider_router.route_no_capacity` means that route had no usable capacity
  and the router is trying the next fallback route.

For Edgegap, compare the active app version against the desired runtime env:

```bash
deploy/edgegap_app_version.sh show --app lightrider --version "$GAME_VERSION"
deploy/edgegap_app_version.sh diff --app lightrider --version "$GAME_VERSION" --tag "$TAG"
```

If `edgegap-release-sync` updated env vars while Edgegap deployments were
already running, new sessions can still reuse an old container until Edgegap
stops it. The matchmaker log `server_id` for an Edgegap assignment is the
Edgegap deployment request id, so use it to inspect that container:

```bash
source secrets/edgegap.env
export EDGEGAP_API_BASE_URL="${EDGEGAP_API_BASE_URL:-https://api.edgegap.com}"
export EDGEGAP_REQUEST_ID="<server_id-from-assignment-log>"

curl -fsS \
  -H "Authorization: ${EDGEGAP_API_KEY:-${EDGEGAP_API_TOKEN:-${EDGEGAP_TOKEN:?}}}" \
  "$EDGEGAP_API_BASE_URL/v1/deployment/$EDGEGAP_REQUEST_ID/container-logs"
```

To force a fresh deployment after changing app-version env, stop the old
deployment from the Edgegap dashboard or use the bulk-stop API:

```bash
curl -fsS -X POST \
  -H "Authorization: ${EDGEGAP_API_KEY:-${EDGEGAP_API_TOKEN:-${EDGEGAP_TOKEN:?}}}" \
  -H "Content-Type: application/json" \
  "$EDGEGAP_API_BASE_URL/v1/deployments/bulk-stop" \
  --data-binary @- <<EOF
{
  "filters": [
    {
      "field": "request_id",
      "values": ["$EDGEGAP_REQUEST_ID"],
      "filter_type": "any"
    }
  ]
}
EOF
```

The Edgegap game-server env must match the control host on:

- `LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE`,
- `NATS_USER` / `NATS_PASSWORD`,
- `LIGHTRIDER_PROTOCOL_ID` / `LIGHTRIDER_PRIVATE_KEY`,
- NATS transport mode.

Check the public NATS mode with the server `INFO` line:

```bash
ssh -i "$SSH_KEY" "root@$VPS_HOST" "DOMAIN='$DOMAIN' bash -s" <<'EOF'
exec 3<>/dev/tcp/"$DOMAIN"/4222
IFS= read -r line <&3
printf '%s\n' "$line"
EOF
```

If the line contains `"tls_required":true`, sync Edgegap with
`EDGEGAP_NATS_INSECURE=0` and `nats_host="$DOMAIN:4222"`. If it does not,
sync Edgegap with `EDGEGAP_NATS_INSECURE=1` and `nats_host="$VPS_HOST:4222"`.

When NATS is in TLS mode, `nats_host` must be the DNS name on the certificate,
not the raw VPS IP. A NATS log line like `TLS handshake error: remote error:
tls: bad certificate` plus hosted game-server logs like `certificate not valid
for name "45.79.138.102"; certificate is only valid for
DnsName("45.79.138.102.sslip.io")` means the Edgegap app version is still using
the IP address in `NATS_HOST` or `LIGHTYEAR_MATCHMAKER_NATS_URL`. Re-run the
sync with the certificate name, then stop any old Edgegap deployment so the next
session starts with the updated environment:

```bash
EDGEGAP_NATS_INSECURE=0 just edgegap-release-sync \
  tag="$TAG" \
  nats_host="$DOMAIN:4222" \
  app=lightrider \
  version="$GAME_VERSION" \
  env_file=secrets/web-server.env
```

## Troubleshooting

If Podman or Buildah fails with `/var/tmp/... input/output error`, treat it as a
Podman VM or host storage failure, not as a Rust compile error. On macOS, the
fastest recovery path is usually:

```bash
podman machine stop
podman machine start
podman system df
podman builder prune -f
```

If the VM is too small for production image builds, resize or recreate it:

```bash
podman machine stop
podman machine set --disk-size 80 --memory 8192 --cpus 4
podman machine start
```

If resizing is not supported by your Podman version, recreate the machine after
backing up anything important:

```bash
podman machine stop
podman machine rm
podman machine init --disk-size 80 --memory 8192 --cpus 4
podman machine start
```

If Edgegap or GameFlow servers cannot join the matchmaker:

```bash
ssh -i "$SSH_KEY" "root@$VPS_HOST" 'podman exec lightrider-matchmaker tail -n 200 /var/log/nats.log'
ssh -i "$SSH_KEY" "root@$VPS_HOST" 'podman exec lightrider-matchmaker tail -n 200 /var/log/lightyear_matchmaker_server.log'
ssh -i "$SSH_KEY" "root@$VPS_HOST" "DOMAIN='$DOMAIN' bash -s" <<'EOF'
exec 3<>/dev/tcp/"$DOMAIN"/4222
IFS= read -r line <&3
printf '%s\n' "$line"
EOF
```

For the normal [NATS TLS mode](https://docs.nats.io/running-a-nats-service/configuration/securing_nats/tls),
`openssl s_client -connect "$DOMAIN:4222"` is not a valid health check. NATS
sends an initial plaintext `INFO` line first, then NATS clients upgrade to TLS
when that line advertises `"tls_required":true`. Seeing `wrong version number`
from direct OpenSSL usually means OpenSSL read the expected `INFO ` prefix.
Check the NATS line instead; it should contain `"tls_required":true`.

If the NATS line does not advertise required TLS, rebuild, push, redeploy with
the same tag, then rerun the TLS recipe:

```bash
just prod-images-build-push tag="$TAG"

SKIP_IMAGE_BUILD=1 just control-host-deploy \
  host="$VPS_HOST" \
  domain="$DOMAIN" \
  https=1 \
  ssh_key="$SSH_KEY" \
  tag="$TAG" \
  allocation_source=nats_static \
  game_version="$GAME_VERSION" \
  manage_static_server=1 \
  run_static_server=1

just web-server-enable-nats-tls-from-caddy \
  host="$VPS_HOST" \
  domain="$DOMAIN" \
  ssh_key="$SSH_KEY"
```

Check that the matchmaker allocation source matches the provider you are
testing, and that hosted game servers use the same `LIGHTRIDER_PROTOCOL_ID`,
`LIGHTRIDER_PRIVATE_KEY`, NATS credentials, and NATS namespace as the control
host.
