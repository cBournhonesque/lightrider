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
# 4. `just bevygap-matchmaker-mock-stack-local` starts the matchmaker worker plus
#    WebSocket HTTPD gateway without creating real Edgegap sessions.
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
      4. just bevygap-matchmaker-mock-stack-local
      5. just bevygap-client-bot

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
    for env_file in secrets/edgegap.env secrets/prod-netcode.env secrets/nats.env; do
      if [[ -f "$env_file" ]]; then
        source "$env_file"
      fi
    done
    export NATS_HOST="${NATS_HOST:-127.0.0.1:4222}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export NATS_INSECURE="${NATS_INSECURE:-1}"
    export BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}"
    echo "bevygap-matchmaker-local: NATS_HOST=$NATS_HOST NATS_USER=$NATS_USER NATS_INSECURE=$NATS_INSECURE BEVYGAP_NATS_NAMESPACE=$BEVYGAP_NATS_NAMESPACE"
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
      --cert-digest-timeout-ms "${BEVYGAP_CERT_DIGEST_LOOKUP_TIMEOUT_MS:-15000}" \
      --cert-digest-poll-ms "${BEVYGAP_CERT_DIGEST_LOOKUP_POLL_MS:-200}" \
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
      --cert-digest-timeout-ms "${BEVYGAP_CERT_DIGEST_LOOKUP_TIMEOUT_MS:-15000}" \
      --cert-digest-poll-ms "${BEVYGAP_CERT_DIGEST_LOOKUP_POLL_MS:-200}" \
      ${LIGHTRIDER_PRIVATE_KEY:+--lightyear-private-key "$LIGHTRIDER_PRIVATE_KEY"} \
      --mock-edgegap \
      --mock-public-ip {{public_ip}} \
      --mock-external-port {{port}} \
      --mock-deployment-request-id "${ARBITRIUM_REQUEST_ID:-local-lightrider}"

bevygap-matchmaker-httpd-local bind="127.0.0.1:3000" cors="http://localhost:8000" fake_ip="81.128.157.100":
    #!/usr/bin/env bash
    set -euo pipefail
    for env_file in secrets/nats.env; do
      if [[ -f "$env_file" ]]; then
        source "$env_file"
      fi
    done
    export NATS_HOST="${NATS_HOST:-127.0.0.1:4222}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export NATS_INSECURE="${NATS_INSECURE:-1}"
    export BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE:-lightrider_dev}"
    echo "bevygap-matchmaker-httpd-local: NATS_HOST=$NATS_HOST NATS_USER=$NATS_USER NATS_INSECURE=$NATS_INSECURE BEVYGAP_NATS_NAMESPACE=$BEVYGAP_NATS_NAMESPACE"
    cargo run -j 2 --manifest-path ../bevygap/Cargo.toml -p bevygap_matchmaker_httpd -- \
      --bind {{bind}} \
      --cors {{cors}} \
      --fake-ip {{fake_ip}}

bevygap-matchmaker-mock-stack-local app_name="lightrider" app_version="dev" public_ip="127.0.0.1" game_port="7777" bind="127.0.0.1:3000" cors="http://localhost:8000" fake_ip="81.128.157.100":
    #!/usr/bin/env bash
    set -euo pipefail
    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
    }
    trap cleanup EXIT INT TERM
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
      --cert-digest-timeout-ms "${BEVYGAP_CERT_DIGEST_LOOKUP_TIMEOUT_MS:-15000}" \
      --cert-digest-poll-ms "${BEVYGAP_CERT_DIGEST_LOOKUP_POLL_MS:-200}" \
      ${LIGHTRIDER_PRIVATE_KEY:+--lightyear-private-key "$LIGHTRIDER_PRIVATE_KEY"} \
      --mock-edgegap \
      --mock-public-ip {{public_ip}} \
      --mock-external-port {{game_port}} \
      --mock-deployment-request-id "${ARBITRIUM_REQUEST_ID:-local-lightrider}" &
    pids+=("$!")
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

