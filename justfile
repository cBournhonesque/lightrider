set shell := ["bash", "-cu"]

edgegap-default-tag := `git rev-parse --short HEAD 2>/dev/null || date +%Y%m%d%H%M%S`

export CARGO_BUILD_JOBS := "2"
export CARGO_INCREMENTAL := "1"

server config="config/test.ron" port="5000":
    cargo run -j 2 -p server --bin lightrider-server -- --headless --port {{port}} --config {{config}}

client id="1" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto":
    cargo run -j 2 -p client --bin lightrider-client -- --client-id {{id}} --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}}

client-debug id="1" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto":
    cargo run -j 2 -p client --bin lightrider-client -- --debug --client-id {{id}} --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}}

bot id="1001" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto":
    cargo run -j 2 -p client --bin lightrider-client -- --headless --mode bot --client-id {{id}} --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}}

bots count="4" first_id="1001" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto":
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'jobs -pr | xargs -r kill' EXIT
    for i in $(seq 0 $(({{count}} - 1))); do
      cargo run -j 2 -p client --bin lightrider-client -- --headless --mode bot --client-id $(({{first_id}} + i)) --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}} &
      sleep 1
    done
    wait

local bots="4" config="config/test.ron" port="5000" client_id="1" first_bot_id="1001" room="auto":
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'jobs -pr | xargs -r kill' EXIT
    cargo run -j 2 -p server --bin lightrider-server -- --headless --port {{port}} --config {{config}} &
    sleep 2
    for i in $(seq 0 $(({{bots}} - 1))); do
      cargo run -j 2 -p client --bin lightrider-client -- --headless --mode bot --client-id $(({{first_bot_id}} + i)) --server-port {{port}} --config {{config}} --room {{room}} &
      sleep 1
    done
    cargo run -j 2 -p client --bin lightrider-client -- --client-id {{client_id}} --server-port {{port}} --config {{config}} --room {{room}}

trace-local clients="4" seconds="20" config="config/test.ron" port="5000" first_client_id="2001" room="auto":
    #!/usr/bin/env bash
    set -euo pipefail
    run_dir="logs/debug/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$run_dir"
    ln -sfn "$(basename "$run_dir")" logs/debug/latest
    cargo build -j 2 -p server --bin lightrider-server -p client --bin lightrider-client
    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
    }
    trap cleanup EXIT
    RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/server.ndjson" \
      target/debug/lightrider-server --headless --port {{port}} --config {{config}} \
      > "$run_dir/server.log" 2>&1 &
    pids+=("$!")
    sleep 2
    for i in $(seq 0 $(({{clients}} - 1))); do
      id=$(({{first_client_id}} + i))
      RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-$id.ndjson" \
        target/debug/lightrider-client --headless --mode bot --client-id "$id" --server-port {{port}} --config {{config}} --room {{room}} \
        > "$run_dir/client-$id.log" 2>&1 &
      pids+=("$!")
      sleep 1
    done
    sleep {{seconds}}
    cleanup
    trap - EXIT
    echo "trace run: $run_dir"
    if command -v duckdb >/dev/null 2>&1; then
      duckdb -batch -cmd "SET VARIABLE trace_glob = '$run_dir/*.ndjson';" < tools/debug_trace_summary.sql | tee "$run_dir/summary.txt"
    else
      echo "duckdb not found; inspect $run_dir/*.ndjson manually"
    fi

trace-summary dir="logs/debug/latest":
    duckdb -batch -cmd "SET VARIABLE trace_glob = '{{dir}}/*.ndjson';" < tools/debug_trace_summary.sql

