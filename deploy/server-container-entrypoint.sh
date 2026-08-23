#!/usr/bin/env bash
set -euo pipefail

first_non_empty_env() {
  local name value
  for name in "$@"; do
    value="${!name:-}"
    if [[ -n "$value" ]]; then
      printf '%s\n' "$value"
      return 0
    fi
  done
  return 1
}

is_gameflow_runtime=0
if [[ "${LIGHTRIDER_MATCHMAKER_PROVIDER:-}" == "gameflow" || -n "${GAMEFLOW_DEFAULT_PORT:-}" ]]; then
  is_gameflow_runtime=1
fi

port="${PORT:-${GAMEFLOW_DEFAULT_PORT:-7777}}"
config="${LIGHTRIDER_CONFIG:-/app/config/default.ron}"
matchmaker="${LIGHTRIDER_MATCHMAKER:-1}"

if [[ "$is_gameflow_runtime" == "1" && -z "${LIGHTRIDER_SERVER_ID:-}" ]]; then
  if server_id="$(first_non_empty_env \
    GAMEFLOW_SERVER_ID \
    GAMEFLOW_ALLOCATION_ID \
    GAMEFLOW_SESSION_ID \
    GAMEFLOW_INSTANCE_ID \
    GAME_SERVER_ID \
    SERVER_ID \
    POD_NAME \
    HOSTNAME)"; then
    export LIGHTRIDER_SERVER_ID="$server_id"
  fi
fi

if [[ "$is_gameflow_runtime" == "1" && -z "${LIGHTRIDER_PUBLIC_PORT:-}" ]]; then
  if public_port="$(first_non_empty_env GAMEFLOW_DEFAULT_PORT GAMEFLOW_GAME_PORT PORT)"; then
    export LIGHTRIDER_PUBLIC_PORT="$public_port"
  fi
fi

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
