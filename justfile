set shell := ["bash", "-cu"]

edgegap-default-tag := `git rev-parse --short HEAD 2>/dev/null || date +%Y%m%d%H%M%S`

export CARGO_BUILD_JOBS := "2"
export CARGO_INCREMENTAL := "1"

import "deploy/local.just"
import "deploy/edgegap.just"
import "deploy/static.just"
import "deploy/gameflow.just"

clean-build:
    cargo clean

clean-incremental:
    rm -rf target/debug/incremental target/*/debug/incremental

netcode-secret protocol_id="":
    #!/usr/bin/env bash
    set -euo pipefail
    protocol_id="{{ protocol_id }}"
    if [[ -z "$protocol_id" ]]; then
      protocol_id="$(od -An -N8 -tu8 /dev/urandom | tr -d ' ')"
    fi
    private_key="$(openssl rand -hex 32)"
    printf 'LIGHTRIDER_PROTOCOL_ID=%s\n' "$protocol_id"
    printf 'LIGHTRIDER_PRIVATE_KEY=%s\n' "$private_key"
    printf 'LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=1\n'