web-build:
    #!/usr/bin/env bash
    set -euo pipefail
    rustup target add wasm32-unknown-unknown
    tool_root=".edgegap-build/tools"
    wasm_bindgen="$tool_root/bin/wasm-bindgen"
    need_wasm_bindgen=1
    if [[ -x "$wasm_bindgen" ]]; then
      version="$("$wasm_bindgen" --version | awk '{print $2}')"
      if [[ "$version" == "0.2.122" ]]; then
        need_wasm_bindgen=0
      fi
    fi
    if [[ "$need_wasm_bindgen" == "1" ]]; then
      rustup run nightly cargo install \
        wasm-bindgen-cli \
        --version 0.2.122 \
        --locked \
        --force \
        --root "$tool_root"
    fi
    rustup run nightly cargo build -j 2 \
      -p web_client \
      --features bevygap \
      --bin lightrider-web \
      --target wasm32-unknown-unknown
    rm -rf web/pkg
    "$wasm_bindgen" \
      --target web \
      --out-dir web/pkg \
      target/wasm32-unknown-unknown/debug/lightrider-web.wasm
    rm -rf web/assets
    mkdir -p web/assets
    cp -R assets/. web/assets/
    echo "Built web/pkg/lightrider-web.js"

web-serve bind="127.0.0.1" port="8000": web-build
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Serving http://localhost:{{port}}/"
    echo "For the local Bevygap stack, open:"
    echo "http://localhost:{{port}}/?matchmaker_url=ws://127.0.0.1:3000/matchmaker/ws&matchmaker_game=lightrider&matchmaker_version=dev"
    python3 -m http.server "{{port}}" --bind "{{bind}}" --directory web

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
    if [[ -f /etc/ssl/certs/ca-certificates.crt ]]; then
      cp /etc/ssl/certs/ca-certificates.crt .edgegap-build/context/host-ca-certificates.crt
    else
      : > .edgegap-build/context/host-ca-certificates.crt
    fi

edgegap-login:
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    printf '%s' "$EDGEGAP_REGISTRY_TOKEN" | podman login "$EDGEGAP_REGISTRY_URL" \
      --username "$EDGEGAP_REGISTRY_USERNAME" \
      --password-stdin

edgegap-build *args: edgegap-context
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    tag="{{edgegap-default-tag}}"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
        cpus=*) build_cpus="${arg#cpus=}" ;;
        build_cpus=*) build_cpus="${arg#build_cpus=}" ;;
        cpu_quota=*) build_cpu_quota="${arg#cpu_quota=}" ;;
        build_cpu_quota=*) build_cpu_quota="${arg#build_cpu_quota=}" ;;
        cpuset_cpus=*) build_cpuset_cpus="${arg#cpuset_cpus=}" ;;
        build_cpuset_cpus=*) build_cpuset_cpus="${arg#build_cpuset_cpus=}" ;;
        *)
          case "$positional" in
            0) tag="$arg" ;;
            1) build_memory="$arg" ;;
            2) build_cpus="$arg" ;;
            3) build_cpu_quota="$arg" ;;
            4) build_cpuset_cpus="$arg" ;;
            *)
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-server:$tag"
    build_cmd=(podman build --layers)
    if [[ "${NO_CACHE:-0}" == "1" ]]; then
      build_cmd+=(--no-cache)
    fi
    build_memory="${PODMAN_BUILD_MEMORY:-$build_memory}"
    build_cpus="${PODMAN_BUILD_CPUS:-$build_cpus}"
    build_cpu_quota="${PODMAN_BUILD_CPU_QUOTA:-$build_cpu_quota}"
    build_cpuset_cpus="${PODMAN_BUILD_CPUSET_CPUS:-$build_cpuset_cpus}"
    if [[ -n "${PODMAN_CACHE_FROM:-}" ]]; then
      build_cmd+=(--cache-from "$PODMAN_CACHE_FROM")
    fi
    if [[ -n "${PODMAN_CACHE_TO:-}" ]]; then
      build_cmd+=(--cache-to "$PODMAN_CACHE_TO")
    fi
    if [[ -n "$build_memory" ]]; then
      build_cmd+=(--memory "$build_memory")
    fi
    if [[ -n "$build_cpus" ]]; then
      if [[ ! "$build_cpus" =~ ^[0-9]+$ ]]; then
        echo "edgegap-build cpus must be an integer because podman build has no --cpus flag; got '$build_cpus'" >&2
        exit 2
      fi
      build_cmd+=(--cpu-period 100000 --cpu-quota "$((build_cpus * 100000))")
    fi
    if [[ -n "$build_cpu_quota" ]]; then
      build_cmd+=(--cpu-quota "$build_cpu_quota")
    fi
    if [[ -n "$build_cpuset_cpus" ]]; then
      build_cmd+=(--cpuset-cpus "$build_cpuset_cpus")
    fi
    "${build_cmd[@]}" \
      --build-arg "SERVER_CARGO_JOBS=${SERVER_CARGO_JOBS:-2}" \
      --build-arg "SERVER_CARGO_INCREMENTAL=${SERVER_CARGO_INCREMENTAL:-0}" \
      --build-arg "SERVER_RELEASE_OPT_LEVEL=${SERVER_RELEASE_OPT_LEVEL:-3}" \
      --build-arg "SERVER_RELEASE_LTO=${SERVER_RELEASE_LTO:-false}" \
      --build-arg "SERVER_RELEASE_CODEGEN_UNITS=${SERVER_RELEASE_CODEGEN_UNITS:-16}" \
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

