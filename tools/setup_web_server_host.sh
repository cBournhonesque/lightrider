#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  setup_web_server_host.sh --env-file /path/to/web-server.env [options]

Installs and starts the Lightrider matchmaker/control container on a Debian
VPS host. The container bundles NATS, nginx, the WASM client, and
lightyear_matchmaker_server.

Options:
  --env-file PATH       Shell env file containing Edgegap, registry, NATS, and netcode settings.
  --service-name NAME   systemd service/container name. Default: lightrider-matchmaker
  --no-install          Do not apt-install host packages.
  --no-pull             Do not podman pull the image.
  --no-start            Install files but do not start/restart the service.
  -h, --help            Show this help.

Required env:
  LIGHTRIDER_PROTOCOL_ID
  LIGHTRIDER_PRIVATE_KEY
  NATS_USER
  NATS_PASSWORD
  LIGHTRIDER_MATCHMAKER_IMAGE or EDGEGAP_REGISTRY_URL/PROJECT plus LIGHTRIDER_MATCHMAKER_TAG
  EDGEGAP_API_KEY or EDGEGAP_API_TOKEN when LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE=edgegap

Useful optional env:
  EDGEGAP_APP_NAME=lightrider
  EDGEGAP_APP_VERSION=<edgegap app version>
  LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE=nats_static
  LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE=lightrider_<version>
  LIGHTRIDER_ENABLE_HTTPS=1
  LIGHTRIDER_WEB_DOMAIN=play.example.com
  LIGHTRIDER_CADDY_EMAIL=admin@example.com
  MATCHMAKER_CORS=http://<public-ip-or-domain>
  LIGHTRIDER_MATCHMAKER_URL=wss://<public-domain>/matchmaker/ws
  NATS_ALLOW_INSECURE=1
  NATS_TLS_CERT=/etc/lightrider/nats-cert.pem
  NATS_TLS_KEY=/etc/lightrider/nats-key.pem
  NATS_CA=/etc/lightrider/nats-ca.pem
  LIGHTRIDER_RUN_STATIC_SERVER=1
  LIGHTRIDER_STATIC_SERVER_IMAGE=<registry>/<project>/lightrider-server:<tag>
  LIGHTRIDER_STATIC_PUBLIC_IP=<public-ip>
  LIGHTRIDER_STATIC_PORT=7777
  LIGHTRIDER_STATIC_REQUEST_ID=linode-us-east-1
EOF
}

env_file=""
service_name="${LIGHTRIDER_CONTROL_SERVICE_NAME:-lightrider-matchmaker}"
install_packages=1
pull_image=1
start_service=1

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env-file)
      env_file="${2:-}"
      shift 2
      ;;
    --service-name)
      service_name="${2:-}"
      shift 2
      ;;
    --no-install)
      install_packages=0
      shift
      ;;
    --no-pull)
      pull_image=0
      shift
      ;;
    --no-start)
      start_service=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$env_file" ]]; then
  echo "--env-file is required" >&2
  usage >&2
  exit 2
fi

if [[ ! -f "$env_file" ]]; then
  echo "env file does not exist: $env_file" >&2
  exit 1
fi

if [[ "${EUID:-$(id -u)}" -ne 0 ]]; then
  sudo_args=(sudo -E bash "$0" --env-file "$env_file" --service-name "$service_name")
  [[ "$install_packages" == 0 ]] && sudo_args+=(--no-install)
  [[ "$pull_image" == 0 ]] && sudo_args+=(--no-pull)
  [[ "$start_service" == 0 ]] && sudo_args+=(--no-start)
  exec "${sudo_args[@]}"
fi

set -a
# shellcheck disable=SC1090
source "$env_file"
set +a

if [[ -z "${EDGEGAP_API_KEY:-}" && -n "${EDGEGAP_API_TOKEN:-}" ]]; then
  EDGEGAP_API_KEY="$EDGEGAP_API_TOKEN"
fi

