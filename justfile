set shell := ["bash", "-cu"]

edgegap-default-tag := `git rev-parse --short HEAD 2>/dev/null || date +%Y%m%d%H%M%S`

export CARGO_BUILD_JOBS := "2"
export CARGO_INCREMENTAL := "1"

server config="config/test.ron" port="5000" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p server --bin lightrider-server -- --headless --port {{port}} --config {{config}}

client id="1" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p client --bin lightrider-client -- --client-id {{id}} --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}}

client-debug id="1" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p client --bin lightrider-client -- --debug --client-id {{id}} --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}}

bot id="1001" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p client --bin lightrider-client -- --headless --mode bot --client-id {{id}} --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}}

bots count="4" first_id="1001" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'jobs -pr | xargs -r kill' EXIT
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    for i in $(seq 0 $(({{count}} - 1))); do
      cargo run "${cargo_args[@]}" -p client --bin lightrider-client -- --headless --mode bot --client-id $(({{first_id}} + i)) --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}} &
      sleep 1
    done
    wait

local bots="4" config="config/test.ron" port="5000" client_id="1" first_bot_id="1001" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    bots_arg="{{bots}}"
    config_arg="{{config}}"
    port_arg="{{port}}"
    client_id_arg="{{client_id}}"
    first_bot_id_arg="{{first_bot_id}}"
    room_arg="{{room}}"
    release_arg="{{release}}"
    if [[ "$bots_arg" == release=* ]]; then
      release_arg="${bots_arg#release=}"
      bots_arg="4"
    fi
    if [[ "$release_arg" == release=* ]]; then
      release_arg="${release_arg#release=}"
    fi
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    # Local rendered smoke should exercise the same moderate receive-side
    # conditioner on both peers that Lightyear examples call "average".
    : "${LIGHTRIDER_NETWORK_CONDITIONER:=average}"
    export LIGHTRIDER_NETWORK_CONDITIONER
    trap 'jobs -pr | xargs -r kill' EXIT
    cargo_args=(-j 2)
    if [[ "$release_arg" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p server --bin lightrider-server -- --headless --port "$port_arg" --config "$config_arg" &
    sleep 2
    for i in $(seq 0 $((bots_arg - 1))); do
      cargo run "${cargo_args[@]}" -p client --bin lightrider-client -- --headless --mode bot --client-id $((first_bot_id_arg + i)) --server-port "$port_arg" --config "$config_arg" --room "$room_arg" &
      sleep 1
    done
    cargo run "${cargo_args[@]}" -p client --bin lightrider-client -- --client-id "$client_id_arg" --server-port "$port_arg" --config "$config_arg" --room "$room_arg"

trace-local clients="4" seconds="20" config="config/test.ron" port="5000" first_client_id="2001" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    run_dir="logs/debug/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$run_dir"
    ln -sfn "$(basename "$run_dir")" logs/debug/latest
    cargo_args=(-j 2)
    bin_dir="debug"
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    cargo build "${cargo_args[@]}" -p server --bin lightrider-server -p client --bin lightrider-client
    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
    }
    trap cleanup EXIT
    RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/server.ndjson" \
      "target/$bin_dir/lightrider-server" --headless --port {{port}} --config {{config}} \
      > "$run_dir/server.log" 2>&1 &
    pids+=("$!")
    sleep 2
    for i in $(seq 0 $(({{clients}} - 1))); do
      id=$(({{first_client_id}} + i))
      RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-$id.ndjson" \
        "target/$bin_dir/lightrider-client" --headless --mode bot --client-id "$id" --server-port {{port}} --config {{config}} --room {{room}} \
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

trace-local-mixed seconds="20" config="config/test.ron" port="5000" player_id="3001" first_bot_id="3002" bot_clients="2" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    run_dir="logs/debug/$(date +%Y%m%d-%H%M%S)-mixed-headless"
    mkdir -p "$run_dir"
    ln -sfn "$(basename "$run_dir")" logs/debug/latest-mixed
    cargo_args=(-j 2)
    bin_dir="debug"
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    cargo build "${cargo_args[@]}" -p server --bin lightrider-server -p client --bin lightrider-client
    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
    }
    trap cleanup EXIT
    RUST_LOG="info,lightyear_debug=trace,server::food=warn" LIGHTYEAR_DEBUG_FILE="$run_dir/server.ndjson" \
      "target/$bin_dir/lightrider-server" --headless --port {{port}} --config {{config}} \
      > "$run_dir/server.log" 2>&1 &
    pids+=("$!")
    sleep 2
    RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-player-{{player_id}}.ndjson" \
      "target/$bin_dir/lightrider-client" --headless --mode player --client-id {{player_id}} --server-port {{port}} --config {{config}} --room {{room}} \
      > "$run_dir/client-player-{{player_id}}.log" 2>&1 &
    pids+=("$!")
    sleep 1
    if (( {{bot_clients}} > 0 )); then
      for i in $(seq 0 $(({{bot_clients}} - 1))); do
        id=$(({{first_bot_id}} + i))
        RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-bot-$id.ndjson" \
          "target/$bin_dir/lightrider-client" --headless --mode bot --client-id "$id" --server-port {{port}} --config {{config}} --room {{room}} \
          > "$run_dir/client-bot-$id.log" 2>&1 &
        pids+=("$!")
        sleep 1
      done
    fi
    sleep {{seconds}}
    cleanup
    trap - EXIT
    echo "trace run: $run_dir"
    if command -v duckdb >/dev/null 2>&1; then
      duckdb -batch -cmd "SET VARIABLE trace_glob = '$run_dir/*.ndjson';" < tools/debug_trace_summary.sql | tee "$run_dir/summary.txt"
    else
      echo "duckdb not found; inspect $run_dir/*.ndjson manually"
    fi