edgegap-build-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{edgegap-default-tag}}"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
        cpus=*) build_cpus="${arg#cpus=}" ;;
        build_cpus=*) build_cpus="${arg#build_cpus=}" ;;
        cpu_quota=*) build_cpu_quota="${arg#cpu_quota=}" ;;
        build_cpu_quota=*) build_cpu_quota="${arg#build_cpu_quota=}" ;;
        cpuset_cpus=*) build_cpuset_cpus="${arg#cpuset_cpus=}" ;;
        build_cpuset_cpus=*) build_cpuset_cpus="${arg#build_cpuset_cpus=}" ;;
        *)
          case "$positional" in
            0) tag="$arg" ;;
            1) build_memory="$arg" ;;
            2) build_cpus="$arg" ;;
            3) build_cpu_quota="$arg" ;;
            4) build_cpuset_cpus="$arg" ;;
            *)
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just edgegap-build tag="$tag" memory="$build_memory" cpus="$build_cpus" cpu_quota="$build_cpu_quota" cpuset_cpus="$build_cpuset_cpus"
    just edgegap-push "$tag"

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

matchmaker-build *args: edgegap-context
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    tag="{{edgegap-default-tag}}"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
        cpus=*) build_cpus="${arg#cpus=}" ;;
        build_cpus=*) build_cpus="${arg#build_cpus=}" ;;
        cpu_quota=*) build_cpu_quota="${arg#cpu_quota=}" ;;
        build_cpu_quota=*) build_cpu_quota="${arg#build_cpu_quota=}" ;;
        cpuset_cpus=*) build_cpuset_cpus="${arg#cpuset_cpus=}" ;;
        build_cpuset_cpus=*) build_cpuset_cpus="${arg#build_cpuset_cpus=}" ;;
        *)
          case "$positional" in
            0) tag="$arg" ;;
            1) build_memory="$arg" ;;
            2) build_cpus="$arg" ;;
            3) build_cpu_quota="$arg" ;;
            4) build_cpuset_cpus="$arg" ;;
            *)
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-matchmaker:$tag"
    # Defaults are balanced for a 32G+ build machine. Drop the env values to
    # 1/false/16 if rustc is OOM-killed on a smaller host.
    build_cmd=(podman build --layers)
    if [[ "${NO_CACHE:-0}" == "1" ]]; then
      build_cmd+=(--no-cache)
    fi
    build_memory="${PODMAN_BUILD_MEMORY:-$build_memory}"
    build_cpus="${PODMAN_BUILD_CPUS:-$build_cpus}"
    build_cpu_quota="${PODMAN_BUILD_CPU_QUOTA:-$build_cpu_quota}"
    build_cpuset_cpus="${PODMAN_BUILD_CPUSET_CPUS:-$build_cpuset_cpus}"
    if [[ -n "${PODMAN_CACHE_FROM:-}" ]]; then
      build_cmd+=(--cache-from "$PODMAN_CACHE_FROM")
    fi
    if [[ -n "${PODMAN_CACHE_TO:-}" ]]; then
      build_cmd+=(--cache-to "$PODMAN_CACHE_TO")
    fi
    if [[ -n "$build_memory" ]]; then
      build_cmd+=(--memory "$build_memory")
    fi
    if [[ -n "$build_cpus" ]]; then
      if [[ ! "$build_cpus" =~ ^[0-9]+$ ]]; then
        echo "matchmaker-build cpus must be an integer because podman build has no --cpus flag; got '$build_cpus'" >&2
        exit 2
      fi
      build_cmd+=(--cpu-period 100000 --cpu-quota "$((build_cpus * 100000))")
    fi
    if [[ -n "$build_cpu_quota" ]]; then
      build_cmd+=(--cpu-quota "$build_cpu_quota")
    fi
    if [[ -n "$build_cpuset_cpus" ]]; then
      build_cmd+=(--cpuset-cpus "$build_cpuset_cpus")
    fi
    "${build_cmd[@]}" \
      --build-arg "MATCHMAKER_CARGO_JOBS=${MATCHMAKER_CARGO_JOBS:-2}" \
      --build-arg "MATCHMAKER_CARGO_INCREMENTAL=${MATCHMAKER_CARGO_INCREMENTAL:-0}" \
      --build-arg "MATCHMAKER_RELEASE_OPT_LEVEL=${MATCHMAKER_RELEASE_OPT_LEVEL:-2}" \
      --build-arg "MATCHMAKER_RELEASE_LTO=${MATCHMAKER_RELEASE_LTO:-thin}" \
      --build-arg "MATCHMAKER_RELEASE_CODEGEN_UNITS=${MATCHMAKER_RELEASE_CODEGEN_UNITS:-8}" \
      --build-arg "WEB_CARGO_JOBS=${WEB_CARGO_JOBS:-2}" \
      --build-arg "WEB_CARGO_INCREMENTAL=${WEB_CARGO_INCREMENTAL:-0}" \
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