# Bevygap local smoke order:
# 1. `just bevygap-nats` starts local NATS with JetStream, which Bevygap uses for request/reply,
#    session/client KV mappings, certificate digest KV, and active connection tracking.
# 2. `just bevygap-fake-context` starts a tiny local Edgegap context endpoint. The current
#    Bevygap server plugin always fetches ARBITRIUM_CONTEXT_URL, even for local smoke tests.
# 3. `just bevygap-server-local` starts Lightrider with `--features bevygap -- --bevygap`
#    and should publish context plus WebTransport cert digest into NATS.
# 4. `just bevygap-matchmaker-mock-local` and `just bevygap-httpd-local` test the token path
#    without creating real Edgegap sessions.
# 5. `just bevygap-client-bot` requests a matchmaker token over WebSocket and connects with it.
# 6. `just bevygap-matchmaker-local` switches from mock sessions to real Edgegap sessions and
#    requires `EDGEGAP_API_KEY` in the environment or secrets.
# Print the ordered Bevygap local-smoke instructions.
bevygap-help:
    #!/usr/bin/env bash
    set -euo pipefail
    cat <<'EOF'
    Bevygap local order:
      1. just bevygap-nats-pull
      2. just bevygap-nats
      3. in another terminal: just bevygap-server-local

    That validates the server-side NATS/context/cert-digest path.

    Mock token flow:
      4. just bevygap-matchmaker-mock-local
      5. just bevygap-httpd-local
      6. just bevygap-client-bot

    One-command mock smoke:
      just bevygap-local-smoke

    Real Edgegap token flow:
      just bevygap-matchmaker-local app_name=<edgegap-app> app_version=<edgegap-version>

    The real token flow creates Edgegap sessions, so EDGEGAP_API_KEY must be exported
    or present in secrets/edgegap.env.

    The local server recipe uses BEVYGAP_CONTEXT_MODE=local by default, so it
    synthesizes Edgegap context in-process instead of requiring a fake context
    HTTP server.

    Local NATS uses:
      NATS_HOST=127.0.0.1:4222
      NATS_USER=lightrider
      NATS_PASSWORD=lightrider
      NATS_INSECURE=1
      BEVYGAP_NATS_NAMESPACE=lightrider_dev

    BEVYGAP_NATS_NAMESPACE scopes Bevygap buckets, streams, and subjects so
    multiple app versions can share one NATS instance without mixing sessions.
    EOF

bevygap-nats-pull:
    podman pull nats:latest

bevygap-nats:
    #!/usr/bin/env bash
    set -euo pipefail
    podman rm -f lightrider-nats >/dev/null 2>&1 || true
    podman run --rm --name lightrider-nats \
      -p 4222:4222 \
      -p 8222:8222 \
      nats:latest \
      -js \
      -m 8222 \
      --user lightrider \
      --pass lightrider

bevygap-nats-health:
    curl -fsS http://127.0.0.1:8222/healthz

bevygap-fake-context bind="127.0.0.1" port="9876" public_ip="127.0.0.1" game_port="7777":
    #!/usr/bin/env bash
    set -euo pipefail
    LIGHTRIDER_FAKE_CONTEXT_BIND="{{bind}}" \
    LIGHTRIDER_FAKE_CONTEXT_PORT="{{port}}" \
    LIGHTRIDER_FAKE_CONTEXT_PUBLIC_IP="{{public_ip}}" \
    LIGHTRIDER_FAKE_CONTEXT_GAME_PORT="{{game_port}}" \
      python3 - <<'PY'
    import json
    import os
    from http.server import BaseHTTPRequestHandler, HTTPServer

    bind = os.environ["LIGHTRIDER_FAKE_CONTEXT_BIND"]
    port = int(os.environ["LIGHTRIDER_FAKE_CONTEXT_PORT"])
    public_ip = os.environ["LIGHTRIDER_FAKE_CONTEXT_PUBLIC_IP"]
    game_port = int(os.environ["LIGHTRIDER_FAKE_CONTEXT_GAME_PORT"])

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            body = {
                "request_id": "local-lightrider",
                "public_ip": public_ip,
                "fqdn": "localhost",
                "sockets": 1,
                "location": {"city": "Local", "country": "Dev"},
                "ports": {
                    "game": {
                        "name": "game",
                        "internal": game_port,
                        "external": game_port,
                        "protocol": "UDP",
                    }
                },
            }
            data = json.dumps(body).encode("utf-8")
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

        def log_message(self, fmt, *args):
            print("fake-edgegap-context:", fmt % args)

    print(f"fake Edgegap context listening on http://{bind}:{port}/context/local-lightrider/1")
    HTTPServer((bind, port), Handler).serve_forever()
    PY