# Lightyear Matchmaker local workflow:
# 1. `just matchmaker-nats` starts local NATS with JetStream. The game server
#    publishes readiness/capacity there, and the matchmaker writes assignments.
# 2. `just lightyear-matchmaker-server-local` starts a static Lightrider game
#    server that registers itself with the matchmaker through NATS.
# 3. `just lightyear-matchmaker-service-local` starts the deployable
#    lightyear_matchmaker_server with the NATS-backed static provider.
# 4. `just lightyear-matchmaker-client-bot` requests a token over WebSocket and
#    connects to the assigned game server.
# 5. `just lightyear-matchmaker-local-smoke` runs the full local stack in one
#    command and checks logs for assignment and client-connect events.
matchmaker-help:
    #!/usr/bin/env bash
    set -euo pipefail
    cat <<'EOF'
    Lightyear Matchmaker local order:
      1. just matchmaker-nats-pull
      2. just matchmaker-nats
      3. in another terminal: just lightyear-matchmaker-server-local
      4. in another terminal: just lightyear-matchmaker-service-local
      5. just lightyear-matchmaker-client-bot

    One-command local static smoke:
      just lightyear-matchmaker-local-smoke

    Real Edgegap token flow:
      EDGEGAP_API_KEY=... \
      LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE=edgegap \
      EDGEGAP_APP_NAME=lightrider \
      EDGEGAP_APP_VERSION=<edgegap-version> \
        just lightyear-matchmaker-service-local

    The Edgegap flow creates Edgegap sessions, so EDGEGAP_API_KEY must be
    exported or present in secrets/edgegap.env.

    Local NATS uses:
      NATS_HOST=127.0.0.1:4222
      NATS_USER=lightrider
      NATS_PASSWORD=lightrider
      LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE=lightrider_dev

    LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE scopes buckets, streams, and subjects
    so multiple app versions can share one NATS instance without mixing state.
    EOF

matchmaker-nats-pull:
    podman pull nats:latest

# Run a disposable local NATS + JetStream instance for matchmaker smoke tests.
matchmaker-nats:
    #!/usr/bin/env bash
    set -euo pipefail
    podman rm -f lightrider-nats >/dev/null 2>&1 || true
    podman run --rm --name lightrider-nats \
      -p 4222:4222 \
      -p 8222:8222 \
      nats:latest \
      -js \
      -m 8222 \
      --user "${NATS_USER:-lightrider}" \
      --pass "${NATS_PASSWORD:-lightrider}"

matchmaker-nats-health:
    curl -fsS http://127.0.0.1:8222/healthz

# Start a local game server that publishes readiness/capacity to NATS. This is
# the static-provider path used by the VPS-hosted static game server too.
lightyear-matchmaker-server-local config="config/test.ron" port="7777" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    export NATS_HOST="${NATS_HOST:-127.0.0.1:4222}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-${MATCHMAKER_NATS_NAMESPACE:-lightrider_dev}}"
    export LIGHTRIDER_SERVER_ID="${LIGHTRIDER_SERVER_ID:-local-lightrider}"
    export LIGHTRIDER_MATCHMAKER_PROVIDER="${LIGHTRIDER_MATCHMAKER_PROVIDER:-static}"
    export LIGHTRIDER_PUBLIC_IP="${LIGHTRIDER_PUBLIC_IP:-127.0.0.1}"
    export LIGHTRIDER_PUBLIC_PORT="${LIGHTRIDER_PUBLIC_PORT:-{{port}}}"
    export SELF_SIGNED_SANS="${SELF_SIGNED_SANS:-127.0.0.1,localhost}"
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p server --features lightyear-matchmaker --bin lightrider-server -- --headless --matchmaker --port {{port}} --config {{config}}