matchmaker-build-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{edgegap-default-tag}}"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
        cpus=*) build_cpus="${arg#cpus=}" ;;
        build_cpus=*) build_cpus="${arg#build_cpus=}" ;;
        cpu_quota=*) build_cpu_quota="${arg#cpu_quota=}" ;;
        build_cpu_quota=*) build_cpu_quota="${arg#build_cpu_quota=}" ;;
        cpuset_cpus=*) build_cpuset_cpus="${arg#cpuset_cpus=}" ;;
        build_cpuset_cpus=*) build_cpuset_cpus="${arg#build_cpuset_cpus=}" ;;
        *)
          case "$positional" in
            0) tag="$arg" ;;
            1) build_memory="$arg" ;;
            2) build_cpus="$arg" ;;
            3) build_cpu_quota="$arg" ;;
            4) build_cpuset_cpus="$arg" ;;
            *)
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just matchmaker-build tag="$tag" memory="$build_memory" cpus="$build_cpus" cpu_quota="$build_cpu_quota" cpuset_cpus="$build_cpuset_cpus"
    just matchmaker-push "$tag"

prod-images-build *args:
    just edgegap-build {{args}}
    just matchmaker-build {{args}}

prod-images-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{edgegap-default-tag}}"
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*|build_memory=*|cpus=*|build_cpus=*|cpu_quota=*|build_cpu_quota=*|cpuset_cpus=*|build_cpuset_cpus=*)
          ;;
        *)
          case "$positional" in
            0) tag="$arg" ;;
            1|2|3|4) ;;
            *)
              echo "unexpected extra argument for prod-images-push: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just edgegap-push "$tag"
    just matchmaker-push "$tag"

prod-images-build-push *args:
    just prod-images-build {{args}}
    just prod-images-push {{args}}

github-release tag=edgegap-default-tag title="" notes="":
    #!/usr/bin/env bash
    set -euo pipefail
    command -v gh >/dev/null 2>&1 || {
      echo "gh CLI is required to create a GitHub release from the command line" >&2
      exit 1
    }
    release_title="{{title}}"
    release_notes="{{notes}}"
    if [[ -z "$release_title" ]]; then
      release_title="{{tag}}"
    fi
    if [[ -z "$release_notes" ]]; then
      release_notes="Release {{tag}}"
    fi
    gh release create "{{tag}}" --title "$release_title" --notes "$release_notes"

edgegap-release-sync tag=edgegap-default-tag nats_host="45.79.138.102:4222" app="lightrider" version="":
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    source secrets/prod-netcode.env
    if [[ -f secrets/web-server.env ]]; then
      source secrets/web-server.env
    fi
    edgegap_version="{{version}}"
    if [[ -z "$edgegap_version" ]]; then
      edgegap_version="{{tag}}"
    fi
    export NATS_HOST="{{nats_host}}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export EDGEGAP_NATS_INSECURE="${EDGEGAP_NATS_INSECURE:-1}"
    export BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE_OVERRIDE:-{{app}}_${edgegap_version}}"
    just edgegap-app-sync "{{tag}}" "$edgegap_version" "{{app}}"
    just edgegap-app-verify "{{tag}}" "$edgegap_version" "{{app}}"

