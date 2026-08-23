server config="config/test.ron" port="5000" release="false" bots="":
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    server_args=(--headless --port {{ port }} --config {{ config }})
    if [[ -n "{{ bots }}" ]]; then
      server_args+=(--bots {{ bots }})
    fi
    cargo run "${cargo_args[@]}" -p server --no-default-features --features server --bin lightrider-server -- "${server_args[@]}"

client id="1" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client -- --client-id {{ id }} --server-addr {{ server_addr }} --server-port {{ port }} --config {{ config }} --room {{ room }}

client-debug id="1" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client -- --debug --client-id {{ id }} --server-addr {{ server_addr }} --server-port {{ port }} --config {{ config }} --room {{ room }}

bot id="1001" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client -- --headless --mode player --auto-respawn --client-id {{ id }} --server-addr {{ server_addr }} --server-port {{ port }} --config {{ config }} --room {{ room }}

bots count="4" first_id="1001" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'jobs -pr | xargs -r kill' EXIT
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    for i in $(seq 0 $(({{ count }} - 1))); do
      cargo run "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client -- --headless --mode player --auto-respawn --client-id $(({{ first_id }} + i)) --server-addr {{ server_addr }} --server-port {{ port }} --config {{ config }} --room {{ room }} &
      sleep 1
    done
    wait