bevygap-server-local config="config/test.ron" port="7777" context_url="http://127.0.0.1:9876/context/local-lightrider/1":
    #!/usr/bin/env bash
    set -euo pipefail
    export NATS_HOST="${NATS_HOST:-127.0.0.1:4222}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export NATS_INSECURE="${NATS_INSECURE:-1}"
    export BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}"
    export BEVYGAP_CONTEXT_MODE="${BEVYGAP_CONTEXT_MODE:-local}"
    export ARBITRIUM_REQUEST_ID="${ARBITRIUM_REQUEST_ID:-local-lightrider}"
    export ARBITRIUM_DELETE_URL="${ARBITRIUM_DELETE_URL:-http://127.0.0.1:9876/delete/local-lightrider}"
    export ARBITRIUM_DELETE_TOKEN="${ARBITRIUM_DELETE_TOKEN:-local-delete-token}"
    export ARBITRIUM_DEPLOYMENT_LOCATION="${ARBITRIUM_DEPLOYMENT_LOCATION:-{\"city\":\"Local\",\"country\":\"Dev\"}}"
    export ARBITRIUM_CONTEXT_URL="${ARBITRIUM_CONTEXT_URL:-{{context_url}}}"
    export ARBITRIUM_CONTEXT_TOKEN="${ARBITRIUM_CONTEXT_TOKEN:-local-context-token}"
    export ARBITRIUM_PUBLIC_IP="${ARBITRIUM_PUBLIC_IP:-127.0.0.1}"
    export ARBITRIUM_PORTS_MAPPING="${ARBITRIUM_PORTS_MAPPING:-{\"game\":{\"internal\":{{port}},\"external\":{{port}},\"protocol\":\"UDP\"}}}"
    export SELF_SIGNED_SANS="${SELF_SIGNED_SANS:-127.0.0.1,localhost}"
    cargo run -j 2 -p server --features bevygap --bin lightrider-server -- --headless --bevygap --port {{port}} --config {{config}}

bevygap-matchmaker-local app_name="lightrider" app_version="dev":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f secrets/edgegap.env ]]; then
      source secrets/edgegap.env
    fi
    export NATS_HOST="${NATS_HOST:-127.0.0.1:4222}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export NATS_INSECURE="${NATS_INSECURE:-1}"
    export BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}"
    if [[ -z "${EDGEGAP_API_KEY:-}" && -n "${EDGEGAP_API_TOKEN:-}" ]]; then
      export EDGEGAP_API_KEY="$EDGEGAP_API_TOKEN"
    fi
    if [[ -z "${EDGEGAP_API_KEY:-}" && -n "${EDGEGAP_TOKEN:-}" ]]; then
      export EDGEGAP_API_KEY="$EDGEGAP_TOKEN"
    fi
    : "${EDGEGAP_API_KEY:?set EDGEGAP_API_KEY, EDGEGAP_API_TOKEN, or EDGEGAP_TOKEN in the environment or secrets/edgegap.env}"
    cargo run -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_matchmaker -- \
      --app-name {{app_name}} \
      --app-version {{app_version}} \
      --lightyear-protocol-id "${LIGHTRIDER_PROTOCOL_ID:-0}" \
      --max-players-per-deployment "${BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT:-800}" \
      --max-rooms-per-deployment "${BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT:-16}" \
      --max-cpu-percent-per-deployment "${BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT:-85}" \
      ${LIGHTRIDER_PRIVATE_KEY:+--lightyear-private-key "$LIGHTRIDER_PRIVATE_KEY"}

