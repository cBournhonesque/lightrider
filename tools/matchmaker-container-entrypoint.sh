#!/usr/bin/env bash
set -euo pipefail

pids=()

cleanup() {
  status=$?
  if [[ "$status" != "0" ]]; then
    echo "lightrider-matchmaker: entrypoint exiting with status $status" >&2
    for log_file in \
      /var/log/nats.log \
      /var/log/lightyear_matchmaker_server.log \
      /var/log/nginx/error.log; do
      if [[ -s "$log_file" ]]; then
        echo "==> $log_file <==" >&2
        tail -n 200 "$log_file" >&2 || true
      fi
    done
  fi
  for pid in "${pids[@]}"; do
    kill "$pid" 2>/dev/null || true
  done
  wait 2>/dev/null || true
}
trap cleanup EXIT INT TERM

if [[ -z "${EDGEGAP_API_KEY:-}" && -n "${EDGEGAP_API_TOKEN:-}" ]]; then
  export EDGEGAP_API_KEY="$EDGEGAP_API_TOKEN"
fi

truthy() {
  case "${1:-}" in
    1|true|TRUE|True|yes|YES|Yes|y|Y|on|ON|On) return 0 ;;
    *) return 1 ;;
  esac
}

js_string() {
  local value="${1:-}"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  printf '"%s"' "$value"
}

toml_string() {
  local value="${1:-}"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  printf '"%s"' "$value"
}

nats_port="${NATS_PORT:-4222}"
nats_monitor_port="${NATS_MONITOR_PORT:-8222}"
nats_user="${NATS_USER:-lightrider}"
nats_password="${NATS_PASSWORD:-lightrider}"
nats_store_dir="${NATS_STORE_DIR:-/data/nats}"
web_port="${WEB_PORT:-8080}"
matchmaker_port="${MATCHMAKER_PORT:-3000}"
app_name="${EDGEGAP_APP_NAME:-lightrider}"
app_version="${EDGEGAP_APP_VERSION:-dev}"
namespace="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-${MATCHMAKER_NATS_NAMESPACE:-${app_name}_${app_version}}}"
allocation_source="${LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE:-${MATCHMAKER_ALLOCATION_SOURCE:-nats_static}}"

if [[ -n "${NATS_TLS_CERT:-}" || -n "${NATS_TLS_KEY:-}" ]] && [[ -z "${NATS_TLS_CERT:-}" || -z "${NATS_TLS_KEY:-}" ]]; then
  echo "lightrider-matchmaker: set both NATS_TLS_CERT and NATS_TLS_KEY, or neither" >&2
  exit 1
fi

nats_tls_configured=0
if [[ -n "${NATS_TLS_CERT:-}" && -n "${NATS_TLS_KEY:-}" ]]; then
  nats_tls_configured=1
fi

production_nats_required=0
if truthy "${LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS:-}" || truthy "${LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE:-}"; then
  production_nats_required=1
fi

allow_insecure_nats=0
if truthy "${NATS_ALLOW_INSECURE:-}"; then
  allow_insecure_nats=1
fi

allow_dev_nats_credentials=0
if truthy "${LIGHTYEAR_MATCHMAKER_ALLOW_DEV_NATS_CREDENTIALS:-}"; then
  allow_dev_nats_credentials=1
fi

if [[ "$production_nats_required" == "1" && "$allow_dev_nats_credentials" != "1" && "$nats_user" == "lightrider" && "$nats_password" == "lightrider" ]]; then
  echo "lightrider-matchmaker: production NATS setup refuses default credentials; set strong NATS_USER/NATS_PASSWORD or LIGHTYEAR_MATCHMAKER_ALLOW_DEV_NATS_CREDENTIALS=1 for local-only testing" >&2
  exit 1
fi

if [[ "$production_nats_required" == "1" && "$nats_tls_configured" != "1" && "$allow_insecure_nats" != "1" ]]; then
  echo "lightrider-matchmaker: production NATS setup requires NATS_TLS_CERT/NATS_TLS_KEY; set NATS_ALLOW_INSECURE=1 only for temporary testing" >&2
  exit 1
fi

mkdir -p "$nats_store_dir" /run/nginx /var/log/nginx

nats_args=(
  /app/nats-server
  -js
  -sd "$nats_store_dir"
  -p "$nats_port"
  -m "$nats_monitor_port"
  --user "$nats_user"
  --pass "$nats_password"
)

if [[ "$nats_tls_configured" == "1" ]]; then
  nats_args+=(--tls --tlscert "$NATS_TLS_CERT" --tlskey "$NATS_TLS_KEY")
fi

"${nats_args[@]}" > /var/log/nats.log 2>&1 &
pids+=("$!")