LIGHTRIDER_MATCHMAKER_TAG="${LIGHTRIDER_MATCHMAKER_TAG:-${LIGHTRIDER_MATCHMAKER_VERSION:-${EDGEGAP_APP_VERSION:-dev}}}"
LIGHTRIDER_MATCHMAKER_GAME="${LIGHTRIDER_MATCHMAKER_GAME:-${EDGEGAP_APP_NAME:-lightrider}}"
LIGHTRIDER_MATCHMAKER_VERSION="${LIGHTRIDER_MATCHMAKER_VERSION:-${EDGEGAP_APP_VERSION:-$LIGHTRIDER_MATCHMAKER_TAG}}"
EDGEGAP_APP_NAME="${EDGEGAP_APP_NAME:-$LIGHTRIDER_MATCHMAKER_GAME}"
EDGEGAP_APP_VERSION="${EDGEGAP_APP_VERSION:-$LIGHTRIDER_MATCHMAKER_VERSION}"
LIGHTRIDER_RUN_STATIC_SERVER="${LIGHTRIDER_RUN_STATIC_SERVER:-0}"
NATS_ALLOW_INSECURE="${NATS_ALLOW_INSECURE:-1}"
LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE="${LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE:-nats_static}"
LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-${LIGHTRIDER_MATCHMAKER_GAME}_${LIGHTRIDER_MATCHMAKER_VERSION}}"
LIGHTRIDER_STATIC_PORT="${LIGHTRIDER_STATIC_PORT:-7777}"
LIGHTRIDER_STATIC_REQUEST_ID="${LIGHTRIDER_STATIC_REQUEST_ID:-linode-us-east-1}"
LIGHTRIDER_STATIC_COUNTRY_CODE="${LIGHTRIDER_STATIC_COUNTRY_CODE:-US}"
LIGHTRIDER_STATIC_REGION="${LIGHTRIDER_STATIC_REGION:-us-east}"
LIGHTRIDER_WEB_DOMAIN="${LIGHTRIDER_WEB_DOMAIN:-}"
LIGHTRIDER_WEB_DOMAIN="${LIGHTRIDER_WEB_DOMAIN#http://}"
LIGHTRIDER_WEB_DOMAIN="${LIGHTRIDER_WEB_DOMAIN#https://}"
LIGHTRIDER_WEB_DOMAIN="${LIGHTRIDER_WEB_DOMAIN%%/*}"
LIGHTRIDER_ENABLE_HTTPS="${LIGHTRIDER_ENABLE_HTTPS:-}"
if [[ -z "$LIGHTRIDER_ENABLE_HTTPS" && -n "$LIGHTRIDER_WEB_DOMAIN" ]]; then
  LIGHTRIDER_ENABLE_HTTPS=1
fi
LIGHTRIDER_ENABLE_HTTPS="${LIGHTRIDER_ENABLE_HTTPS:-0}"
LIGHTRIDER_WEB_UPSTREAM_PORT="${LIGHTRIDER_WEB_UPSTREAM_PORT:-8080}"
LIGHTRIDER_WEB_PUBLIC_PORT="${LIGHTRIDER_WEB_PUBLIC_PORT:-80}"

if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" || "$LIGHTRIDER_ENABLE_HTTPS" == "true" || "$LIGHTRIDER_ENABLE_HTTPS" == "yes" ]]; then
  LIGHTRIDER_ENABLE_HTTPS=1
else
  LIGHTRIDER_ENABLE_HTTPS=0
fi

if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" && -z "$LIGHTRIDER_WEB_DOMAIN" ]]; then
  echo "LIGHTRIDER_WEB_DOMAIN is required when LIGHTRIDER_ENABLE_HTTPS=1" >&2
  exit 1
fi

if [[ -z "${MATCHMAKER_CORS:-}" ]]; then
  if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
    MATCHMAKER_CORS="https://${LIGHTRIDER_WEB_DOMAIN}"
  else
    if command -v curl >/dev/null 2>&1; then
      public_ip="$(curl -fsS --max-time 5 https://ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}')"
    else
      public_ip="$(hostname -I | awk '{print $1}')"
    fi
    MATCHMAKER_CORS="http://${public_ip}"
  fi