bevygap-matchmaker-mock-local app_name="lightrider" app_version="dev" public_ip="127.0.0.1" port="7777":
    #!/usr/bin/env bash
    set -euo pipefail
    export NATS_HOST="${NATS_HOST:-127.0.0.1:4222}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export NATS_INSECURE="${NATS_INSECURE:-1}"
    export BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}"
    cargo run -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_matchmaker -- \
      --app-name {{app_name}} \
      --app-version {{app_version}} \
      --lightyear-protocol-id "${LIGHTRIDER_PROTOCOL_ID:-0}" \
      --max-players-per-deployment "${BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT:-800}" \
      --max-rooms-per-deployment "${BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT:-16}" \
      --max-cpu-percent-per-deployment "${BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT:-85}" \
      ${LIGHTRIDER_PRIVATE_KEY:+--lightyear-private-key "$LIGHTRIDER_PRIVATE_KEY"} \
      --mock-edgegap \
      --mock-public-ip {{public_ip}} \
      --mock-external-port {{port}} \
      --mock-deployment-request-id "${ARBITRIUM_REQUEST_ID:-local-lightrider}"

bevygap-httpd-local bind="127.0.0.1:3000" cors="http://localhost:8000" fake_ip="81.128.157.100":
    #!/usr/bin/env bash
    set -euo pipefail
    export NATS_HOST="${NATS_HOST:-127.0.0.1:4222}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export NATS_INSECURE="${NATS_INSECURE:-1}"
    export BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}"
    cargo run -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_matchmaker_httpd -- \
      --bind {{bind}} \
      --cors {{cors}} \
      --fake-ip {{fake_ip}}

bevygap-client-bot id="3001" config="config/test.ron" matchmaker_url="ws://127.0.0.1:3000/matchmaker/ws" game="lightrider" version="dev" room="auto":
    cargo run -j 2 -p client --features bevygap --bin lightrider-client -- --headless --mode bot --client-id {{id}} --config {{config}} --room {{room}} --matchmaker-url {{matchmaker_url}} --matchmaker-game {{game}} --matchmaker-version {{version}}