deploy-web-server-pull *args:
    #!/usr/bin/env bash
    set -euo pipefail
    host=""
    tag=""
    edgegap_version=""
    web_domain=""
    enable_https=""
    ssh_port="22"
    ssh_key=""
    env_file="secrets/web-server.env"
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        host=*) host="${arg#host=}" ;;
        tag=*) tag="${arg#tag=}" ;;
        edgegap_version=*) edgegap_version="${arg#edgegap_version=}" ;;
        version=*) edgegap_version="${arg#version=}" ;;
        domain=*) web_domain="${arg#domain=}" ;;
        web_domain=*) web_domain="${arg#web_domain=}" ;;
        https=*) enable_https="${arg#https=}" ;;
        enable_https=*) enable_https="${arg#enable_https=}" ;;
        ssh_port=*) ssh_port="${arg#ssh_port=}" ;;
        ssh_key=*) ssh_key="${arg#ssh_key=}" ;;
        env=*) env_file="${arg#env=}" ;;
        *)
          case "$positional" in
            0) host="$arg" ;;
            1) tag="$arg" ;;
            2) ssh_port="$arg" ;;
            3) env_file="$arg" ;;
            4) ssh_key="$arg" ;;
            5) edgegap_version="$arg" ;;
            6) web_domain="$arg" ;;
            *)
              echo "unexpected extra argument for deploy-web-server-pull: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    if [[ -z "$host" || -z "$tag" ]]; then
      echo "usage: just deploy-web-server-pull <vps-ip-or-host> <tag> [ssh_port] [env] [ssh_key] [edgegap_version] [domain]" >&2
      echo "   or: just deploy-web-server-pull host=<vps-ip-or-host> tag=<tag> [edgegap_version=<version>] [domain=play.example.com] [https=1] [ssh_port=22] [env=secrets/web-server.env] [ssh_key=~/.ssh/key]" >&2
      exit 2
    fi
    SKIP_IMAGE_BUILD=1 just deploy-web-server host="$host" ssh_port="$ssh_port" ssh_key="$ssh_key" tag="$tag" edgegap_version="$edgegap_version" domain="$web_domain" https="$enable_https" env="$env_file"