# Start the standalone matchmaker. By default it uses live static capacity from
# NATS; set LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE=edgegap to use Edgegap.
lightyear-matchmaker-service-local bind="127.0.0.1:3000" config_path="" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    for env_file in secrets/edgegap.env secrets/prod-netcode.env secrets/nats.env; do
      if [[ -f "$env_file" ]]; then
        source "$env_file"
      fi
    done
    config="{{config_path}}"
    if [[ -z "$config" ]]; then
      config="$(mktemp -t lightrider-matchmaker.XXXXXX.toml)"
      cleanup_config=1
    else
      cleanup_config=0
    fi
    if [[ "$cleanup_config" == "1" ]]; then
      trap 'rm -f "$config"' EXIT
    fi
    nats_host="${NATS_HOST:-127.0.0.1:4222}"
    nats_user="${NATS_USER:-lightrider}"
    nats_password="${NATS_PASSWORD:-lightrider}"
    namespace="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-${MATCHMAKER_NATS_NAMESPACE:-lightrider_dev}}"
    protocol_id="${LIGHTRIDER_PROTOCOL_ID:-0}"
    private_key="${LIGHTRIDER_PRIVATE_KEY:-}"
    app_name="${EDGEGAP_APP_NAME:-lightrider}"
    app_version="${EDGEGAP_APP_VERSION:-dev}"
    allocation_source="${LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE:-nats_static}"
    cat > "$config" <<EOF
    [server]
    bind = "{{bind}}"

    [game]
    name = "$app_name"
    version = "$app_version"

    [lightyear]
    protocol_id = $protocol_id
    private_key = "$private_key"
    client_timeout_secs = 15
    token_expire_secs = 30

    [nats]
    url = "nats://$nats_host"
    username = "$nats_user"
    password = "$nats_password"
    namespace = "$namespace"

    [allocation]
    source = "$allocation_source"
    require_assignment_prepare = true
    assignment_prepare_timeout_ms = 5000
    assignment_prepare_poll_ms = 25

    [edgegap_provider]
    app = "$app_name"
    version = "$app_version"
    api_key_env = "EDGEGAP_API_KEY"
    base_url = "${EDGEGAP_API_BASE_URL:-https://api.edgegap.com}"
    port_name = "${EDGEGAP_GAME_PORT_NAME:-game}"
    session_ready_timeout_secs = ${LIGHTYEAR_MATCHMAKER_EDGEGAP_READY_TIMEOUT_SECS:-120}
    session_poll_ms = ${LIGHTYEAR_MATCHMAKER_EDGEGAP_POLL_MS:-500}
    release_missing_ok = true
    EOF
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" --manifest-path ../lightyear-matchmaker/Cargo.toml -p lightyear_matchmaker_server -- --config "$config"

# Run one headless client through the matchmaker WebSocket API.
lightyear-matchmaker-client-bot id="3001" config="config/test.ron" matchmaker_url="ws://127.0.0.1:3000/ws" game="lightrider" version="dev" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p client --features lightyear-matchmaker --bin lightrider-client -- --headless --mode bot --client-id {{id}} --config {{config}} --room {{room}} --matchmaker-url {{matchmaker_url}} --matchmaker-game {{game}} --matchmaker-version {{version}}