bevygap-local-smoke seconds="8" config="config/test.ron" port="7777" httpd_port="3000" context_port="9876" client_id="3001":
    #!/usr/bin/env bash
    set -euo pipefail
    # context_port is retained for old command lines; the server now uses
    # BEVYGAP_CONTEXT_MODE=local and does not need a fake context HTTP server.
    run_dir="logs/bevygap/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$run_dir"
    ln -sfn "$(basename "$run_dir")" logs/bevygap/latest

    cargo build -j 2 -p server --features bevygap --bin lightrider-server
    cargo build -j 2 -p client --features bevygap --bin lightrider-client
    cargo build -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_matchmaker --bin bevygap_matchmaker
    cargo build -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_matchmaker_httpd --bin bevygap_matchmaker_httpd

    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
      podman rm -f lightrider-nats >/dev/null 2>&1 || true
    }
    trap cleanup EXIT

    just bevygap-nats > "$run_dir/nats.log" 2>&1 &
    pids+=("$!")
    for _ in $(seq 1 40); do
      curl -fsS http://127.0.0.1:8222/healthz >/dev/null 2>&1 && break
      sleep 0.25
    done
    curl -fsS http://127.0.0.1:8222/healthz > "$run_dir/nats-health.json"

    env \
      NATS_HOST="${NATS_HOST:-127.0.0.1:4222}" \
      NATS_USER="${NATS_USER:-lightrider}" \
      NATS_PASSWORD="${NATS_PASSWORD:-lightrider}" \
      NATS_INSECURE="${NATS_INSECURE:-1}" \
      BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}" \
      BEVYGAP_CONTEXT_MODE="${BEVYGAP_CONTEXT_MODE:-local}" \
      ARBITRIUM_REQUEST_ID="${ARBITRIUM_REQUEST_ID:-local-lightrider}" \
      ARBITRIUM_DELETE_URL="${ARBITRIUM_DELETE_URL:-http://127.0.0.1:{{context_port}}/delete/local-lightrider}" \
      ARBITRIUM_DELETE_TOKEN="${ARBITRIUM_DELETE_TOKEN:-local-delete-token}" \
      ARBITRIUM_DEPLOYMENT_LOCATION="${ARBITRIUM_DEPLOYMENT_LOCATION:-{\"city\":\"Local\",\"country\":\"Dev\"}}" \
      ARBITRIUM_CONTEXT_URL="${ARBITRIUM_CONTEXT_URL:-http://127.0.0.1:{{context_port}}/context/local-lightrider/1}" \
      ARBITRIUM_CONTEXT_TOKEN="${ARBITRIUM_CONTEXT_TOKEN:-local-context-token}" \
      ARBITRIUM_PUBLIC_IP="${ARBITRIUM_PUBLIC_IP:-127.0.0.1}" \
      ARBITRIUM_PORTS_MAPPING="${ARBITRIUM_PORTS_MAPPING:-{\"game\":{\"internal\":{{port}},\"external\":{{port}},\"protocol\":\"UDP\"}}}" \
      SELF_SIGNED_SANS="${SELF_SIGNED_SANS:-127.0.0.1,localhost}" \
      target/debug/lightrider-server --headless --bevygap --port {{port}} --config {{config}} \
      > "$run_dir/server.log" 2>&1 &
    pids+=("$!")
    for _ in $(seq 1 80); do
      if rg -q "CertDigest added|BevygapReady|CONTEXT added" "$run_dir/server.log"; then
        break
      fi
      sleep 0.25
    done

    env \
      NATS_HOST="${NATS_HOST:-127.0.0.1:4222}" \
      NATS_USER="${NATS_USER:-lightrider}" \
      NATS_PASSWORD="${NATS_PASSWORD:-lightrider}" \
      NATS_INSECURE="${NATS_INSECURE:-1}" \
      BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}" \
      ../bevygap/target/debug/bevygap_matchmaker \
        --app-name lightrider \
        --app-version dev \
        --lightyear-protocol-id "${LIGHTRIDER_PROTOCOL_ID:-0}" \
        ${LIGHTRIDER_PRIVATE_KEY:+--lightyear-private-key "$LIGHTRIDER_PRIVATE_KEY"} \
        --mock-edgegap \
        --mock-public-ip 127.0.0.1 \
        --mock-external-port {{port}} \
        --mock-deployment-request-id "${ARBITRIUM_REQUEST_ID:-local-lightrider}" \
      > "$run_dir/matchmaker.log" 2>&1 &
    pids+=("$!")
    for _ in $(seq 1 80); do
      if rg -q "Listening for session requests" "$run_dir/matchmaker.log"; then
        break
      fi
      sleep 0.25
    done

    env \
      NATS_HOST="${NATS_HOST:-127.0.0.1:4222}" \
      NATS_USER="${NATS_USER:-lightrider}" \
      NATS_PASSWORD="${NATS_PASSWORD:-lightrider}" \
      NATS_INSECURE="${NATS_INSECURE:-1}" \
      BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}" \
      ../bevygap/target/debug/bevygap_matchmaker_httpd \
        --bind "127.0.0.1:{{httpd_port}}" \
        --cors "http://localhost:8000" \
        --fake-ip "81.128.157.100" \
      > "$run_dir/httpd.log" 2>&1 &
    pids+=("$!")
    for _ in $(seq 1 80); do
      if rg -q "bevygap_matchmaker_httpd listening" "$run_dir/httpd.log"; then
        break
      fi
      sleep 0.25
    done

    timeout "$(({{seconds}} + 8))" \
      target/debug/lightrider-client \
        --headless --mode bot --client-id {{client_id}} --config {{config}} --room auto \
        --matchmaker-url "ws://127.0.0.1:{{httpd_port}}/matchmaker/ws" \
        --matchmaker-game lightrider \
        --matchmaker-version dev \
        > "$run_dir/client.log" 2>&1 || true

    sleep {{seconds}}

    required_patterns=(
      "Extracted cert digest"
      "CertDigest added"
      "Using mock Edgegap session"
      "Session Ready"
      "Got matchmaker response"
      "Connecting to server"
      "Lightyear connect event"
      "Active connection put"
      "Active connection put observed"
    )
    for pattern in "${required_patterns[@]}"; do
      if ! rg -q "$pattern" "$run_dir"; then
        echo "bevygap local smoke failed: missing '$pattern' in $run_dir" >&2
        echo "logs: $run_dir" >&2
        exit 1
      fi
    done

    echo "bevygap local smoke passed: $run_dir"