web-server-env-template tag=edgegap-default-tag file="secrets/web-server.env" host="45.79.138.102" edgegap_version="":
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
      LIGHTRIDER_PROTOCOL_ID="1"
    fi
    if [[ -z "${LIGHTRIDER_PRIVATE_KEY:-}" ]]; then
      LIGHTRIDER_PRIVATE_KEY="$(openssl rand -hex 32)"
    fi
    if [[ -z "${NATS_PASSWORD:-}" || "${NATS_PASSWORD:-}" == "lightrider" ]]; then
      NATS_PASSWORD="$(openssl rand -hex 24)"
    fi
    edgegap_app_version="{{edgegap_version}}"
    if [[ -z "$edgegap_app_version" ]]; then
      edgegap_app_version="${EDGEGAP_APP_VERSION:-{{tag}}}"
    fi
    if [[ -n "{{edgegap_version}}" ]]; then
      BEVYGAP_NATS_NAMESPACE="${BEVYGAP_NATS_NAMESPACE_OVERRIDE:-${EDGEGAP_APP_NAME:-lightrider}_${edgegap_app_version}}"
    fi
    web_domain="${LIGHTRIDER_WEB_DOMAIN:-}"
    web_domain="${web_domain#http://}"
    web_domain="${web_domain#https://}"
    web_domain="${web_domain%%/*}"
    enable_https="${LIGHTRIDER_ENABLE_HTTPS:-}"
    if [[ -z "$enable_https" && -n "$web_domain" ]]; then
      enable_https=1
    fi
    enable_https="${enable_https:-0}"
    if [[ "$enable_https" == "true" || "$enable_https" == "yes" || "$enable_https" == "on" ]]; then
      enable_https=1
    fi
    if [[ "$enable_https" == "1" && -z "$web_domain" ]]; then
      echo "LIGHTRIDER_WEB_DOMAIN is required when LIGHTRIDER_ENABLE_HTTPS=1" >&2
      exit 1
    fi
    if [[ "$enable_https" == "1" ]]; then
      default_matchmaker_cors="https://${web_domain}"
      default_matchmaker_url="wss://${web_domain}/matchmaker/ws"
    else
      default_matchmaker_cors="http://{{host}}"
      default_matchmaker_url=""
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
      write_env EDGEGAP_APP_VERSION "$edgegap_app_version"
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
      write_env LIGHTRIDER_ENABLE_HTTPS "$enable_https"
      write_env LIGHTRIDER_WEB_DOMAIN "$web_domain"
      write_env LIGHTRIDER_CADDY_EMAIL "${LIGHTRIDER_CADDY_EMAIL:-}"
      write_env MATCHMAKER_CORS "${MATCHMAKER_CORS:-$default_matchmaker_cors}"
      write_env LIGHTRIDER_MATCHMAKER_URL "${LIGHTRIDER_MATCHMAKER_URL:-$default_matchmaker_url}"
      write_env BEVYGAP_NATS_NAMESPACE "${BEVYGAP_NATS_NAMESPACE:-${EDGEGAP_APP_NAME:-lightrider}_${edgegap_app_version}}"
      write_env BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT "${BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT:-800}"
      write_env BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT "${BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT:-16}"
      write_env BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT "${BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT:-85}"
      write_env BEVYGAP_STATIC_CLIENT_COUNTRY_CODES "${BEVYGAP_STATIC_CLIENT_COUNTRY_CODES:-US}"
      if [[ -n "${BEVYGAP_GEOIP_DB:-}" ]]; then
        write_env BEVYGAP_GEOIP_DB "$BEVYGAP_GEOIP_DB"
      fi
      write_env LIGHTRIDER_RUN_STATIC_SERVER "${LIGHTRIDER_RUN_STATIC_SERVER:-1}"
      write_env LIGHTRIDER_STATIC_SERVER_IMAGE "${LIGHTRIDER_STATIC_SERVER_IMAGE:-${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}/${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}/lightrider-server:{{tag}}}"
      write_env LIGHTRIDER_STATIC_PUBLIC_IP "${LIGHTRIDER_STATIC_PUBLIC_IP:-{{host}}}"
      write_env LIGHTRIDER_STATIC_PORT "${LIGHTRIDER_STATIC_PORT:-7777}"
      write_env LIGHTRIDER_STATIC_REQUEST_ID "${LIGHTRIDER_STATIC_REQUEST_ID:-linode-us-east-1}"
      write_env LIGHTRIDER_STATIC_COUNTRY_CODE "${LIGHTRIDER_STATIC_COUNTRY_CODE:-US}"
      write_env LIGHTRIDER_STATIC_REGION "${LIGHTRIDER_STATIC_REGION:-us-east}"
      if [[ -n "${NATS_TLS_CERT:-}" ]]; then write_env NATS_TLS_CERT "$NATS_TLS_CERT"; fi
      if [[ -n "${NATS_TLS_KEY:-}" ]]; then write_env NATS_TLS_KEY "$NATS_TLS_KEY"; fi
      if [[ -n "${NATS_CA:-}" ]]; then write_env NATS_CA "$NATS_CA"; fi
      if [[ -n "${MATCHMAKER_NATS_HOST:-}" ]]; then write_env MATCHMAKER_NATS_HOST "$MATCHMAKER_NATS_HOST"; fi
      if [[ -n "${MATCHMAKER_NATS_INSECURE:-}" ]]; then write_env MATCHMAKER_NATS_INSECURE "$MATCHMAKER_NATS_INSECURE"; fi
    } > "{{file}}"
    chmod 600 "{{file}}"
    echo "Wrote {{file}}"

web-server-install host="45.79.138.102" ssh_port="22" env="secrets/web-server.env" ssh_key="":
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{env}}" || {
      echo "{{env}} does not exist. Run: just web-server-env-template" >&2
      exit 1
    }
    ssh_key="{{ssh_key}}"
    if [[ "$ssh_key" == "~/"* ]]; then
      ssh_key="${HOME}/${ssh_key#~/}"
    fi
    ssh_opts=(-p "{{ssh_port}}")
    scp_opts=(-P "{{ssh_port}}")
    if [[ -n "$ssh_key" ]]; then
      test -f "$ssh_key" || {
        echo "SSH key not found: $ssh_key" >&2
        exit 1
      }
      ssh_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
      scp_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
    fi
    remote_dir="/tmp/lightrider-web-server-setup"
    ssh "${ssh_opts[@]}" "root@{{host}}" "mkdir -p '$remote_dir'"
    scp "${scp_opts[@]}" tools/setup_web_server_host.sh "root@{{host}}:$remote_dir/setup_web_server_host.sh"
    scp "${scp_opts[@]}" "{{env}}" "root@{{host}}:$remote_dir/web-server.env"
    ssh "${ssh_opts[@]}" "root@{{host}}" "bash '$remote_dir/setup_web_server_host.sh' --env-file '$remote_dir/web-server.env'; rm -f '$remote_dir/web-server.env'"