fi
if [[ -z "${LIGHTRIDER_MATCHMAKER_URL:-}" && "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
  LIGHTRIDER_MATCHMAKER_URL="wss://${LIGHTRIDER_WEB_DOMAIN}/matchmaker/ws"
fi
LIGHTRIDER_STATIC_PUBLIC_IP="${LIGHTRIDER_STATIC_PUBLIC_IP:-${public_ip:-$(hostname -I | awk '{print $1}')}}"

if [[ -z "${LIGHTRIDER_MATCHMAKER_IMAGE:-}" ]]; then
  : "${EDGEGAP_REGISTRY_URL:?EDGEGAP_REGISTRY_URL is required when LIGHTRIDER_MATCHMAKER_IMAGE is unset}"
  : "${EDGEGAP_REGISTRY_PROJECT:?EDGEGAP_REGISTRY_PROJECT is required when LIGHTRIDER_MATCHMAKER_IMAGE is unset}"
  LIGHTRIDER_MATCHMAKER_IMAGE="${EDGEGAP_REGISTRY_URL}/${EDGEGAP_REGISTRY_PROJECT}/lightrider-matchmaker:${LIGHTRIDER_MATCHMAKER_TAG}"
fi
if [[ -z "${LIGHTRIDER_STATIC_SERVER_IMAGE:-}" ]]; then
  : "${EDGEGAP_REGISTRY_URL:?EDGEGAP_REGISTRY_URL is required when LIGHTRIDER_STATIC_SERVER_IMAGE is unset}"
  : "${EDGEGAP_REGISTRY_PROJECT:?EDGEGAP_REGISTRY_PROJECT is required when LIGHTRIDER_STATIC_SERVER_IMAGE is unset}"
  LIGHTRIDER_STATIC_SERVER_IMAGE="${EDGEGAP_REGISTRY_URL}/${EDGEGAP_REGISTRY_PROJECT}/lightrider-server:${LIGHTRIDER_MATCHMAKER_TAG}"
fi

required_vars=(
  LIGHTRIDER_PROTOCOL_ID
  LIGHTRIDER_PRIVATE_KEY
  NATS_USER
  NATS_PASSWORD
  LIGHTRIDER_MATCHMAKER_IMAGE
)
if [[ "$LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE" == "edgegap" ]]; then
  required_vars+=(EDGEGAP_API_KEY)
fi

for var in "${required_vars[@]}"; do
  if [[ -z "${!var:-}" ]]; then
    echo "required variable is empty: $var" >&2
    exit 1
  fi
done

env_file_value() {
  local file="$1"
  local key="$2"
  awk -F= -v key="$key" '
    $1 == key {
      sub(/^[^=]*=/, "")
      print
      found = 1
      exit
    }
    END {
      if (!found) {
        exit 1
      }
    }
  ' "$file"
}

require_env_file_value() {
  local file="$1"
  local key="$2"
  local value
  value="$(env_file_value "$file" "$key" || true)"
  if [[ -z "$value" ]]; then
    echo "generated env file is missing required key $key: $file" >&2
    exit 1
  fi
}

if [[ "$install_packages" == 1 ]]; then
  export DEBIAN_FRONTEND=noninteractive
  packages=(
    ca-certificates \
    curl \
    iproute2 \
    podman
  )
  if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
    packages+=(caddy)
  fi
  apt-get update
  apt-get install -y --no-install-recommends "${packages[@]}"
fi

if [[ -n "${EDGEGAP_REGISTRY_URL:-}" && -n "${EDGEGAP_REGISTRY_USERNAME:-}" && -n "${EDGEGAP_REGISTRY_TOKEN:-}" ]]; then
  printf '%s' "$EDGEGAP_REGISTRY_TOKEN" | podman login "$EDGEGAP_REGISTRY_URL" \
    --username "$EDGEGAP_REGISTRY_USERNAME" \
    --password-stdin
fi

if [[ "$pull_image" == 1 ]]; then
  podman pull "$LIGHTRIDER_MATCHMAKER_IMAGE"
  if [[ "$LIGHTRIDER_RUN_STATIC_SERVER" == "1" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "true" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "yes" ]]; then
    podman pull "$LIGHTRIDER_STATIC_SERVER_IMAGE"
  fi
fi

install -d -m 700 /etc/lightrider
install -d -m 755 /var/lib/lightrider/nats

runtime_env="/etc/lightrider/${service_name}.env"
cat > "$runtime_env" <<EOF
EDGEGAP_API_KEY=${EDGEGAP_API_KEY:-}
EDGEGAP_APP_NAME=$EDGEGAP_APP_NAME
EDGEGAP_APP_VERSION=$EDGEGAP_APP_VERSION
LIGHTRIDER_MATCHMAKER_GAME=$LIGHTRIDER_MATCHMAKER_GAME
LIGHTRIDER_MATCHMAKER_VERSION=$LIGHTRIDER_MATCHMAKER_VERSION
LIGHTRIDER_PROTOCOL_ID=$LIGHTRIDER_PROTOCOL_ID
LIGHTRIDER_PRIVATE_KEY=$LIGHTRIDER_PRIVATE_KEY
LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=${LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE:-1}
LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE=$LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE
LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE=$LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE
NATS_ALLOW_INSECURE=$NATS_ALLOW_INSECURE
NATS_USER=$NATS_USER
NATS_PASSWORD=$NATS_PASSWORD
NATS_STORE_DIR=/data/nats
MATCHMAKER_CORS=$MATCHMAKER_CORS
LIGHTRIDER_MATCHMAKER_URL=${LIGHTRIDER_MATCHMAKER_URL:-}
EOF
for optional_var in \
  NATS_TLS_CERT \
  NATS_TLS_KEY \
  NATS_CA \
  NATS_CA_CONTENTS \
  MATCHMAKER_NATS_HOST \
  MATCHMAKER_NATS_INSECURE \
  LIGHTYEAR_MATCHMAKER_ASSIGNMENT_PREPARE_TIMEOUT_MS \
  LIGHTYEAR_MATCHMAKER_ASSIGNMENT_PREPARE_POLL_MS \
  LIGHTYEAR_MATCHMAKER_EDGEGAP_READY_TIMEOUT_SECS \
  LIGHTYEAR_MATCHMAKER_EDGEGAP_POLL_MS; do
  if [[ -n "${!optional_var:-}" ]]; then
    printf '%s=%s\n' "$optional_var" "${!optional_var}" >> "$runtime_env"
  fi
done
chmod 600 "$runtime_env"
for required_runtime_key in \
  LIGHTRIDER_MATCHMAKER_GAME \
  LIGHTRIDER_MATCHMAKER_VERSION \
  LIGHTRIDER_PROTOCOL_ID \
  LIGHTRIDER_PRIVATE_KEY \
  LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE \
  LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE \
  NATS_USER \
  NATS_PASSWORD; do
  require_env_file_value "$runtime_env" "$required_runtime_key"
done

static_runtime_env="/etc/lightrider/lightrider-static-server.env"
if [[ "$LIGHTRIDER_RUN_STATIC_SERVER" == "1" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "true" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "yes" ]]; then
  if [[ -n "${NATS_TLS_CERT:-}" && -n "${NATS_TLS_KEY:-}" ]]; then
    static_nats_host="${MATCHMAKER_NATS_HOST:-${LIGHTRIDER_WEB_DOMAIN}:4222}"
    static_nats_insecure=""
    static_require_secure_nats="1"
  else
    static_nats_host="127.0.0.1:4222"
    static_nats_insecure="1"
    static_require_secure_nats="0"
  fi
  cat > "$static_runtime_env" <<EOF
PORT=$LIGHTRIDER_STATIC_PORT
LIGHTRIDER_CONFIG=${LIGHTRIDER_CONFIG:-/app/config/default.ron}
LIGHTRIDER_MATCHMAKER=1
LIGHTRIDER_MATCHMAKER_GAME=$LIGHTRIDER_MATCHMAKER_GAME
LIGHTRIDER_MATCHMAKER_VERSION=$LIGHTRIDER_MATCHMAKER_VERSION
LIGHTRIDER_PROTOCOL_ID=$LIGHTRIDER_PROTOCOL_ID
LIGHTRIDER_PRIVATE_KEY=$LIGHTRIDER_PRIVATE_KEY
LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=${LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE:-1}
NATS_HOST=$static_nats_host
NATS_USER=$NATS_USER
NATS_PASSWORD=$NATS_PASSWORD
LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE=$LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE
LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS=$static_require_secure_nats
LIGHTRIDER_SERVER_ID=$LIGHTRIDER_STATIC_REQUEST_ID
LIGHTRIDER_MATCHMAKER_PROVIDER=static
LIGHTRIDER_MATCHMAKER_COUNTRY=$LIGHTRIDER_STATIC_COUNTRY_CODE
LIGHTRIDER_MATCHMAKER_REGION=$LIGHTRIDER_STATIC_REGION
LIGHTRIDER_PUBLIC_IP=$LIGHTRIDER_STATIC_PUBLIC_IP
LIGHTRIDER_PUBLIC_PORT=$LIGHTRIDER_STATIC_PORT
SELF_SIGNED_SANS=$LIGHTRIDER_STATIC_PUBLIC_IP,localhost,127.0.0.1
EOF
  if [[ -n "$static_nats_insecure" ]]; then
    echo "NATS_INSECURE=$static_nats_insecure" >> "$static_runtime_env"
  else
    echo "LIGHTYEAR_MATCHMAKER_NATS_URL=tls://$static_nats_host" >> "$static_runtime_env"
  fi
  chmod 600 "$static_runtime_env"
  for required_static_key in \
    LIGHTRIDER_MATCHMAKER_GAME \
    LIGHTRIDER_MATCHMAKER_VERSION \
    LIGHTRIDER_PROTOCOL_ID \
    LIGHTRIDER_PRIVATE_KEY \
    LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE \
    NATS_HOST \
    NATS_USER \
    NATS_PASSWORD \
    LIGHTRIDER_PUBLIC_IP \
    LIGHTRIDER_PUBLIC_PORT; do
    require_env_file_value "$static_runtime_env" "$required_static_key"
  done
fi

service_file="/etc/systemd/system/${service_name}.service"
web_publish="-p ${LIGHTRIDER_WEB_PUBLIC_PORT}:8080"
if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
  web_publish="-p 127.0.0.1:${LIGHTRIDER_WEB_UPSTREAM_PORT}:8080"
fi
cat > "$service_file" <<EOF
[Unit]
Description=Lightrider matchmaker/control host
Wants=network-online.target
After=network-online.target

[Service]
Environment=LIGHTRIDER_MATCHMAKER_IMAGE=$LIGHTRIDER_MATCHMAKER_IMAGE
EnvironmentFile=$runtime_env
Restart=always
RestartSec=5
TimeoutStopSec=30
ExecStartPre=-/usr/bin/podman rm -f $service_name
ExecStart=/usr/bin/podman run --name $service_name \\
  --env-file $runtime_env \\
  $web_publish \\
  -p 4222:4222 \\
  -p 127.0.0.1:8222:8222 \\
  -v /var/lib/lightrider/nats:/data/nats \\
  -v /etc/lightrider:/etc/lightrider:ro \\
  \${LIGHTRIDER_MATCHMAKER_IMAGE}
ExecStop=/usr/bin/podman stop -t 20 $service_name

[Install]
WantedBy=multi-user.target
EOF

static_service_name="lightrider-static-server"
static_service_file="/etc/systemd/system/${static_service_name}.service"
if [[ "$LIGHTRIDER_RUN_STATIC_SERVER" == "1" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "true" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "yes" ]]; then
  cat > "$static_service_file" <<EOF
[Unit]
Description=Lightrider static game server
Wants=network-online.target ${service_name}.service
After=network-online.target ${service_name}.service

[Service]
Environment=LIGHTRIDER_STATIC_SERVER_IMAGE=$LIGHTRIDER_STATIC_SERVER_IMAGE
EnvironmentFile=$static_runtime_env
Restart=always
RestartSec=5
TimeoutStopSec=30
ExecStartPre=-/usr/bin/podman rm -f $static_service_name
ExecStart=/usr/bin/podman run --name $static_service_name \\
  --network host \\
  --env-file $static_runtime_env \\
  \${LIGHTRIDER_STATIC_SERVER_IMAGE}
ExecStop=/usr/bin/podman stop -t 20 $static_service_name

[Install]
WantedBy=multi-user.target
EOF
else
  systemctl disable "$static_service_name" >/dev/null 2>&1 || true
  rm -f "$static_service_file"
fi

if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
  caddy_global=""
  if [[ -n "${LIGHTRIDER_CADDY_EMAIL:-}" ]]; then
    caddy_global="{
    email ${LIGHTRIDER_CADDY_EMAIL}
}

"
  fi
  cat > /etc/caddy/Caddyfile <<EOF
${caddy_global}${LIGHTRIDER_WEB_DOMAIN} {
    encode zstd gzip
    reverse_proxy 127.0.0.1:${LIGHTRIDER_WEB_UPSTREAM_PORT}
}
EOF
  systemctl enable caddy
fi

systemctl daemon-reload
systemctl enable "$service_name"
if [[ "$LIGHTRIDER_RUN_STATIC_SERVER" == "1" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "true" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "yes" ]]; then
  systemctl enable "$static_service_name"
fi

if [[ "$start_service" == 1 ]]; then
  systemctl restart "$service_name"
  if [[ "$LIGHTRIDER_RUN_STATIC_SERVER" == "1" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "true" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "yes" ]]; then
    systemctl restart "$static_service_name"
  fi
  if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
    systemctl restart caddy
  fi
  sleep 2
  systemctl --no-pager --full status "$service_name" || true
  if [[ "$LIGHTRIDER_RUN_STATIC_SERVER" == "1" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "true" || "$LIGHTRIDER_RUN_STATIC_SERVER" == "yes" ]]; then
    systemctl --no-pager --full status "$static_service_name" || true
  fi
  echo
  echo "Local health checks:"
  web_ready=0
  https_ready=1
  nats_ready=0
  if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
    curl -fsS "http://127.0.0.1:${LIGHTRIDER_WEB_UPSTREAM_PORT}/" >/dev/null && web_ready=1
    curl -fsS --max-time 20 "https://${LIGHTRIDER_WEB_DOMAIN}/" >/dev/null || https_ready=0
  else
    curl -fsS "http://127.0.0.1:${LIGHTRIDER_WEB_PUBLIC_PORT}/" >/dev/null && web_ready=1
  fi
  curl -fsS http://127.0.0.1:8222/healthz >/dev/null && nats_ready=1
  [[ "$web_ready" == 1 ]] && echo "  web upstream: ok" || echo "  web upstream: not ready"
  if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
    [[ "$https_ready" == 1 ]] && echo "  https: ok" || echo "  https: not ready"
  fi
  [[ "$nats_ready" == 1 ]] && echo "  nats: ok" || echo "  nats: not ready"
  if [[ "$web_ready" != 1 || "$https_ready" != 1 || "$nats_ready" != 1 ]]; then
    echo
    echo "Recent service logs:"
    journalctl -u "$service_name" -n 120 --no-pager || true
    if [[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]]; then
      journalctl -u caddy -n 120 --no-pager || true
    fi
  fi
fi

cat <<EOF

Lightrider control host setup complete.

Service:
  systemctl status $service_name --no-pager
  journalctl -u $service_name -f

Container logs:
  podman logs $service_name
  podman exec $service_name tail -n 200 /var/log/nats.log
  podman exec $service_name tail -n 200 /var/log/lightyear_matchmaker_server.log

Published host ports:
  $([[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]] && echo "80/tcp, 443/tcp  Caddy HTTPS web client + /matchmaker/ws" || echo "${LIGHTRIDER_WEB_PUBLIC_PORT}/tcp    web client + /matchmaker/ws")
  4222/tcp  NATS for game servers
  8222/tcp  NATS monitoring bound to localhost only
  $LIGHTRIDER_STATIC_PORT/udp  optional static Lightrider game server

Public web URL:
  $([[ "$LIGHTRIDER_ENABLE_HTTPS" == "1" ]] && echo "https://${LIGHTRIDER_WEB_DOMAIN}/" || echo "http://${LIGHTRIDER_STATIC_PUBLIC_IP}:${LIGHTRIDER_WEB_PUBLIC_PORT}/")

Matchmaker image:
  $LIGHTRIDER_MATCHMAKER_IMAGE
Static server image:
  $LIGHTRIDER_STATIC_SERVER_IMAGE
EOF