local *args:
    #!/usr/bin/env bash
    set -euo pipefail
    bots_arg="4"
    config_arg="config/test.ron"
    port_arg="5000"
    client_id_arg="1"
    first_bot_id_arg="1001"
    room_arg="auto"
    release_arg="false"
    headless_arg="true"
    name_arg=""
    auto_respawn_arg="true"

    args=({{ args }})
    positionals=()
    for ((idx = 0; idx < ${#args[@]}; idx++)); do
      arg="${args[$idx]}"
      case "$arg" in
        bots=*) bots_arg="${arg#*=}" ;;
        config=*) config_arg="${arg#*=}" ;;
        port=*) port_arg="${arg#*=}" ;;
        client_id=*|client-id=*) client_id_arg="${arg#*=}" ;;
        first_bot_id=*|first-bot-id=*) first_bot_id_arg="${arg#*=}" ;;
        room=*) room_arg="${arg#*=}" ;;
        release=*) release_arg="${arg#*=}" ;;
        headless=*|--headless=*) headless_arg="${arg#*=}" ;;
        --headless)
          if [[ $((idx + 1)) -lt ${#args[@]} && "${args[$((idx + 1))]}" != *=* && "${args[$((idx + 1))]}" != --* ]]; then
            idx=$((idx + 1))
            headless_arg="${args[$idx]}"
          else
            headless_arg="true"
          fi
          ;;
        name=*|--name=*) name_arg="${arg#*=}" ;;
        --name)
          idx=$((idx + 1))
          name_arg="${args[$idx]:-}"
          ;;
        auto_respawn=*|auto-respawn=*|--auto-respawn=*) auto_respawn_arg="${arg#*=}" ;;
        --auto-respawn) auto_respawn_arg="true" ;;
        --no-auto-respawn) auto_respawn_arg="false" ;;
        *) positionals+=("$arg") ;;
      esac
    done

    if [[ ${#positionals[@]} -gt 0 ]]; then bots_arg="${positionals[0]}"; fi
    if [[ ${#positionals[@]} -gt 1 ]]; then config_arg="${positionals[1]}"; fi
    if [[ ${#positionals[@]} -gt 2 ]]; then port_arg="${positionals[2]}"; fi
    if [[ ${#positionals[@]} -gt 3 ]]; then client_id_arg="${positionals[3]}"; fi
    if [[ ${#positionals[@]} -gt 4 ]]; then first_bot_id_arg="${positionals[4]}"; fi
    if [[ ${#positionals[@]} -gt 5 ]]; then room_arg="${positionals[5]}"; fi
    if [[ ${#positionals[@]} -gt 6 ]]; then release_arg="${positionals[6]}"; fi
    if [[ ${#positionals[@]} -gt 7 ]]; then headless_arg="${positionals[7]}"; fi
    if [[ ${#positionals[@]} -gt 8 ]]; then name_arg="${positionals[8]}"; fi
    if [[ ${#positionals[@]} -gt 9 ]]; then auto_respawn_arg="${positionals[9]}"; fi

    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    # Keep the interactive local run responsive by default. Set
    # LIGHTRIDER_NETWORK_CONDITIONER=average explicitly when testing latency.
    : "${LIGHTRIDER_NETWORK_CONDITIONER:=none}"
    export LIGHTRIDER_NETWORK_CONDITIONER
    trap 'jobs -pr | xargs -r kill' EXIT
    cargo_args=(-j 2)
    if [[ "$release_arg" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p server --no-default-features --features server --bin lightrider-server -- --headless --port "$port_arg" --config "$config_arg" --bots "$bots_arg" &
    sleep 2
    foreground_client_args=(--client-id "$client_id_arg" --server-port "$port_arg" --config "$config_arg" --room "$room_arg")
    if [[ -n "$name_arg" ]]; then
      foreground_client_args+=(--name "$name_arg")
    fi
    if [[ "$auto_respawn_arg" == "true" || "$auto_respawn_arg" == "1" || "$auto_respawn_arg" == "yes" ]]; then
      foreground_client_args+=(--auto-respawn)
    fi
    if [[ "$headless_arg" == "true" || "$headless_arg" == "1" || "$headless_arg" == "yes" ]]; then
      foreground_client_args=(--headless --mode player "${foreground_client_args[@]}")
    fi
    cargo run "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client -- "${foreground_client_args[@]}"

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
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    target_dir="${CARGO_TARGET_DIR:-target}"
    cargo build "${cargo_args[@]}" -p server --no-default-features --features server --bin lightrider-server
    cargo build "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client
    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
    }
    trap cleanup EXIT
    RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/server.ndjson" \
      "$target_dir/$bin_dir/lightrider-server" --headless --port {{ port }} --config {{ config }} \
      > "$run_dir/server.log" 2>&1 &
    pids+=("$!")
    sleep 2
    for i in $(seq 0 $(({{ clients }} - 1))); do
      id=$(({{ first_client_id }} + i))
      RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-$id.ndjson" \
        "$target_dir/$bin_dir/lightrider-client" --headless --mode player --auto-respawn --client-id "$id" --server-port {{ port }} --config {{ config }} --room {{ room }} \
        > "$run_dir/client-$id.log" 2>&1 &
      pids+=("$!")
      sleep 1
    done
    sleep {{ seconds }}
    cleanup
    trap - EXIT
    echo "trace run: $run_dir"
    if command -v duckdb >/dev/null 2>&1; then
      duckdb -batch -cmd "SET VARIABLE trace_glob = '$run_dir/*.ndjson';" < tools/debug_trace_summary.sql | tee "$run_dir/summary.txt"
    else
      echo "duckdb not found; inspect $run_dir/*.ndjson manually"
    fi

trace-summary dir="logs/debug/latest":
    duckdb -batch -cmd "SET VARIABLE trace_glob = '{{ dir }}/*.ndjson';" < tools/debug_trace_summary.sql

interpolation-stress *args:
    #!/usr/bin/env bash
    set -euo pipefail
    seconds="20"
    config="config/interpolation_stress.ron"
    port="5100"
    clients="3"
    first_client_id="5001"
    room="auto"
    release="false"
    max_step_per_tick="64"
    args=({{ args }})
    for arg in "${args[@]}"; do
      case "$arg" in
        seconds=*) seconds="${arg#*=}" ;;
        config=*) config="${arg#*=}" ;;
        port=*) port="${arg#*=}" ;;
        clients=*) clients="${arg#*=}" ;;
        first_client_id=*) first_client_id="${arg#*=}" ;;
        room=*) room="${arg#*=}" ;;
        release=*) release="${arg#*=}" ;;
        max_step_per_tick=*) max_step_per_tick="${arg#*=}" ;;
        *) echo "unknown interpolation-stress argument: $arg" >&2; exit 2 ;;
      esac
    done
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    run_dir="logs/debug/$(date +%Y%m%d-%H%M%S)-interpolation-stress"
    mkdir -p "$run_dir"
    ln -sfn "$(basename "$run_dir")" logs/debug/latest-interpolation-stress
    unset LIGHTRIDER_NETWORK_CONDITIONER
    cargo_args=(-j 4)
    bin_dir="debug"
    if [[ "$release" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    target_dir="${CARGO_TARGET_DIR:-target}"
    cargo build "${cargo_args[@]}" -p server --no-default-features --features server --bin lightrider-server
    cargo build "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client
    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
    }
    trap cleanup EXIT
    RUST_LOG="info,lightyear_debug=trace,server::food=warn" LIGHTYEAR_DEBUG_FILE="$run_dir/server.ndjson" \
      "$target_dir/$bin_dir/lightrider-server" --headless --port "$port" --config "$config" \
      > "$run_dir/server.log" 2>&1 &
    pids+=("$!")
    sleep 2
    for i in $(seq 0 $((clients - 1))); do
      id=$((first_client_id + i))
      RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-bot-$id.ndjson" \
        "$target_dir/$bin_dir/lightrider-client" --headless --mode player --auto-respawn --client-id "$id" --server-port "$port" --config "$config" --room "$room" \
        > "$run_dir/client-bot-$id.log" 2>&1 &
      pids+=("$!")
      sleep 0.5
    done
    sleep "$seconds"
    cleanup
    trap - EXIT
    echo "interpolation stress run: $run_dir"
    python3 tools/interpolation_stress_check.py "$run_dir" --max-interpolated-step-per-tick "$max_step_per_tick"

# Load-test commands are intentionally separate from trace-local:
# - `load-local` starts a server, many headless player clients, OS samplers, and
#   low-rate Lightyear JSONL tracing for the server plus a small client sample.
# - `fake-clients` only launches headless player clients against an already-running
#   server, useful for testing deployed or manually started servers.
# Metrics written under logs/load include CPU, RSS/HWM memory, network interface
# throughput, frame/link trace rows, transport packet rows, and received fragment
# counts from Lightyear debug traces.
load-help:
    #!/usr/bin/env bash
    set -euo pipefail
    cat <<'EOF'
    Local full-stack load test:
      just load-local clients=100 seconds=60 release=true
      just load-local clients=100 seconds=60 net_interfaces=lo release=true

    Attach fake clients to an existing local or deployed server:
      just fake-clients count=100 server_addr=127.0.0.1 port=5000 release=true
      just fake-clients count=100 server_addr=<edgegap-ip> port=<external-port> net_interfaces=eth0 release=true

    Summarize an existing load run:
      just load-summary dir=logs/load/latest

    Chrome-trace server profiling with agent-readable summaries:
      just profile-chrome clients=50 seconds=20 release=true
      just profile-chrome clients=10 seconds=10 release=false incremental=true

    Outputs:
      logs/load/<timestamp>/process_metrics.csv  CPU/RSS/HWM/thread/fd samples
      logs/load/<timestamp>/network_metrics.csv  per-interface bandwidth samples
      logs/load/<timestamp>/*.ndjson             sampled Lightyear debug traces
      logs/load/<timestamp>/load_summary.txt     high-level summary
      logs/profile/<timestamp>/chrome/profile_summary.md
    EOF

load-local *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    clients="100"
    seconds="60"
    config="config/load.ron"
    port="5000"
    first_client_id="2001"
    room="auto"
    release="true"
    ramp_per_second="20"
    trace_clients="2"
    sample_interval="1"
    server_trace="true"
    net_interfaces="lo"
    args=({{ args }})
    for arg in "${args[@]}"; do
      case "$arg" in
        clients=*) clients="${arg#*=}" ;;
        seconds=*) seconds="${arg#*=}" ;;
        config=*) config="${arg#*=}" ;;
        port=*) port="${arg#*=}" ;;
        first_client_id=*) first_client_id="${arg#*=}" ;;
        room=*) room="${arg#*=}" ;;
        release=*) release="${arg#*=}" ;;
        ramp_per_second=*) ramp_per_second="${arg#*=}" ;;
        trace_clients=*) trace_clients="${arg#*=}" ;;
        sample_interval=*) sample_interval="${arg#*=}" ;;
        server_trace=*) server_trace="${arg#*=}" ;;
        net_interfaces=*) net_interfaces="${arg#*=}" ;;
        *) echo "unknown load-local argument: $arg" >&2; exit 2 ;;
      esac
    done
    run_dir="logs/load/$(date +%Y%m%d-%H%M%S)-local-c$clients"
    mkdir -p "$run_dir"
    mkdir -p logs/load
    ln -sfn "$(realpath "$run_dir")" logs/load/latest
    cargo_args=(-j 2)
    bin_dir="debug"
    if [[ "$release" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    target_dir="${CARGO_TARGET_DIR:-target}"
    cargo build "${cargo_args[@]}" -p server --no-default-features --features server --bin lightrider-server
    cargo build "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client
    pids_file="$run_dir/pids.csv"
    echo "role,name,pid" > "$pids_file"
    pids=()
    sampler_pid=""
    cleanup() {
      set +e
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      if [[ -n "$sampler_pid" ]]; then
        kill "$sampler_pid" 2>/dev/null || true
      fi
      wait 2>/dev/null || true
    }
    trap cleanup EXIT INT TERM
    LOAD_NET_INTERFACES="$net_interfaces" tools/load_sampler.sh "$run_dir" "$pids_file" "$sample_interval" &
    sampler_pid="$!"
    if [[ "$server_trace" == "true" ]]; then
      RUST_LOG="warn,lightrider::interest=info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/server.ndjson" \
        "$target_dir/$bin_dir/lightrider-server" --headless --port "$port" --config "$config" \
        > "$run_dir/server.log" 2>&1 &
    else
      RUST_LOG="warn,lightrider::interest=info" \
        "$target_dir/$bin_dir/lightrider-server" --headless --port "$port" --config "$config" \
        > "$run_dir/server.log" 2>&1 &
    fi
    server_pid="$!"
    pids+=("$server_pid")
    echo "server,server,$server_pid" >> "$pids_file"
    sleep 2
    if (( ramp_per_second > 0 )); then
      sleep_interval="$(awk -v rate="$ramp_per_second" 'BEGIN { printf "%.3f", 1.0 / rate }')"
    else
      sleep_interval="0"
    fi
    for i in $(seq 0 $((clients - 1))); do
      id=$((first_client_id + i))
      if (( i < trace_clients )); then
        RUST_LOG="warn,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-$id.ndjson" \
          "$target_dir/$bin_dir/lightrider-client" --headless --mode player --auto-respawn --client-id "$id" --server-port "$port" --config "$config" --room "$room" \
          > "$run_dir/client-$id.log" 2>&1 &
      else
        RUST_LOG="warn" \
          "$target_dir/$bin_dir/lightrider-client" --headless --mode player --auto-respawn --client-id "$id" --server-port "$port" --config "$config" --room "$room" \
          > "$run_dir/client-$id.log" 2>&1 &
      fi
      client_pid="$!"
      pids+=("$client_pid")
      echo "client,client-$id,$client_pid" >> "$pids_file"
      sleep "$sleep_interval"
    done
    sleep "$seconds"
    cleanup
    trap - EXIT INT TERM
    echo "load run: $run_dir"
    tools/load_summary.py "$run_dir" | tee "$run_dir/load_summary.stdout.txt"
    if command -v duckdb >/dev/null 2>&1; then
      duckdb -batch -cmd "SET VARIABLE trace_glob = '$run_dir/*.ndjson';" < tools/debug_trace_summary.sql > "$run_dir/duckdb_summary.txt" 2>&1 || true
    fi

fake-clients *args:
    #!/usr/bin/env bash
    set -euo pipefail
    count="100"
    first_id="2001"
    config="config/load.ron"
    server_addr="127.0.0.1"
    port="5000"
    room="auto"
    release="true"
    ramp_per_second="20"
    seconds="0"
    run_dir=""
    trace_clients="0"
    sample_interval="1"
    net_interfaces=""
    args=({{ args }})
    for arg in "${args[@]}"; do
      case "$arg" in
        count=*) count="${arg#*=}" ;;
        first_id=*) first_id="${arg#*=}" ;;
        config=*) config="${arg#*=}" ;;
        server_addr=*) server_addr="${arg#*=}" ;;
        port=*) port="${arg#*=}" ;;
        room=*) room="${arg#*=}" ;;
        release=*) release="${arg#*=}" ;;
        ramp_per_second=*) ramp_per_second="${arg#*=}" ;;
        seconds=*) seconds="${arg#*=}" ;;
        run_dir=*) run_dir="${arg#*=}" ;;
        trace_clients=*) trace_clients="${arg#*=}" ;;
        sample_interval=*) sample_interval="${arg#*=}" ;;
        net_interfaces=*) net_interfaces="${arg#*=}" ;;
        *) echo "unknown fake-clients argument: $arg" >&2; exit 2 ;;
      esac
    done
    if [[ -z "$run_dir" ]]; then
      run_dir="logs/load/$(date +%Y%m%d-%H%M%S)-fake-c$count"
    else
      run_dir="$run_dir"
    fi
    mkdir -p "$run_dir"
    mkdir -p logs/load
    ln -sfn "$(realpath "$run_dir")" logs/load/latest
    cargo_args=(-j 2)
    bin_dir="debug"
    if [[ "$release" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    target_dir="${CARGO_TARGET_DIR:-target}"
    cargo build "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client
    pids_file="$run_dir/pids.csv"
    echo "role,name,pid" > "$pids_file"
    pids=()
    sampler_pid=""
    cleanup() {
      set +e
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      if [[ -n "$sampler_pid" ]]; then
        kill "$sampler_pid" 2>/dev/null || true
      fi
      wait 2>/dev/null || true
    }
    trap cleanup EXIT INT TERM
    LOAD_NET_INTERFACES="$net_interfaces" tools/load_sampler.sh "$run_dir" "$pids_file" "$sample_interval" &
    sampler_pid="$!"
    if (( ramp_per_second > 0 )); then
      sleep_interval="$(awk -v rate="$ramp_per_second" 'BEGIN { printf "%.3f", 1.0 / rate }')"
    else
      sleep_interval="0"
    fi
    for i in $(seq 0 $((count - 1))); do
      id=$((first_id + i))
      if (( i < trace_clients )); then
        RUST_LOG="warn,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-$id.ndjson" \
          "$target_dir/$bin_dir/lightrider-client" --headless --mode player --auto-respawn --client-id "$id" --server-addr "$server_addr" --server-port "$port" --config "$config" --room "$room" \
          > "$run_dir/client-$id.log" 2>&1 &
      else
        RUST_LOG="warn" \
          "$target_dir/$bin_dir/lightrider-client" --headless --mode player --auto-respawn --client-id "$id" --server-addr "$server_addr" --server-port "$port" --config "$config" --room "$room" \
          > "$run_dir/client-$id.log" 2>&1 &
      fi
      client_pid="$!"
      pids+=("$client_pid")
      echo "client,client-$id,$client_pid" >> "$pids_file"
      sleep "$sleep_interval"
    done
    echo "fake clients running: count=$count run_dir=$run_dir"
    if (( seconds > 0 )); then
      sleep "$seconds"
      cleanup
      trap - EXIT INT TERM
      tools/load_summary.py "$run_dir" | tee "$run_dir/load_summary.stdout.txt"
    else
      wait
    fi

load-summary *args:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="logs/load/latest"
    args=({{ args }})
    for arg in "${args[@]}"; do
      case "$arg" in
        dir=*) dir="${arg#*=}" ;;
        *) dir="$arg" ;;
      esac
    done
    tools/load_summary.py "$dir"

profile-chrome *args:
    #!/usr/bin/env bash
    set -euo pipefail
    if [[ -f secrets/admin.env ]]; then
      set -a
      source secrets/admin.env
      set +a
    fi
    clients="50"
    seconds="20"
    config="config/load.ron"
    port="5000"
    first_client_id="4001"
    room="auto"
    release="true"
    ramp_per_second="20"
    sample_interval="1"
    net_interfaces="lo"
    rust_log="warn,bevy_ecs=trace,bevy_app=trace"
    incremental="false"
    args=({{ args }})
    for arg in "${args[@]}"; do
      case "$arg" in
        clients=*) clients="${arg#*=}" ;;
        seconds=*) seconds="${arg#*=}" ;;
        config=*) config="${arg#*=}" ;;
        port=*) port="${arg#*=}" ;;
        first_client_id=*) first_client_id="${arg#*=}" ;;
        room=*) room="${arg#*=}" ;;
        release=*) release="${arg#*=}" ;;
        ramp_per_second=*) ramp_per_second="${arg#*=}" ;;
        sample_interval=*) sample_interval="${arg#*=}" ;;
        net_interfaces=*) net_interfaces="${arg#*=}" ;;
        rust_log=*) rust_log="${arg#*=}" ;;
        incremental=*) incremental="${arg#*=}" ;;
        *) echo "unknown profile-chrome argument: $arg" >&2; exit 2 ;;
      esac
    done
    run_dir="logs/profile/$(date +%Y%m%d-%H%M%S)-chrome-c$clients"
    mkdir -p "$run_dir"
    mkdir -p logs/profile
    ln -sfn "$(realpath "$run_dir")" logs/profile/latest
    cargo_args=(-j 2)
    bin_dir="debug"
    if [[ "$release" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    cargo_incremental="0"
    if [[ "$incremental" == "true" ]]; then
      cargo_incremental="1"
    fi
    target_dir="${CARGO_TARGET_DIR:-target}"
    CARGO_INCREMENTAL="$cargo_incremental" cargo build "${cargo_args[@]}" -p server --no-default-features --features "server profile-chrome" --bin lightrider-server
    CARGO_INCREMENTAL="$cargo_incremental" cargo build "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client
    pids_file="$run_dir/pids.csv"
    echo "role,name,pid" > "$pids_file"
    client_pids=()
    server_pid=""
    sampler_pid=""
    cleanup() {
      set +e
      for pid in "${client_pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      if [[ -n "$server_pid" ]]; then
        kill "$server_pid" 2>/dev/null || true
      fi
      if [[ -n "$sampler_pid" ]]; then
        kill "$sampler_pid" 2>/dev/null || true
      fi
      wait 2>/dev/null || true
    }
    trap cleanup EXIT INT TERM
    LOAD_NET_INTERFACES="$net_interfaces" tools/load_sampler.sh "$run_dir" "$pids_file" "$sample_interval" &
    sampler_pid="$!"
    trace_path="$run_dir/server.trace.json"
    server_runtime="$(awk -v clients="$clients" -v rate="$ramp_per_second" -v seconds="$seconds" 'BEGIN { ramp = rate > 0 ? clients / rate : 0; printf "%.3f", seconds + ramp + 5.0 }')"
    TRACE_CHROME="$trace_path" RUST_LOG="$rust_log" \
      "$target_dir/$bin_dir/lightrider-server" --headless --port "$port" --config "$config" --run-seconds "$server_runtime" \
      > "$run_dir/server.log" 2>&1 &
    server_pid="$!"
    echo "server,server,$server_pid" >> "$pids_file"
    sleep 2
    if (( ramp_per_second > 0 )); then
      sleep_interval="$(awk -v rate="$ramp_per_second" 'BEGIN { printf "%.3f", 1.0 / rate }')"
    else
      sleep_interval="0"
    fi
    if (( clients > 0 )); then
      for i in $(seq 0 $((clients - 1))); do
        id=$((first_client_id + i))
        RUST_LOG="warn" \
          "$target_dir/$bin_dir/lightrider-client" --headless --mode player --auto-respawn --client-id "$id" --server-port "$port" --config "$config" --room "$room" \
          > "$run_dir/client-$id.log" 2>&1 &
        client_pid="$!"
        client_pids+=("$client_pid")
        echo "client,client-$id,$client_pid" >> "$pids_file"
        sleep "$sleep_interval"
      done
    fi
    sleep "$seconds"
    for pid in "${client_pids[@]}"; do
      kill "$pid" 2>/dev/null || true
    done
    if ((${#client_pids[@]} > 0)); then
      wait "${client_pids[@]}" 2>/dev/null || true
    fi
    client_pids=()
    server_status=0
    if [[ -n "$server_pid" ]]; then
      wait "$server_pid" || server_status=$?
      server_pid=""
    fi
    if [[ -n "$sampler_pid" ]]; then
      kill "$sampler_pid" 2>/dev/null || true
      wait "$sampler_pid" 2>/dev/null || true
      sampler_pid=""
    fi
    trap - EXIT INT TERM
    echo "profile run: $run_dir"
    tools/load_summary.py "$run_dir" | tee "$run_dir/load_summary.stdout.txt"
    if [[ -s "$trace_path" ]]; then
      tools/chrome_trace_summary.py "$trace_path" "$run_dir/chrome" | tee "$run_dir/chrome_summary.stdout.txt"
    else
      echo "Chrome trace was not written: $trace_path" >&2
      server_status=1
    fi
    exit "$server_status"

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
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    target_dir="${CARGO_TARGET_DIR:-target}"
    cargo build "${cargo_args[@]}" -p server --no-default-features --features server --bin lightrider-server
    cargo build "${cargo_args[@]}" -p client --no-default-features --features client --bin lightrider-client
    pids=()
    cleanup() {
      for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
      done
      wait 2>/dev/null || true
    }
    trap cleanup EXIT
    RUST_LOG="info,lightyear_debug=trace,server::food=warn" LIGHTYEAR_DEBUG_FILE="$run_dir/server.ndjson" \
      "$target_dir/$bin_dir/lightrider-server" --headless --port {{ port }} --config {{ config }} \
      > "$run_dir/server.log" 2>&1 &
    pids+=("$!")
    sleep 2
    RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-player-{{ player_id }}.ndjson" \
      "$target_dir/$bin_dir/lightrider-client" --headless --mode player --client-id {{ player_id }} --server-port {{ port }} --config {{ config }} --room {{ room }} \
      > "$run_dir/client-player-{{ player_id }}.log" 2>&1 &
    pids+=("$!")
    sleep 1
    if (( {{ bot_clients }} > 0 )); then
      for i in $(seq 0 $(({{ bot_clients }} - 1))); do
        id=$(({{ first_bot_id }} + i))
        RUST_LOG="info,lightyear_debug=trace" LIGHTYEAR_DEBUG_FILE="$run_dir/client-bot-$id.ndjson" \
          "$target_dir/$bin_dir/lightrider-client" --headless --mode player --auto-respawn --client-id "$id" --server-port {{ port }} --config {{ config }} --room {{ room }} \
          > "$run_dir/client-bot-$id.log" 2>&1 &
        pids+=("$!")
        sleep 1
      done
    fi
    sleep {{ seconds }}
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
# command and checks logs for assignment and client-connect events.
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
    export LIGHTRIDER_PUBLIC_PORT="${LIGHTRIDER_PUBLIC_PORT:-{{ port }}}"
    export SELF_SIGNED_SANS="${SELF_SIGNED_SANS:-127.0.0.1,localhost}"
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p server --no-default-features --features "server lightyear-matchmaker" --bin lightrider-server -- --headless --matchmaker --port {{ port }} --config {{ config }}

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
    config="{{ config_path }}"
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
    bind = "{{ bind }}"

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
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" --manifest-path ../lightyear-matchmaker/Cargo.toml -p lightyear_matchmaker_server -- --config "$config"

# Run one headless client through the matchmaker WebSocket API.
lightyear-matchmaker-client-bot id="3001" config="config/test.ron" matchmaker_url="ws://127.0.0.1:3000/ws" game="lightrider" version="dev" room="auto" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo run "${cargo_args[@]}" -p client --no-default-features --features "client lightyear-matchmaker" --bin lightrider-client -- --headless --mode player --auto-respawn --client-id {{ id }} --config {{ config }} --room {{ room }} --matchmaker-url {{ matchmaker_url }} --matchmaker-game {{ game }} --matchmaker-version {{ version }}

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
    matchmaker_target_dir="${LIGHTYEAR_MATCHMAKER_TARGET_DIR:-${CARGO_TARGET_DIR:-../lightyear-matchmaker/target}}"
    bin_dir="debug"
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
      bin_dir="release"
    fi
    nats_host="${LIGHTYEAR_MATCHMAKER_SMOKE_NATS_HOST:-127.0.0.1:4222}"
    nats_user="${LIGHTYEAR_MATCHMAKER_SMOKE_NATS_USER:-lightrider}"
    nats_password="${LIGHTYEAR_MATCHMAKER_SMOKE_NATS_PASSWORD:-lightrider}"
    namespace="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-${MATCHMAKER_NATS_NAMESPACE:-lightrider_dev}}"
    cargo build "${cargo_args[@]}" -p server --no-default-features --features "server lightyear-matchmaker" --bin lightrider-server
    cargo build "${cargo_args[@]}" -p client --no-default-features --features "client lightyear-matchmaker" --bin lightrider-client
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
      LIGHTRIDER_PUBLIC_PORT="${LIGHTRIDER_PUBLIC_PORT:-{{ port }}}" \
      SELF_SIGNED_SANS="${SELF_SIGNED_SANS:-127.0.0.1,localhost}" \
      "$target_dir/$bin_dir/lightrider-server" --headless --matchmaker --port {{ port }} --config {{ config }} \
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
    bind = "127.0.0.1:{{ matchmaker_port }}"

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

    timeout "$(({{ seconds }} + 8))" \
      "$target_dir/$bin_dir/lightrider-client" \
        --headless --mode player --auto-respawn --client-id {{ client_id }} --config {{ config }} --room auto \
        --matchmaker-url "ws://127.0.0.1:{{ matchmaker_port }}/ws" \
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
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
    fi
    cargo build "${cargo_args[@]}" -p server --no-default-features --features "server lightyear-matchmaker" --bin lightrider-server

# Compile the standalone lightyear_matchmaker_server from the sibling repo.
matchmaker-build-local release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    matchmaker_target_dir="${LIGHTYEAR_MATCHMAKER_TARGET_DIR:-${CARGO_TARGET_DIR:-../lightyear-matchmaker/target}}"
    cargo_args=(-j 2)
    if [[ "{{ release }}" == "true" ]]; then
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
    wasm_bindgen_version="${WASM_BINDGEN_CLI_VERSION:-0.2.126}"
    need_wasm_bindgen=1
    if [[ -x "$wasm_bindgen" ]]; then
      version="$("$wasm_bindgen" --version | awk '{print $2}')"
      if [[ "$version" == "$wasm_bindgen_version" ]]; then
        need_wasm_bindgen=0
      fi
    fi
    if [[ "$need_wasm_bindgen" == "1" ]]; then
      rustup run nightly cargo install \
        wasm-bindgen-cli \
        --version "$wasm_bindgen_version" \
        --locked \
        --force \
        --root "$tool_root"
    fi
    cargo_args=(-j 2)
    wasm_profile="debug"
    if [[ "{{ release }}" == "true" ]]; then
      cargo_args+=(--release)
      wasm_profile="release"
    fi
    target_dir="${CARGO_TARGET_DIR:-target}"
    rustup run nightly cargo build "${cargo_args[@]}" \
      -p web_client \
      --features lightyear-matchmaker \
      --bin lightrider-web \
      --target wasm32-unknown-unknown
    rm -rf web/pkg
    "$wasm_bindgen" \
      --target web \
      --out-dir web/pkg \
      "$target_dir/wasm32-unknown-unknown/$wasm_profile/lightrider-web.wasm"
    rm -rf web/assets
    mkdir -p web/assets
    cp -R assets/. web/assets/
    echo "Built web/pkg/lightrider-web.js"

web-serve bind="127.0.0.1" port="8000" release="false":
    #!/usr/bin/env bash
    set -euo pipefail
    just web-build release={{ release }}
    echo "Serving http://localhost:{{ port }}/"
    echo "For the local Lightyear Matchmaker stack, open:"
    echo "http://localhost:{{ port }}/?matchmaker_url=ws://127.0.0.1:3000/matchmaker/ws&matchmaker_game=lightrider&matchmaker_version=dev"
    python3 -m http.server "{{ port }}" --bind "{{ bind }}" --directory web