web-server-health host="45.79.138.102" scheme="http":
    #!/usr/bin/env bash
    set -euo pipefail
    url="{{scheme}}://{{host}}/"
    curl -fsSL --max-time 30 "$url" >/dev/null
    echo "web ok: $url"

web-server-enable-nats-tls-from-caddy host="45.79.138.102" domain="45.79.138.102.sslip.io" ssh_port="22" ssh_key="":
    #!/usr/bin/env bash
    set -euo pipefail
    ssh_key="{{ssh_key}}"
    if [[ "$ssh_key" == "~/"* ]]; then
      ssh_key="${HOME}/${ssh_key#~/}"
    fi
    ssh_opts=(-p "{{ssh_port}}")
    if [[ -n "$ssh_key" ]]; then
      test -f "$ssh_key" || {
        echo "SSH key not found: $ssh_key" >&2
        exit 1
      }
      ssh_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
    fi
    ssh "${ssh_opts[@]}" "root@{{host}}" 'bash -s' <<'REMOTE'
    set -euo pipefail
    domain="{{domain}}"
    cert_root="/var/lib/caddy/.local/share/caddy/certificates"
    env_file="/etc/lightrider/lightrider-matchmaker.env"
    static_env_file="/etc/lightrider/lightrider-static-server.env"
    cert_dest="/etc/lightrider/nats-cert.pem"
    key_dest="/etc/lightrider/nats-key.pem"

    curl -fsSL --max-time 30 "https://${domain}/" >/dev/null

    cert="$(find "$cert_root" -type f -name "${domain}.crt" | head -n 1 || true)"
    key="$(find "$cert_root" -type f -name "${domain}.key" | head -n 1 || true)"
    if [[ -z "$cert" || -z "$key" ]]; then
      echo "Could not find Caddy certificate/key for ${domain} under ${cert_root}" >&2
      echo "Check: journalctl -u caddy -n 120 --no-pager" >&2
      exit 1
    fi

    install -m 644 "$cert" "$cert_dest"
    install -m 600 "$key" "$key_dest"

    tmp="$(mktemp)"
    grep -v -E '^(NATS_ALLOW_INSECURE|NATS_TLS_CERT|NATS_TLS_KEY|MATCHMAKER_NATS_HOST|BEVYGAP_REQUIRE_SECURE_NATS)=' "$env_file" > "$tmp" || true
    cat "$tmp" > "$env_file"
    rm -f "$tmp"
    {
      printf 'NATS_ALLOW_INSECURE=0\n'
      printf 'NATS_TLS_CERT=%s\n' "$cert_dest"
      printf 'NATS_TLS_KEY=%s\n' "$key_dest"
      printf 'MATCHMAKER_NATS_HOST=%s:4222\n' "$domain"
      printf 'BEVYGAP_REQUIRE_SECURE_NATS=1\n'
    } >> "$env_file"
    chmod 600 "$env_file"

    if [[ -f "$static_env_file" ]]; then
      tmp="$(mktemp)"
      grep -v -E '^(NATS_HOST|NATS_INSECURE|BEVYGAP_REQUIRE_SECURE_NATS)=' "$static_env_file" > "$tmp" || true
      cat "$tmp" > "$static_env_file"
      rm -f "$tmp"
      {
        printf 'NATS_HOST=%s:4222\n' "$domain"
        printf 'BEVYGAP_REQUIRE_SECURE_NATS=1\n'
      } >> "$static_env_file"
      chmod 600 "$static_env_file"
    fi

    systemctl restart lightrider-matchmaker
    if systemctl list-unit-files lightrider-static-server.service >/dev/null 2>&1; then
      systemctl restart lightrider-static-server || true
    fi
    sleep 2
    systemctl --no-pager --full status lightrider-matchmaker || true
    if systemctl list-unit-files lightrider-static-server.service >/dev/null 2>&1; then
      systemctl --no-pager --full status lightrider-static-server || true
    fi
    if command -v openssl >/dev/null 2>&1; then
      timeout 10 openssl s_client -connect "${domain}:4222" -servername "$domain" -verify_return_error </dev/null >/tmp/lightrider-nats-tls-check.txt 2>&1 || {
        cat /tmp/lightrider-nats-tls-check.txt >&2
        exit 1
      }
      echo "nats tls ok: ${domain}:4222"
    else
      echo "openssl not found; skipped external NATS TLS check"
    fi
    REMOTE

