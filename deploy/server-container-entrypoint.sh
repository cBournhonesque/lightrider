#!/usr/bin/env bash
set -euo pipefail

port="${PORT:-7777}"
config="${LIGHTRIDER_CONFIG:-/app/config/default.ron}"
matchmaker="${LIGHTRIDER_MATCHMAKER:-1}"

if [[ -z "${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-}" ]]; then
  export LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="${EDGEGAP_APP_NAME:-lightrider}_${EDGEGAP_APP_VERSION:-dev}"
fi

if [[ $# -gt 0 ]]; then
  exec /app/lightrider-server "$@"
fi

args=(--headless --port "$port" --config "$config")
if [[ "$matchmaker" == "1" || "$matchmaker" == "true" || "$matchmaker" == "yes" ]]; then
  args+=(--matchmaker)
fi

exec /app/lightrider-server "${args[@]}"
