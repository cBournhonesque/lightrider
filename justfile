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
    done
    cargo run -j 4 -p client --bin lightrider-client -- --client-id {{client_id}} --server-port {{port}} --config {{config}} --room {{room}}