# Build server/client/matchmaker binaries, run NATS, run a local static game
# server, run the matchmaker, then verify a bot can obtain a Lightyear token and
# connect. Logs are written under logs/lightyear-matchmaker/.
lightyear-matchmaker-local-smoke seconds="8" config="config/test.ron" port="7777" matchmaker_port="3000" client_id="3001" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    run_dir="logs/lightyear-matchmaker/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$run_dir"
    ln -sfn "$(basename "$run_dir")" logs/lightyear-matchmaker/latest

    target_dir="${CARGO_TARGET_DIR:-target}"
    matchmaker_target_dir="${LIGHTYEAR_MATCHMAKER_TARGET_DIR:-../lightyear-matchmaker/target}"
    bin_dir="debug"
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    nats_host="${LIGHTYEAR_MATCHMAKER_SMOKE_NATS_HOST:-127.0.0.1:4222}"
    nats_user="${LIGHTYEAR_MATCHMAKER_SMOKE_NATS_USER:-lightrider}"
    nats_password="${LIGHTYEAR_MATCHMAKER_SMOKE_NATS_PASSWORD:-lightrider}"
    namespace="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-${MATCHMAKER_NATS_NAMESPACE:-lightrider_dev}}"
    cargo build "${cargo_args[@]}" -p server --features lightyear-matchmaker --bin lightrider-server
    cargo build "${cargo_args[@]}" -p client --features lightyear-matchmaker --bin lightrider-client
    CARGO_TARGET_DIR="$matchmaker_target_dir" cargo build "${cargo_args[@]}" --manifest-path ../lightyear-matchmaker/Cargo.toml -p lightyear_matchmaker_server --bin lightyear_matchmaker_server

    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
      podman rm -f lightrider-nats >/dev/null 2>&1 || true
    }
    trap cleanup EXIT

    NATS_USER="$nats_user" NATS_PASSWORD="$nats_password" just matchmaker-nats > "$run_dir/nats.log" 2>&1 &
    pids+=("$!")
    for _ in $(seq 1 40); do
      curl -fsS http://127.0.0.1:8222/healthz >/dev/null 2>&1 && break
      sleep 0.25
    done
    curl -fsS http://127.0.0.1:8222/healthz > "$run_dir/nats-health.json"

    env \
      NATS_HOST="$nats_host" \
      NATS_USER="$nats_user" \
      NATS_PASSWORD="$nats_password" \
      LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="$namespace" \
      LIGHTRIDER_SERVER_ID="${LIGHTRIDER_SERVER_ID:-local-lightrider}" \
      LIGHTRIDER_MATCHMAKER_PROVIDER="${LIGHTRIDER_MATCHMAKER_PROVIDER:-static}" \
      LIGHTRIDER_PUBLIC_IP="${LIGHTRIDER_PUBLIC_IP:-127.0.0.1}" \
      LIGHTRIDER_PUBLIC_PORT="${LIGHTRIDER_PUBLIC_PORT:-{{port}}}" \
      SELF_SIGNED_SANS="${SELF_SIGNED_SANS:-127.0.0.1,localhost}" \
      "$target_dir/$bin_dir/lightrider-server" --headless --matchmaker --port {{port}} --config {{config}} \
      > "$run_dir/server.log" 2>&1 &
    pids+=("$!")
    for _ in $(seq 1 80); do
      if rg -q "Lightyear Matchmaker readiness published|installed matchmaker connection-request handler" "$run_dir/server.log"; then
        break
      fi
      sleep 0.25
    done

    matchmaker_config="$run_dir/matchmaker.toml"
    cat > "$matchmaker_config" <<EOF
    [server]
    bind = "127.0.0.1:{{matchmaker_port}}"

    [game]
    name = "lightrider"
    version = "dev"

    [lightyear]
    protocol_id = ${LIGHTRIDER_PROTOCOL_ID:-0}
    private_key = "${LIGHTRIDER_PRIVATE_KEY:-}"
    client_timeout_secs = 15
    token_expire_secs = 30

    [nats]
    url = "nats://$nats_host"
    username = "$nats_user"
    password = "$nats_password"
    namespace = "$namespace"

    [allocation]
    source = "nats_static"
    require_assignment_prepare = true
    assignment_prepare_timeout_ms = 5000
    assignment_prepare_poll_ms = 25
    EOF
    "$matchmaker_target_dir/$bin_dir/lightyear_matchmaker_server" --config "$matchmaker_config" \
      > "$run_dir/matchmaker.log" 2>&1 &
    pids+=("$!")
    for _ in $(seq 1 80); do
      if rg -q "lightyear matchmaker listening" "$run_dir/matchmaker.log"; then
        break
      fi
      sleep 0.25
    done

    timeout "$(({{seconds}} + 8))" \
      "$target_dir/$bin_dir/lightrider-client" \
        --headless --mode bot --client-id {{client_id}} --config {{config}} --room auto \
        --matchmaker-url "ws://127.0.0.1:{{matchmaker_port}}/ws" \
        --matchmaker-game lightrider \
        --matchmaker-version dev \
        > "$run_dir/client.log" 2>&1 || true

    required_patterns=(
      "installed matchmaker connection-request handler"
      "Lightyear Matchmaker readiness published"
      "assignment.created"
      "assignment.prepared"
      "assignment.ready"
      "Got matchmaker response; connecting to server"
      "matchmaker client connected"
    )
    for pattern in "${required_patterns[@]}"; do
      if ! rg -q "$pattern" "$run_dir"; then
        echo "lightyear-matchmaker local smoke failed: missing '$pattern' in $run_dir" >&2
        echo "logs: $run_dir" >&2
        exit 1
      fi
    done

    echo "lightyear-matchmaker local smoke passed: $run_dir"

# Compile the static-capable game server locally. This is useful before building
# the container image or when testing the VPS server entrypoint by hand.
game-server-build-local release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo build "${cargo_args[@]}" -p server --features lightyear-matchmaker --bin lightrider-server

# Compile the standalone lightyear_matchmaker_server from the sibling repo.
matchmaker-build-local release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    matchmaker_target_dir="${LIGHTYEAR_MATCHMAKER_TARGET_DIR:-../lightyear-matchmaker/target}"
    cargo_args=(-j 2)
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    CARGO_TARGET_DIR="$matchmaker_target_dir" cargo build "${cargo_args[@]}" --manifest-path ../lightyear-matchmaker/Cargo.toml -p lightyear_matchmaker_server --bin lightyear_matchmaker_server

# Build the WASM client with the Lightyear Matchmaker feature enabled.
web-build release="false":
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
    cargo_args=(-j 2)
    wasm_profile="debug"
    if [[ "{{release}}" == "true" ]]; then
      cargo_args+=(--release)
      wasm_profile="release"
    fi
    rustup run nightly cargo build "${cargo_args[@]}" \
      -p web_client \
      --features lightyear-matchmaker \
      --bin lightrider-web \
      --target wasm32-unknown-unknown
    rm -rf web/pkg
    "$wasm_bindgen" \
      --target web \
      --out-dir web/pkg \
      "target/wasm32-unknown-unknown/$wasm_profile/lightrider-web.wasm"
    rm -rf web/assets
    mkdir -p web/assets
    cp -R assets/. web/assets/
    echo "Built web/pkg/lightrider-web.js"