deploy-web-server *args:
    #!/usr/bin/env bash
    set -euo pipefail
    vps_host=""
    ssh_port="22"
    ssh_key=""
    tag="$(git rev-parse --short HEAD 2>/dev/null || date +%Y%m%d%H%M%S)"
    edgegap_version=""
    web_domain=""
    enable_https=""
    env_file="secrets/web-server.env"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        host=*) vps_host="${arg#host=}" ;;
        ssh_port=*) ssh_port="${arg#ssh_port=}" ;;
        port=*) ssh_port="${arg#port=}" ;;
        ssh_key=*) ssh_key="${arg#ssh_key=}" ;;
        tag=*) tag="${arg#tag=}" ;;
        edgegap_version=*) edgegap_version="${arg#edgegap_version=}" ;;
        version=*) edgegap_version="${arg#version=}" ;;
        domain=*) web_domain="${arg#domain=}" ;;
        web_domain=*) web_domain="${arg#web_domain=}" ;;
        https=*) enable_https="${arg#https=}" ;;
        enable_https=*) enable_https="${arg#enable_https=}" ;;
        env=*) env_file="${arg#env=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
        cpus=*) build_cpus="${arg#cpus=}" ;;
        build_cpus=*) build_cpus="${arg#build_cpus=}" ;;
        cpu_quota=*) build_cpu_quota="${arg#cpu_quota=}" ;;
        build_cpu_quota=*) build_cpu_quota="${arg#build_cpu_quota=}" ;;
        cpuset_cpus=*) build_cpuset_cpus="${arg#cpuset_cpus=}" ;;
        build_cpuset_cpus=*) build_cpuset_cpus="${arg#build_cpuset_cpus=}" ;;
        *)
          case "$positional" in
            0) vps_host="$arg" ;;
            1) ssh_port="$arg" ;;
            2) tag="$arg" ;;
            3) env_file="$arg" ;;
            4) ssh_key="$arg" ;;
            5) edgegap_version="$arg" ;;
            6) web_domain="$arg" ;;
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
      echo "usage: just deploy-web-server host=<vps-ip-or-host> [ssh_port=<port>] [ssh_key=<key>] [tag=<tag>] [edgegap_version=<version>] [domain=<domain>] [https=1] [env=<file>]" >&2
      echo "example: just deploy-web-server host=45.79.138.102 domain=play.example.com" >&2
      exit 2
    fi
    if [[ -z "$enable_https" && -n "$web_domain" ]]; then
      enable_https=1
    fi
    enable_https="${enable_https:-0}"
    if [[ "${SKIP_IMAGE_BUILD:-0}" == "1" ]]; then
      echo "Skipping matchmaker image build/push; assuming tag $tag is already pushed."
    else
      just matchmaker-build-push "$tag" "$build_memory" "$build_cpus" "$build_cpu_quota" "$build_cpuset_cpus"
    fi
    LIGHTRIDER_ENABLE_HTTPS="$enable_https" LIGHTRIDER_WEB_DOMAIN="$web_domain" FORCE=1 just web-server-env-template "$tag" "$env_file" "$vps_host" "$edgegap_version"
    just web-server-install "$vps_host" "$ssh_port" "$env_file" "$ssh_key"
    if [[ "$enable_https" == "1" || "$enable_https" == "true" || "$enable_https" == "yes" ]]; then
      just web-server-health "$web_domain" https
    else
      just web-server-health "$vps_host" http
    fi

netcode-secret protocol_id="":
    #!/usr/bin/env bash
    set -euo pipefail
    protocol_id="{{protocol_id}}"
    if [[ -z "$protocol_id" ]]; then
      protocol_id="$(od -An -N8 -tu8 /dev/urandom | tr -d ' ')"
    fi
    private_key="$(openssl rand -hex 32)"
    printf 'LIGHTRIDER_PROTOCOL_ID=%s\n' "$protocol_id"
    printf 'LIGHTRIDER_PRIVATE_KEY=%s\n' "$private_key"
    printf 'LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1\n'
