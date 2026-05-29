#!/usr/bin/env bash
set -euo pipefail

pids=()

cleanup() {
  status=$?
  if [[ "$status" != "0" ]]; then
    echo "lightrider-matchmaker: entrypoint exiting with status $status" >&2
    for log_file in \
      /var/log/nats.log \
      /var/log/bevygap_matchmaker.log \
      /var/log/bevygap_matchmaker_httpd.log \
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

nats_port="${NATS_PORT:-4222}"
nats_monitor_port="${NATS_MONITOR_PORT:-8222}"
nats_user="${NATS_USER:-lightrider}"
nats_password="${NATS_PASSWORD:-lightrider}"
nats_store_dir="${NATS_STORE_DIR:-/data/nats}"
web_port="${WEB_PORT:-8080}"
httpd_port="${MATCHMAKER_HTTPD_PORT:-3000}"
bevygap_namespace="${BEVYGAP_NATS_NAMESPACE:-${EDGEGAP_APP_NAME:-lightrider}_${EDGEGAP_APP_VERSION:-dev}}"

if [[ -n "${NATS_TLS_CERT:-}" || -n "${NATS_TLS_KEY:-}" ]] && [[ -z "${NATS_TLS_CERT:-}" || -z "${NATS_TLS_KEY:-}" ]]; then
  echo "lightrider-matchmaker: set both NATS_TLS_CERT and NATS_TLS_KEY, or neither" >&2
  exit 1
fi

nats_tls_configured=0
if [[ -n "${NATS_TLS_CERT:-}" && -n "${NATS_TLS_KEY:-}" ]]; then
  nats_tls_configured=1
fi

production_nats_required=0
if truthy "${BEVYGAP_REQUIRE_SECURE_NATS:-}" || truthy "${LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE:-}"; then
  production_nats_required=1
fi

allow_insecure_nats=0
if truthy "${NATS_ALLOW_INSECURE:-}"; then
  allow_insecure_nats=1
fi

allow_dev_nats_credentials=0
if truthy "${BEVYGAP_ALLOW_DEV_NATS_CREDENTIALS:-}"; then
  allow_dev_nats_credentials=1
fi

if [[ "$production_nats_required" == "1" && "$allow_dev_nats_credentials" != "1" && "$nats_user" == "lightrider" && "$nats_password" == "lightrider" ]]; then
  echo "lightrider-matchmaker: production NATS setup refuses default credentials; set strong NATS_USER/NATS_PASSWORD or BEVYGAP_ALLOW_DEV_NATS_CREDENTIALS=1 for local-only testing" >&2
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

export NATS_HOST="${MATCHMAKER_NATS_HOST:-127.0.0.1:${nats_port}}"
export NATS_USER="$nats_user"
export NATS_PASSWORD="$nats_password"
export BEVYGAP_NATS_NAMESPACE="$bevygap_namespace"
if [[ "$nats_tls_configured" != "1" ]]; then
  export NATS_INSECURE=1
elif [[ -n "${MATCHMAKER_NATS_INSECURE:-}" ]]; then
  export NATS_INSECURE="$MATCHMAKER_NATS_INSECURE"
else
  unset NATS_INSECURE
fi

if [[ "$allow_insecure_nats" == "1" ]]; then
  export BEVYGAP_REQUIRE_SECURE_NATS=0
elif [[ "$production_nats_required" == "1" ]]; then
  export BEVYGAP_REQUIRE_SECURE_NATS=1
fi

matchmaker_args=(
  /app/bevygap_matchmaker
  --app-name "${EDGEGAP_APP_NAME:-lightrider}"
  --app-version "${EDGEGAP_APP_VERSION:-dev}"
  --lightyear-protocol-id "${LIGHTRIDER_PROTOCOL_ID:-0}"
  --max-players-per-deployment "${BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT:-800}"
  --max-rooms-per-deployment "${BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT:-16}"
  --max-cpu-percent-per-deployment "${BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT:-85}"
  --cert-digest-timeout-ms "${BEVYGAP_CERT_DIGEST_LOOKUP_TIMEOUT_MS:-15000}"
  --cert-digest-poll-ms "${BEVYGAP_CERT_DIGEST_LOOKUP_POLL_MS:-200}"
)

if [[ -n "${LIGHTRIDER_PRIVATE_KEY:-}" ]]; then
  matchmaker_args+=(--lightyear-private-key "$LIGHTRIDER_PRIVATE_KEY")
fi

if [[ "${BEVYGAP_MOCK_EDGEGAP:-0}" == "1" ]]; then
  matchmaker_args+=(
    --mock-edgegap
    --mock-public-ip "${BEVYGAP_MOCK_PUBLIC_IP:-127.0.0.1}"
    --mock-external-port "${BEVYGAP_MOCK_EXTERNAL_PORT:-7777}"
    --mock-deployment-request-id "${BEVYGAP_MOCK_DEPLOYMENT_REQUEST_ID:-local-lightrider}"
  )
fi

"${matchmaker_args[@]}" > /var/log/bevygap_matchmaker.log 2>&1 &
pids+=("$!")

/app/bevygap_matchmaker_httpd \
  --bind "127.0.0.1:${httpd_port}" \
  --cors "${MATCHMAKER_CORS:-http://localhost:${web_port}}" \
  --fake-ip "${MATCHMAKER_FAKE_IP:-81.128.157.100}" \
  > /var/log/bevygap_matchmaker_httpd.log 2>&1 &
pids+=("$!")

rm -f /etc/nginx/sites-enabled/default /etc/nginx/conf.d/default.conf
cat > /etc/nginx/conf.d/default.conf <<NGINX
server {
    listen ${web_port};
    server_name _;
    root /usr/share/nginx/html;
    index index.html;

    location /matchmaker/ {
        proxy_pass http://127.0.0.1:${httpd_port}/matchmaker/;
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