web-serve bind="127.0.0.1" port="8000" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    just web-build release={{release}}
    echo "Serving http://localhost:{{port}}/"
    echo "For the local Lightyear Matchmaker stack, open:"
    echo "http://localhost:{{port}}/?matchmaker_url=ws://127.0.0.1:3000/matchmaker/ws&matchmaker_game=lightrider&matchmaker_version=dev"
    python3 -m http.server "{{port}}" --bind "{{bind}}" --directory web

clean-build:
    cargo clean

clean-incremental:
    rm -rf target/debug/incremental target/*/debug/incremental

clean-edgegap-cache:
    podman builder prune -f

# Stage the multi-repo image build context used by both game-server and
# matchmaker images. The Dockerfiles expect these sibling directories under
# .edgegap-build/context.
edgegap-context:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p .edgegap-build/context
    rm -rf .edgegap-build/context/bevygap
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
      ../lightyear-matchmaker/ .edgegap-build/context/lightyear-matchmaker/
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

# Build the deployable game-server image. The historical recipe name is kept
# because it also builds the image used by Edgegap app versions.
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

# Clear aliases for the game-server image path. These call the older
# edgegap-* recipes because the image is still the Edgegap app-version image.
game-server-build *args:
    just edgegap-build {{args}}

game-server-push tag=edgegap-default-tag:
    just edgegap-push "{{tag}}"

game-server-build-push *args:
    just edgegap-build-push {{args}}

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
    export LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE_OVERRIDE:-{{app}}_${edgegap_version}}"
    just edgegap-app-sync "{{tag}}" "$edgegap_version" "{{app}}"
    just edgegap-app-verify "{{tag}}" "$edgegap_version" "{{app}}"

# Pull one image on the static/control VPS and restart the corresponding
# systemd service. Public wrappers below choose the image/service pair.
_remote-pull-service service image_env image_name *args:
    #!/usr/bin/env bash
    set -euo pipefail
    host=""
    tag=""
    image=""
    ssh_port="22"
    ssh_key=""
    env_file="secrets/web-server.env"
    restart="1"
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        host=*) host="${arg#host=}" ;;
        tag=*) tag="${arg#tag=}" ;;
        image=*) image="${arg#image=}" ;;
        ssh_port=*) ssh_port="${arg#ssh_port=}" ;;
        port=*) ssh_port="${arg#port=}" ;;
        ssh_key=*) ssh_key="${arg#ssh_key=}" ;;
        env=*) env_file="${arg#env=}" ;;
        restart=*) restart="${arg#restart=}" ;;
        *)
          case "$positional" in
            0) host="$arg" ;;
            1) tag="$arg" ;;
            2) ssh_port="$arg" ;;
            3) env_file="$arg" ;;
            4) ssh_key="$arg" ;;
            *)
              echo "unexpected extra argument for remote pull: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    if [[ -z "$host" ]]; then
      echo "usage: just static-server-pull-{game-server,matchmaker} host=<vps-ip-or-host> [tag=<tag>|image=<image>] [ssh_port=22] [env=secrets/web-server.env] [ssh_key=~/.ssh/key] [restart=1]" >&2
      exit 2
    fi
    if [[ -f "$env_file" ]]; then
      source "$env_file"
    fi
    if [[ -f secrets/edgegap.env ]]; then
      source secrets/edgegap.env
    fi
    image_env="{{image_env}}"
    configured_image="${!image_env:-}"
    if [[ -z "$image" ]]; then
      if [[ -n "$configured_image" && -z "$tag" ]]; then
        image="$configured_image"
      else
        : "${EDGEGAP_REGISTRY_PROJECT:?EDGEGAP_REGISTRY_PROJECT is required to build the image name}"
        registry="${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}"
        tag="${tag:-${LIGHTRIDER_MATCHMAKER_TAG:-${EDGEGAP_APP_VERSION:-{{edgegap-default-tag}}}}}"
        image="$registry/$EDGEGAP_REGISTRY_PROJECT/{{image_name}}:$tag"
      fi
    fi
    ssh_key="${ssh_key/#\~\//$HOME/}"
    ssh_opts=(-p "$ssh_port")
    if [[ -n "$ssh_key" ]]; then
      test -f "$ssh_key" || {
        echo "SSH key not found: $ssh_key" >&2
        exit 1
      }
      ssh_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
    fi
    registry_url="${EDGEGAP_REGISTRY_URL:-}"
    registry_user="${EDGEGAP_REGISTRY_USERNAME:-}"
    registry_token="${EDGEGAP_REGISTRY_TOKEN:-}"
    ssh "${ssh_opts[@]}" "root@$host" \
      "REGISTRY_URL=$(printf '%q' "$registry_url") REGISTRY_USER=$(printf '%q' "$registry_user") REGISTRY_TOKEN=$(printf '%q' "$registry_token") IMAGE=$(printf '%q' "$image") IMAGE_ENV=$(printf '%q' "{{image_env}}") SERVICE=$(printf '%q' "{{service}}") RESTART=$(printf '%q' "$restart") bash -s" <<'REMOTE'
    set -euo pipefail
    if [[ -n "${REGISTRY_URL:-}" && -n "${REGISTRY_USER:-}" && -n "${REGISTRY_TOKEN:-}" ]]; then
      printf '%s' "$REGISTRY_TOKEN" | podman login "$REGISTRY_URL" --username "$REGISTRY_USER" --password-stdin
    fi
    podman pull "$IMAGE"
    service_file="/etc/systemd/system/${SERVICE}.service"
    if [[ -f "$service_file" ]]; then
      if grep -Fq "Environment=${IMAGE_ENV}=" "$service_file"; then
        sed -i "s#^Environment=${IMAGE_ENV}=.*#Environment=${IMAGE_ENV}=${IMAGE}#" "$service_file"
      else
        echo "service file $service_file does not define Environment=${IMAGE_ENV}=..." >&2
        exit 1
      fi
      systemctl daemon-reload
    fi
    if [[ "$SERVICE" == "lightrider-static-server" ]]; then
      help="$(podman run --rm --entrypoint /app/lightrider-server "$IMAGE" --help 2>&1 || true)"
      if [[ "$help" != *"--matchmaker"* ]]; then
        cat >&2 <<EOF
    Static server image does not look like the lightyear-matchmaker build:
      $IMAGE

    Expected '/app/lightrider-server --help' to contain '--matchmaker'.
    This usually means the VPS pulled an old Bevygap-era image/tag.

    Observed help output:
    $help
    EOF
        exit 1
      fi
    fi
    if [[ "$RESTART" == "1" || "$RESTART" == "true" || "$RESTART" == "yes" ]]; then
      systemctl restart "$SERVICE"
      systemctl --no-pager --full status "$SERVICE" || true
    fi
    REMOTE
    echo "Pulled $image on $host for {{service}}"

# Pull and restart the static game-server service on the control/static VPS.
static-server-pull-game-server *args:
    just _remote-pull-service lightrider-static-server LIGHTRIDER_STATIC_SERVER_IMAGE lightrider-server {{args}}

# Pull and restart the matchmaker/control service on the control/static VPS.
static-server-pull-matchmaker *args:
    just _remote-pull-service lightrider-matchmaker LIGHTRIDER_MATCHMAKER_IMAGE lightrider-matchmaker {{args}}

# Pull both images on the control/static VPS. Matchmaker restarts first because
# it owns NATS in the current single-container control-host layout.
static-server-pull-all *args:
    just static-server-pull-matchmaker {{args}}
    just static-server-pull-game-server {{args}}

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
    # Keep caller-provided deployment overrides distinct from values sourced out
    # of an older generated env file. The old file is useful for secrets, but it
    # must not pin a redeploy to an obsolete image tag such as "dev".
    caller_has_matchmaker_game=0
    caller_has_matchmaker_version=0
    caller_has_nats_namespace=0
    if [[ "${LIGHTRIDER_MATCHMAKER_GAME+x}" == "x" ]]; then
      caller_has_matchmaker_game=1
      caller_matchmaker_game="$LIGHTRIDER_MATCHMAKER_GAME"
    fi
    if [[ "${LIGHTRIDER_MATCHMAKER_VERSION+x}" == "x" ]]; then
      caller_has_matchmaker_version=1
      caller_matchmaker_version="$LIGHTRIDER_MATCHMAKER_VERSION"
    fi
    if [[ "${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE+x}" == "x" ]]; then
      caller_has_nats_namespace=1
      caller_nats_namespace="$LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE"
    fi
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
    if [[ "${FORCE:-0}" == "1" ]]; then
      unset LIGHTRIDER_MATCHMAKER_IMAGE
      unset LIGHTRIDER_STATIC_SERVER_IMAGE
      unset LIGHTRIDER_MATCHMAKER_TAG
      unset EDGEGAP_APP_VERSION
      if [[ "$caller_has_matchmaker_game" == "1" ]]; then
        LIGHTRIDER_MATCHMAKER_GAME="$caller_matchmaker_game"
      else
        unset LIGHTRIDER_MATCHMAKER_GAME
      fi
      if [[ "$caller_has_matchmaker_version" == "1" ]]; then
        LIGHTRIDER_MATCHMAKER_VERSION="$caller_matchmaker_version"
      else
        unset LIGHTRIDER_MATCHMAKER_VERSION
      fi
      if [[ "$caller_has_nats_namespace" == "1" ]]; then
        LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="$caller_nats_namespace"
      else
        unset LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE
      fi
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
    matchmaker_game="${LIGHTRIDER_MATCHMAKER_GAME:-${EDGEGAP_APP_NAME:-lightrider}}"
    matchmaker_version="${LIGHTRIDER_MATCHMAKER_VERSION:-${EDGEGAP_APP_VERSION:-{{tag}}}}"
    edgegap_app_version="{{edgegap_version}}"
    if [[ -z "$edgegap_app_version" ]]; then
      edgegap_app_version="${EDGEGAP_APP_VERSION:-$matchmaker_version}"
    fi
    if [[ -n "{{edgegap_version}}" ]]; then
      LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE_OVERRIDE:-${matchmaker_game}_${matchmaker_version}}"
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
      echo "# Keep LIGHTRIDER_PROTOCOL_ID/LIGHTRIDER_PRIVATE_KEY in sync with the game-server image."
      write_env LIGHTRIDER_MATCHMAKER_IMAGE "${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}/${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}/lightrider-matchmaker:{{tag}}"
      write_env LIGHTRIDER_MATCHMAKER_TAG "{{tag}}"
      write_env LIGHTRIDER_MATCHMAKER_GAME "$matchmaker_game"
      write_env LIGHTRIDER_MATCHMAKER_VERSION "$matchmaker_version"
      write_env EDGEGAP_APP_NAME "${EDGEGAP_APP_NAME:-$matchmaker_game}"
      write_env EDGEGAP_APP_VERSION "$edgegap_app_version"
      write_env EDGEGAP_API_KEY "${EDGEGAP_API_KEY:-${EDGEGAP_API_TOKEN:-}}"
      write_env EDGEGAP_REGISTRY_URL "${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}"
      write_env EDGEGAP_REGISTRY_PROJECT "${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}"
      write_env EDGEGAP_REGISTRY_USERNAME "${EDGEGAP_REGISTRY_USERNAME:-}"
      write_env EDGEGAP_REGISTRY_TOKEN "${EDGEGAP_REGISTRY_TOKEN:-}"
      write_env LIGHTRIDER_PROTOCOL_ID "$LIGHTRIDER_PROTOCOL_ID"
      write_env LIGHTRIDER_PRIVATE_KEY "$LIGHTRIDER_PRIVATE_KEY"
      write_env LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE "1"
      write_env LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE "${LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE:-nats_static}"
      write_env LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE "${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-${matchmaker_game}_${matchmaker_version}}"
      write_env NATS_USER "${NATS_USER:-lightrider}"
      write_env NATS_PASSWORD "$NATS_PASSWORD"
      write_env NATS_ALLOW_INSECURE "${NATS_ALLOW_INSECURE:-1}"
      write_env LIGHTRIDER_ENABLE_HTTPS "$enable_https"
      write_env LIGHTRIDER_WEB_DOMAIN "$web_domain"
      write_env LIGHTRIDER_CADDY_EMAIL "${LIGHTRIDER_CADDY_EMAIL:-}"
      write_env MATCHMAKER_CORS "${MATCHMAKER_CORS:-$default_matchmaker_cors}"
      write_env LIGHTRIDER_MATCHMAKER_URL "${LIGHTRIDER_MATCHMAKER_URL:-$default_matchmaker_url}"
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

web-server-env-check file="secrets/web-server.env":
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{file}}" || {
      echo "{{file}} does not exist. Run: just web-server-env-template" >&2
      exit 1
    }
    set -a
    source "{{file}}"
    set +a
    require_var() {
      local name="$1"
      if [[ -z "${!name:-}" ]]; then
        echo "required deployment env is empty or missing: $name" >&2
        exit 1
      fi
    }
    truthy() {
      case "${1:-}" in
        1|true|TRUE|True|yes|YES|Yes|y|Y|on|ON|On) return 0 ;;
        *) return 1 ;;
      esac
    }

    require_var LIGHTRIDER_MATCHMAKER_IMAGE
    require_var LIGHTRIDER_MATCHMAKER_TAG
    require_var LIGHTRIDER_PROTOCOL_ID
    require_var LIGHTRIDER_PRIVATE_KEY
    require_var LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE
    require_var LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE
    require_var NATS_USER
    require_var NATS_PASSWORD

    if [[ ! "$LIGHTRIDER_PROTOCOL_ID" =~ ^[0-9]+$ || "$LIGHTRIDER_PROTOCOL_ID" == "0" ]]; then
      echo "LIGHTRIDER_PROTOCOL_ID must be a nonzero integer" >&2
      exit 1
    fi
    if [[ "$NATS_USER" == "lightrider" && "$NATS_PASSWORD" == "lightrider" ]]; then
      echo "NATS_USER/NATS_PASSWORD still use local dev defaults; generate production credentials first" >&2
      exit 1
    fi
    if [[ "$LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE" == "edgegap" ]]; then
      require_var EDGEGAP_API_KEY
    fi
    if truthy "${LIGHTRIDER_RUN_STATIC_SERVER:-1}"; then
      require_var LIGHTRIDER_STATIC_SERVER_IMAGE
      require_var LIGHTRIDER_STATIC_PUBLIC_IP
      require_var LIGHTRIDER_STATIC_PORT
    fi
    if truthy "${LIGHTRIDER_ENABLE_HTTPS:-0}"; then
      require_var LIGHTRIDER_WEB_DOMAIN
      require_var LIGHTRIDER_MATCHMAKER_URL
      if [[ "$LIGHTRIDER_MATCHMAKER_URL" != wss://* ]]; then
        echo "LIGHTRIDER_MATCHMAKER_URL should use wss:// when HTTPS is enabled" >&2
        exit 1
      fi
    fi
    if [[ "$LIGHTRIDER_MATCHMAKER_IMAGE" == registry.edgegap.com/* || "${LIGHTRIDER_STATIC_SERVER_IMAGE:-}" == registry.edgegap.com/* ]]; then
      require_var EDGEGAP_REGISTRY_USERNAME
      require_var EDGEGAP_REGISTRY_TOKEN
    fi

    echo "web-server env ok: {{file}}"

web-server-install host="45.79.138.102" ssh_port="22" env="secrets/web-server.env" ssh_key="":
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{env}}" || {
      echo "{{env}} does not exist. Run: just web-server-env-template" >&2
      exit 1
    }
    just web-server-env-check "{{env}}"
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

web-server-enable-nats-tls-from-caddy *args:
    #!/usr/bin/env bash
    set -euo pipefail
    host="45.79.138.102"
    domain="45.79.138.102.sslip.io"
    ssh_port="22"
    ssh_key=""
    positional=0
    for arg in {{args}}; do
      case "$arg" in
        host=*) host="${arg#host=}" ;;
        domain=*) domain="${arg#domain=}" ;;
        web_domain=*) domain="${arg#web_domain=}" ;;
        ssh_port=*) ssh_port="${arg#ssh_port=}" ;;
        port=*) ssh_port="${arg#port=}" ;;
        ssh_key=*) ssh_key="${arg#ssh_key=}" ;;
        *)
          case "$positional" in
            0) host="$arg" ;;
            1) domain="$arg" ;;
            2) ssh_port="$arg" ;;
            3) ssh_key="$arg" ;;
            *)
              echo "unexpected extra argument for web-server-enable-nats-tls-from-caddy: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    domain="${domain#http://}"
    domain="${domain#https://}"
    domain="${domain%%/*}"
    if [[ -z "$host" || -z "$domain" ]]; then
      echo "usage: just web-server-enable-nats-tls-from-caddy host=<vps-ip> domain=<domain> [ssh_key=~/.ssh/key] [ssh_port=22]" >&2
      exit 2
    fi
    if [[ "$ssh_key" == "~/"* ]]; then
      ssh_key="${HOME}/${ssh_key#~/}"
    fi
    ssh_opts=(-p "$ssh_port")
    if [[ -n "$ssh_key" ]]; then
      test -f "$ssh_key" || {
        echo "SSH key not found: $ssh_key" >&2
        exit 1
      }
      ssh_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
    fi
    ssh "${ssh_opts[@]}" "root@$host" "LIGHTRIDER_WEB_DOMAIN=$(printf '%q' "$domain") bash -s" <<'REMOTE'
    set -euo pipefail
    domain="${LIGHTRIDER_WEB_DOMAIN:?LIGHTRIDER_WEB_DOMAIN is required}"
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
    grep -v -E '^(NATS_ALLOW_INSECURE|NATS_TLS_CERT|NATS_TLS_KEY|MATCHMAKER_NATS_HOST|LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS)=' "$env_file" > "$tmp" || true
    cat "$tmp" > "$env_file"
    rm -f "$tmp"
    {
      printf 'NATS_ALLOW_INSECURE=0\n'
      printf 'NATS_TLS_CERT=%s\n' "$cert_dest"
      printf 'NATS_TLS_KEY=%s\n' "$key_dest"
      printf 'MATCHMAKER_NATS_HOST=%s:4222\n' "$domain"
      printf 'LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS=1\n'
    } >> "$env_file"
    chmod 600 "$env_file"

    if [[ -f "$static_env_file" ]]; then
      tmp="$(mktemp)"
      grep -v -E '^(NATS_HOST|NATS_INSECURE|LIGHTYEAR_MATCHMAKER_NATS_URL|LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS)=' "$static_env_file" > "$tmp" || true
      cat "$tmp" > "$static_env_file"
      rm -f "$tmp"
      {
        printf 'NATS_HOST=%s:4222\n' "$domain"
        printf 'LIGHTYEAR_MATCHMAKER_NATS_URL=tls://%s:4222\n' "$domain"
        printf 'LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS=1\n'
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
    just web-server-env-check "$env_file"
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
