#!/usr/bin/env bash
set -euo pipefail

port="${PORT:-7777}"
config="${LIGHTRIDER_CONFIG:-/app/config/default.ron}"
bevygap="${LIGHTRIDER_BEVYGAP:-1}"

if [[ -z "${BEVYGAP_NATS_NAMESPACE:-}" ]]; then
  export BEVYGAP_NATS_NAMESPACE="${EDGEGAP_APP_NAME:-lightrider}_${EDGEGAP_APP_VERSION:-dev}"
fi

if [[ $# -gt 0 ]]; then
  exec /app/lightrider-server "$@"
fi

args=(--headless --port "$port" --config "$config")
if [[ "$bevygap" == "1" || "$bevygap" == "true" || "$bevygap" == "yes" ]]; then
  args+=(--bevygap)
fi

exec /app/lightrider-server "${args[@]}"
