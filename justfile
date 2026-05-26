set shell := ["bash", "-cu"]

server config="config/test.ron" port="5000":
    cargo run -j 4 -p server --bin lightrider-server -- --headless --port {{port}} --config {{config}}

client id="1" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto":
    cargo run -j 4 -p client --bin lightrider-client -- --client-id {{id}} --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}}

bot id="1001" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto":
    cargo run -j 4 -p client --bin lightrider-client -- --headless --mode bot --client-id {{id}} --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}}

bots count="4" first_id="1001" config="config/test.ron" server_addr="127.0.0.1" port="5000" room="auto":
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'jobs -pr | xargs -r kill' EXIT
    for i in $(seq 0 $(({{count}} - 1))); do
      cargo run -j 4 -p client --bin lightrider-client -- --headless --mode bot --client-id $(({{first_id}} + i)) --server-addr {{server_addr}} --server-port {{port}} --config {{config}} --room {{room}} &
      sleep 1
    done
    wait

local bots="4" config="config/test.ron" port="5000" client_id="1" first_bot_id="1001" room="auto":
    #!/usr/bin/env bash
    set -euo pipefail
    trap 'jobs -pr | xargs -r kill' EXIT
    cargo run -j 4 -p server --bin lightrider-server -- --headless --port {{port}} --config {{config}} &
    sleep 2
    for i in $(seq 0 $(({{bots}} - 1))); do
      cargo run -j 4 -p client --bin lightrider-client -- --headless --mode bot --client-id $(({{first_bot_id}} + i)) --server-port {{port}} --config {{config}} --room {{room}} &
      sleep 1
    done
    cargo run -j 4 -p client --bin lightrider-client -- --client-id {{client_id}} --server-port {{port}} --config {{config}} --room {{room}}

trace-local clients="4" seconds="20" config="config/test.ron" port="5000" first_client_id="2001" room="auto":
    #!/usr/bin/env bash
    set -euo pipefail
    run_dir="logs/debug/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$run_dir"
    ln -sfn "$(basename "$run_dir")" logs/debug/latest
    cargo build -j 4 -p server --bin lightrider-server -p client --bin lightrider-client
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