for _ in $(seq 1 80); do
  if curl -fsS "http://127.0.0.1:${nats_monitor_port}/healthz" >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:${nats_monitor_port}/healthz" >/dev/null

nats_host="${MATCHMAKER_NATS_HOST:-127.0.0.1:${nats_port}}"
if [[ "$nats_host" == *"://"* ]]; then
  nats_url="$nats_host"
else
  nats_scheme="nats"
  if [[ "$nats_tls_configured" == "1" ]]; then
    nats_scheme="tls"
  fi
  nats_url="${nats_scheme}://${nats_host}"
fi

matchmaker_config="/run/lightrider-matchmaker.toml"
cat > "$matchmaker_config" <<EOF
[server]
bind = "127.0.0.1:${matchmaker_port}"

[game]
name = $(toml_string "$app_name")
version = $(toml_string "$app_version")

[identity]
trust_forwarded_for = true

[allocation]
source = $(toml_string "$allocation_source")
require_assignment_prepare = true
assignment_prepare_timeout_ms = ${LIGHTYEAR_MATCHMAKER_ASSIGNMENT_PREPARE_TIMEOUT_MS:-30000}
assignment_prepare_poll_ms = ${LIGHTYEAR_MATCHMAKER_ASSIGNMENT_PREPARE_POLL_MS:-100}

[nats]
url = $(toml_string "$nats_url")
username = $(toml_string "$nats_user")
password = $(toml_string "$nats_password")
namespace = $(toml_string "$namespace")

[nats.ttl]
server_readiness_secs = ${LIGHTYEAR_MATCHMAKER_SERVER_READINESS_TTL_SECS:-30}
server_capacity_secs = ${LIGHTYEAR_MATCHMAKER_SERVER_CAPACITY_TTL_SECS:-30}
assignments_secs = ${LIGHTYEAR_MATCHMAKER_ASSIGNMENTS_TTL_SECS:-60}
assignments_prepared_secs = ${LIGHTYEAR_MATCHMAKER_ASSIGNMENTS_PREPARED_TTL_SECS:-60}
active_connections_secs = ${LIGHTYEAR_MATCHMAKER_ACTIVE_CONNECTIONS_TTL_SECS:-30}
lifecycle_work_secs = ${LIGHTYEAR_MATCHMAKER_LIFECYCLE_WORK_TTL_SECS:-600}

[lightyear]
protocol_id = ${LIGHTRIDER_PROTOCOL_ID:-0}
private_key = $(toml_string "${LIGHTRIDER_PRIVATE_KEY:-}")
client_timeout_secs = ${LIGHTYEAR_MATCHMAKER_CLIENT_TIMEOUT_SECS:-15}
token_expire_secs = ${LIGHTYEAR_MATCHMAKER_TOKEN_EXPIRE_SECS:-30}

[edgegap_provider]
app = $(toml_string "$app_name")
version = $(toml_string "$app_version")
api_key_env = "EDGEGAP_API_KEY"
base_url = $(toml_string "${EDGEGAP_API_BASE_URL:-https://api.edgegap.com}")
port_name = $(toml_string "${EDGEGAP_GAME_PORT_NAME:-game}")
session_ready_timeout_secs = ${LIGHTYEAR_MATCHMAKER_EDGEGAP_READY_TIMEOUT_SECS:-120}
session_poll_ms = ${LIGHTYEAR_MATCHMAKER_EDGEGAP_POLL_MS:-500}
release_missing_ok = true
EOF

/app/lightyear_matchmaker_server --config "$matchmaker_config" \
  > /var/log/lightyear_matchmaker_server.log 2>&1 &
pids+=("$!")

for _ in $(seq 1 80); do
  if curl -fsS "http://127.0.0.1:${matchmaker_port}/health" >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
curl -fsS "http://127.0.0.1:${matchmaker_port}/health" >/dev/null

rm -f /etc/nginx/sites-enabled/default /etc/nginx/conf.d/default.conf
cat > /usr/share/nginx/html/bootstrap.js <<BOOTSTRAP
window.LIGHTRIDER_BOOTSTRAP = {
  matchmaker_url: $(js_string "${LIGHTRIDER_MATCHMAKER_URL:-}"),
  matchmaker_game: $(js_string "$app_name"),
  matchmaker_version: $(js_string "$app_version")
};
BOOTSTRAP

cat > /etc/nginx/conf.d/default.conf <<NGINX
server {
    listen ${web_port};
    server_name _;
    root /usr/share/nginx/html;
    index index.html;

    location /matchmaker/ {
        proxy_pass http://127.0.0.1:${matchmaker_port}/;
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host \$host;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_read_timeout 300s;
    }

    location / {
        try_files \$uri \$uri/ /index.html;
    }
}
NGINX

nginx -g "daemon off;"
