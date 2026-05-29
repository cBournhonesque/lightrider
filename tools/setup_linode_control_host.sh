#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  setup_linode_control_host.sh --env-file /path/to/linode-control-host.env [options]

Installs and starts the Lightrider matchmaker/control container on a Debian
Linode host. The container bundles NATS, the web server, the WASM client,
bevygap_matchmaker_httpd, and bevygap_matchmaker.

Options:
  --env-file PATH       Shell env file containing Edgegap, registry, NATS, and netcode settings.
  --service-name NAME   systemd service/container name. Default: lightrider-matchmaker
  --no-install          Do not apt-install host packages.
  --no-pull             Do not podman pull the image.
  --no-start            Install files but do not start/restart the service.
  -h, --help            Show this help.

Required env:
  EDGEGAP_API_KEY or EDGEGAP_API_TOKEN
  LIGHTRIDER_PROTOCOL_ID
  LIGHTRIDER_PRIVATE_KEY
  NATS_USER
  NATS_PASSWORD
  LIGHTRIDER_MATCHMAKER_IMAGE or EDGEGAP_REGISTRY_URL/PROJECT plus LIGHTRIDER_MATCHMAKER_TAG

Useful optional env:
  EDGEGAP_APP_NAME=lightrider
  EDGEGAP_APP_VERSION=<edgegap app version>
  MATCHMAKER_CORS=http://<public-ip-or-domain>
  NATS_ALLOW_INSECURE=1
  BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT=800
  BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT=16
  BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT=85
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

EDGEGAP_APP_NAME="${EDGEGAP_APP_NAME:-lightrider}"
EDGEGAP_APP_VERSION="${EDGEGAP_APP_VERSION:-${LIGHTRIDER_MATCHMAKER_TAG:-dev}}"
LIGHTRIDER_MATCHMAKER_TAG="${LIGHTRIDER_MATCHMAKER_TAG:-$EDGEGAP_APP_VERSION}"
NATS_ALLOW_INSECURE="${NATS_ALLOW_INSECURE:-1}"
BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT="${BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT:-800}"
BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT="${BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT:-16}"
BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT="${BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT:-85}"

if [[ -z "${MATCHMAKER_CORS:-}" ]]; then
  if command -v curl >/dev/null 2>&1; then
    public_ip="$(curl -fsS --max-time 5 https://ifconfig.me 2>/dev/null || hostname -I | awk '{print $1}')"
  else
    public_ip="$(hostname -I | awk '{print $1}')"
  fi
  MATCHMAKER_CORS="http://${public_ip}"
fi

if [[ -z "${LIGHTRIDER_MATCHMAKER_IMAGE:-}" ]]; then
  : "${EDGEGAP_REGISTRY_URL:?EDGEGAP_REGISTRY_URL is required when LIGHTRIDER_MATCHMAKER_IMAGE is unset}"
  : "${EDGEGAP_REGISTRY_PROJECT:?EDGEGAP_REGISTRY_PROJECT is required when LIGHTRIDER_MATCHMAKER_IMAGE is unset}"
  LIGHTRIDER_MATCHMAKER_IMAGE="${EDGEGAP_REGISTRY_URL}/${EDGEGAP_REGISTRY_PROJECT}/lightrider-matchmaker:${LIGHTRIDER_MATCHMAKER_TAG}"
fi

required_vars=(
  EDGEGAP_API_KEY
  LIGHTRIDER_PROTOCOL_ID
  LIGHTRIDER_PRIVATE_KEY
  NATS_USER
  NATS_PASSWORD
  LIGHTRIDER_MATCHMAKER_IMAGE
)

for var in "${required_vars[@]}"; do
  if [[ -z "${!var:-}" ]]; then
    echo "required variable is empty: $var" >&2
    exit 1
  fi
done

if [[ "$install_packages" == 1 ]]; then
  export DEBIAN_FRONTEND=noninteractive
  apt-get update
  apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    iproute2 \
    podman
fi