clean-build:
    cargo clean

clean-incremental:
    rm -rf target/debug/incremental target/*/debug/incremental

clean-edgegap-cache:
    podman builder prune -f

edgegap-context:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p .edgegap-build/context
    rsync -a --delete \
      --exclude .edgegap-build \
      --exclude .git \
      --exclude logs \
      --exclude pkg \
      --exclude secrets \
      --exclude target \
      ./ .edgegap-build/context/lightrider/
    rsync -a --delete \
      --exclude .git \
      --exclude target \
      ../lightyear/ .edgegap-build/context/lightyear/
    rsync -a --delete \
      --exclude .git \
      --exclude target \
      ../bevygap/ .edgegap-build/context/bevygap/

edgegap-login:
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    printf '%s' "$EDGEGAP_REGISTRY_TOKEN" | podman login "$EDGEGAP_REGISTRY_URL" \
      --username "$EDGEGAP_REGISTRY_USERNAME" \
      --password-stdin

edgegap-build tag=edgegap-default-tag: edgegap-context
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-server:{{tag}}"
    podman build \
      -f .edgegap-build/context/lightrider/Dockerfile.server \
      -t "$image" \
      .edgegap-build/context
    echo "$image" > .edgegap-build/server-image.txt
    echo "Built $image"

edgegap-push tag=edgegap-default-tag: edgegap-login
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-server:{{tag}}"
    podman push "$image"
    echo "Pushed $image"

edgegap-build-push tag=edgegap-default-tag:
    just edgegap-build {{tag}}
    just edgegap-push {{tag}}

edgegap-app-show version="dev" app="lightrider":
    tools/edgegap_app_version.sh show --app "{{app}}" --version "{{version}}"

edgegap-app-desired tag=edgegap-default-tag version="dev" app="lightrider":
    tools/edgegap_app_version.sh desired --app "{{app}}" --version "{{version}}" --tag "{{tag}}"

edgegap-app-diff tag=edgegap-default-tag version="dev" app="lightrider":
    tools/edgegap_app_version.sh diff --app "{{app}}" --version "{{version}}" --tag "{{tag}}"

edgegap-app-sync tag=edgegap-default-tag version="dev" app="lightrider":
    tools/edgegap_app_version.sh sync --app "{{app}}" --version "{{version}}" --tag "{{tag}}"

edgegap-app-verify tag=edgegap-default-tag version="dev" app="lightrider":
    tools/edgegap_app_version.sh verify --app "{{app}}" --version "{{version}}" --tag "{{tag}}"

matchmaker-build tag=edgegap-default-tag: edgegap-context
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-matchmaker:{{tag}}"
    # Defaults are balanced for a 32G+ build machine. Drop the env values to
    # 1/false/16 if rustc is OOM-killed on a smaller host.
    cache_args=()
    if [[ "${NO_CACHE:-0}" == "1" ]]; then
      cache_args+=(--no-cache)
    fi
    podman build \
      "${cache_args[@]}" \
      --build-arg "MATCHMAKER_CARGO_JOBS=${MATCHMAKER_CARGO_JOBS:-2}" \
      --build-arg "MATCHMAKER_RELEASE_OPT_LEVEL=${MATCHMAKER_RELEASE_OPT_LEVEL:-2}" \
      --build-arg "MATCHMAKER_RELEASE_LTO=${MATCHMAKER_RELEASE_LTO:-thin}" \
      --build-arg "MATCHMAKER_RELEASE_CODEGEN_UNITS=${MATCHMAKER_RELEASE_CODEGEN_UNITS:-8}" \
      --build-arg "WEB_CARGO_JOBS=${WEB_CARGO_JOBS:-2}" \
      --build-arg "WEB_RELEASE_OPT_LEVEL=${WEB_RELEASE_OPT_LEVEL:-s}" \
      --build-arg "WEB_RELEASE_LTO=${WEB_RELEASE_LTO:-false}" \
      --build-arg "WEB_RELEASE_CODEGEN_UNITS=${WEB_RELEASE_CODEGEN_UNITS:-16}" \
      -f .edgegap-build/context/lightrider/Dockerfile.matchmaker \
      -t "$image" \
      .edgegap-build/context
    echo "$image" > .edgegap-build/matchmaker-image.txt
    echo "Built $image"

matchmaker-push tag=edgegap-default-tag: edgegap-login
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-matchmaker:{{tag}}"
    podman push "$image"
    echo "Pushed $image"

matchmaker-build-push tag=edgegap-default-tag:
    just matchmaker-build {{tag}}
    just matchmaker-push {{tag}}

prod-images-build tag=edgegap-default-tag:
    just edgegap-build {{tag}}
    just matchmaker-build {{tag}}

prod-images-push tag=edgegap-default-tag:
    just edgegap-push {{tag}}
    just matchmaker-push {{tag}}

web-server-env-template tag=edgegap-default-tag file="secrets/web-server.env" host="45.79.138.102":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f "{{file}}" && "${FORCE:-0}" != "1" ]]; then
      echo "{{file}} already exists; set FORCE=1 to overwrite" >&2
      exit 1
    fi
    if [[ -f "{{file}}" ]]; then
      source "{{file}}"
    fi
    if [[ -f secrets/edgegap.env ]]; then
      source secrets/edgegap.env
    fi
    if [[ -f secrets/prod-netcode.env ]]; then
      source secrets/prod-netcode.env
    fi
    if [[ -z "${LIGHTRIDER_PROTOCOL_ID:-}" ]]; then
      LIGHTRIDER_PROTOCOL_ID="$(od -An -N8 -tu8 /dev/urandom | tr -d ' ')"
    fi
    if [[ -z "${LIGHTRIDER_PRIVATE_KEY:-}" ]]; then
      LIGHTRIDER_PRIVATE_KEY="$(openssl rand -hex 32)"
    fi
    if [[ -z "${NATS_PASSWORD:-}" ]]; then
      NATS_PASSWORD="$(openssl rand -hex 24)"
    fi
    mkdir -p "$(dirname "{{file}}")"
    write_env() {
      printf '%s=' "$1"
      printf '%q\n' "$2"
    }
    {
      echo "# Lightrider web-server/control-host install env."
      echo "# This file is shell-sourced locally, then converted to a container env file on the VPS."
      echo "# Keep LIGHTRIDER_PROTOCOL_ID/LIGHTRIDER_PRIVATE_KEY in sync with the Edgegap game-server app version."
      write_env LIGHTRIDER_MATCHMAKER_IMAGE "${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}/${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}/lightrider-matchmaker:{{tag}}"
      write_env LIGHTRIDER_MATCHMAKER_TAG "{{tag}}"
      write_env EDGEGAP_APP_NAME "${EDGEGAP_APP_NAME:-lightrider}"
      write_env EDGEGAP_APP_VERSION "${EDGEGAP_APP_VERSION:-{{tag}}}"
      write_env EDGEGAP_API_KEY "${EDGEGAP_API_KEY:-${EDGEGAP_API_TOKEN:-}}"
      write_env EDGEGAP_REGISTRY_URL "${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}"
      write_env EDGEGAP_REGISTRY_PROJECT "${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}"
      write_env EDGEGAP_REGISTRY_USERNAME "${EDGEGAP_REGISTRY_USERNAME:-}"
      write_env EDGEGAP_REGISTRY_TOKEN "${EDGEGAP_REGISTRY_TOKEN:-}"
      write_env LIGHTRIDER_PROTOCOL_ID "$LIGHTRIDER_PROTOCOL_ID"
      write_env LIGHTRIDER_PRIVATE_KEY "$LIGHTRIDER_PRIVATE_KEY"
      write_env LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE "1"
      write_env NATS_USER "${NATS_USER:-lightrider}"
      write_env NATS_PASSWORD "$NATS_PASSWORD"
      write_env NATS_ALLOW_INSECURE "${NATS_ALLOW_INSECURE:-1}"
      write_env MATCHMAKER_CORS "${MATCHMAKER_CORS:-http://{{host}}}"
      write_env BEVYGAP_NATS_NAMESPACE "${BEVYGAP_NATS_NAMESPACE:-lightrider_{{tag}}}"
      write_env BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT "${BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT:-800}"
      write_env BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT "${BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT:-16}"
      write_env BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT "${BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT:-85}"
    } > "{{file}}"
    chmod 600 "{{file}}"
    echo "Wrote {{file}}"

web-server-install host="45.79.138.102" ssh_port="22" env="secrets/web-server.env":
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{env}}" || {
      echo "{{env}} does not exist. Run: just web-server-env-template" >&2
      exit 1
    }
    remote_dir="/tmp/lightrider-web-server-setup"
    ssh -p "{{ssh_port}}" "root@{{host}}" "mkdir -p '$remote_dir'"
    scp -P "{{ssh_port}}" tools/setup_web_server_host.sh "root@{{host}}:$remote_dir/setup_web_server_host.sh"
    scp -P "{{ssh_port}}" "{{env}}" "root@{{host}}:$remote_dir/web-server.env"
    ssh -p "{{ssh_port}}" "root@{{host}}" "bash '$remote_dir/setup_web_server_host.sh' --env-file '$remote_dir/web-server.env'; rm -f '$remote_dir/web-server.env'"

web-server-health host="45.79.138.102":
    #!/usr/bin/env bash
    set -euo pipefail
    curl -fsS "http://{{host}}/" >/dev/null
    echo "web ok: http://{{host}}/"

deploy-web-server *args:
    #!/usr/bin/env bash
    set -euo pipefail
    vps_host=""
    ssh_port="22"
    tag="$(git rev-parse --short HEAD 2>/dev/null || date +%Y%m%d%H%M%S)"
    env_file="secrets/web-server.env"
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        host=*) vps_host="${arg#host=}" ;;
        ssh_port=*) ssh_port="${arg#ssh_port=}" ;;
        port=*) ssh_port="${arg#port=}" ;;
        tag=*) tag="${arg#tag=}" ;;
        env=*) env_file="${arg#env=}" ;;
        *)
          case "$positional" in
            0) vps_host="$arg" ;;
            1) ssh_port="$arg" ;;
            2) tag="$arg" ;;
            3) env_file="$arg" ;;
            *)
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    if [[ -z "$vps_host" ]]; then
      echo "usage: just deploy-web-server host=<vps-ip-or-host> [ssh_port=<port>] [tag=<tag>] [env=<file>]" >&2
      echo "example: just deploy-web-server host=45.79.138.102" >&2
      exit 2
    fi
    if [[ "${SKIP_IMAGE_BUILD:-0}" == "1" ]]; then
      echo "Skipping matchmaker image build/push; assuming tag $tag is already pushed."
    else
      just matchmaker-build-push "$tag"
    fi
    FORCE=1 just web-server-env-template "$tag" "$env_file" "$vps_host"
    just web-server-install "$vps_host" "$ssh_port" "$env_file"
    just web-server-health "$vps_host"

linode-control-env-template tag=edgegap-default-tag file="secrets/linode-control-host.env" host="45.79.138.102":
    just web-server-env-template {{tag}} {{file}} {{host}}

linode-control-install host="45.79.138.102" ssh_port="22" env="secrets/linode-control-host.env":
    just web-server-install {{host}} {{ssh_port}} {{env}}

linode-control-health host="45.79.138.102":
    just web-server-health {{host}}

netcode-secret:
    #!/usr/bin/env bash
    set -euo pipefail
    protocol_id="$(od -An -N8 -tu8 /dev/urandom | tr -d ' ')"
    private_key="$(openssl rand -hex 32)"
    printf 'LIGHTRIDER_PROTOCOL_ID=%s\n' "$protocol_id"
    printf 'LIGHTRIDER_PRIVATE_KEY=%s\n' "$private_key"
    printf 'LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1\n'