if [[ -n "${EDGEGAP_REGISTRY_URL:-}" && -n "${EDGEGAP_REGISTRY_USERNAME:-}" && -n "${EDGEGAP_REGISTRY_TOKEN:-}" ]]; then
  printf '%s' "$EDGEGAP_REGISTRY_TOKEN" | podman login "$EDGEGAP_REGISTRY_URL" \
    --username "$EDGEGAP_REGISTRY_USERNAME" \
    --password-stdin
fi

if [[ "$pull_image" == 1 ]]; then
  podman pull "$LIGHTRIDER_MATCHMAKER_IMAGE"
fi

install -d -m 700 /etc/lightrider
install -d -m 755 /var/lib/lightrider/nats

runtime_env="/etc/lightrider/${service_name}.env"
cat > "$runtime_env" <<EOF
EDGEGAP_API_KEY=$EDGEGAP_API_KEY
EDGEGAP_APP_NAME=$EDGEGAP_APP_NAME
EDGEGAP_APP_VERSION=$EDGEGAP_APP_VERSION
LIGHTRIDER_PROTOCOL_ID=$LIGHTRIDER_PROTOCOL_ID
LIGHTRIDER_PRIVATE_KEY=$LIGHTRIDER_PRIVATE_KEY
LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE=${LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE:-1}
NATS_ALLOW_INSECURE=$NATS_ALLOW_INSECURE
NATS_USER=$NATS_USER
NATS_PASSWORD=$NATS_PASSWORD
NATS_STORE_DIR=/data/nats
MATCHMAKER_CORS=$MATCHMAKER_CORS
MATCHMAKER_FAKE_IP=${MATCHMAKER_FAKE_IP:-81.128.157.100}
BEVYGAP_NATS_NAMESPACE=${BEVYGAP_NATS_NAMESPACE:-${EDGEGAP_APP_NAME}_${EDGEGAP_APP_VERSION}}
BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT=$BEVYGAP_MAX_PLAYERS_PER_DEPLOYMENT
BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT=$BEVYGAP_MAX_ROOMS_PER_DEPLOYMENT
BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT=$BEVYGAP_MAX_CPU_PERCENT_PER_DEPLOYMENT
EOF
chmod 600 "$runtime_env"

service_file="/etc/systemd/system/${service_name}.service"
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
ExecStart=/usr/bin/podman run --rm --name $service_name \\
  --env-file $runtime_env \\
  -p 80:8080 \\
  -p 4222:4222 \\
  -p 127.0.0.1:8222:8222 \\
  -v /var/lib/lightrider/nats:/data/nats \\
  \${LIGHTRIDER_MATCHMAKER_IMAGE}
ExecStop=/usr/bin/podman stop -t 20 $service_name

[Install]
WantedBy=multi-user.target
EOF

systemctl daemon-reload
systemctl enable "$service_name"

if [[ "$start_service" == 1 ]]; then
  systemctl restart "$service_name"
  sleep 2
  systemctl --no-pager --full status "$service_name" || true
  echo
  echo "Local health checks:"
  curl -fsS http://127.0.0.1/ >/dev/null && echo "  web: ok" || echo "  web: not ready"
  curl -fsS http://127.0.0.1:8222/healthz >/dev/null && echo "  nats: ok" || echo "  nats: not ready"
fi

cat <<EOF

Lightrider control host setup complete.

Service:
  systemctl status $service_name --no-pager
  journalctl -u $service_name -f

Container logs:
  podman logs $service_name
  podman exec $service_name tail -n 200 /var/log/nats.log
  podman exec $service_name tail -n 200 /var/log/bevygap_matchmaker.log
  podman exec $service_name tail -n 200 /var/log/bevygap_matchmaker_httpd.log

Published host ports:
  80/tcp    web client + /matchmaker/ws
  4222/tcp  NATS for Edgegap game servers
  8222/tcp  NATS monitoring bound to localhost only

Matchmaker image:
  $LIGHTRIDER_MATCHMAKER_IMAGE
EOF
