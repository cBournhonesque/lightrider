static-default-host := "45.79.138.102"
static-default-env := "secrets/web-server.env"
static-default-ssh-port := "22"
static-default-service := "lightrider-matchmaker"
static-default-sslip-domain := "45.79.138.102.sslip.io"

# Internal helper: on the VPS, pull one already-pushed image, update one
# systemd service's image env line, and optionally restart only that service.
_remote-pull-service service image_env image_name *args:
    #!/usr/bin/env bash
    set -euo pipefail
    host="{{ static-default-host }}"
    tag="${TAG:-}"
    image=""
    ssh_port="{{ static-default-ssh-port }}"
    ssh_key=""
    env_file="{{ static-default-env }}"
    service_name=""
    route=""
    web_upstream_port=""
    caddy_default_upstream_port=""
    matchmaker_public_port=""
    nats_port=""
    nats_monitor_port=""
    manage_static_server=""
    run_static_server=""
    allocation_source=""
    restart="1"
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        host=*) host="${arg#host=}" ;;
        tag=*) tag="${arg#tag=}" ;;
        image=*) image="${arg#image=}" ;;
        ssh_port=*) ssh_port="${arg#ssh_port=}" ;;
        port=*) ssh_port="${arg#port=}" ;;
        ssh_key=*) ssh_key="${arg#ssh_key=}" ;;
        env=*) env_file="${arg#env=}" ;;
        service_name=*) service_name="${arg#service_name=}" ;;
        route=*) route="${arg#route=}" ;;
        matchmaker_route=*) route="${arg#matchmaker_route=}" ;;
        web_upstream_port=*) web_upstream_port="${arg#web_upstream_port=}" ;;
        caddy_default_upstream_port=*) caddy_default_upstream_port="${arg#caddy_default_upstream_port=}" ;;
        matchmaker_public_port=*) matchmaker_public_port="${arg#matchmaker_public_port=}" ;;
        nats_port=*) nats_port="${arg#nats_port=}" ;;
        nats_public_port=*) nats_port="${arg#nats_public_port=}" ;;
        nats_monitor_port=*) nats_monitor_port="${arg#nats_monitor_port=}" ;;
        nats_monitor_public_port=*) nats_monitor_port="${arg#nats_monitor_public_port=}" ;;
        manage_static_server=*) manage_static_server="${arg#manage_static_server=}" ;;
        run_static_server=*) run_static_server="${arg#run_static_server=}" ;;
        allocation_source=*) allocation_source="${arg#allocation_source=}" ;;
        restart=*) restart="${arg#restart=}" ;;
        *)
          case "$positional" in
            0) host="$arg" ;;
            1) tag="$arg" ;;
            2) ssh_port="$arg" ;;
            3) env_file="$arg" ;;
            4) ssh_key="$arg" ;;
            *)
              echo "unexpected extra argument for remote pull: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    if [[ -z "$host" ]]; then
      echo "usage: just control-host-pull-{game-server,matchmaker,webclient} host=<vps-ip-or-host> [tag=<tag>|image=<image>] [ssh_port=22] [env=secrets/web-server.env] [ssh_key=~/.ssh/key] [restart=1]" >&2
      exit 2
    fi
    if [[ -f "$env_file" ]]; then
      source "$env_file"
    fi
    if [[ -f secrets/edgegap.env ]]; then
      source secrets/edgegap.env
    fi
    image_env="{{ image_env }}"
    configured_image="${!image_env:-}"
    if [[ -z "$image" ]]; then
      if [[ -n "$configured_image" && -z "$tag" ]]; then
        image="$configured_image"
      else
        : "${EDGEGAP_REGISTRY_PROJECT:?EDGEGAP_REGISTRY_PROJECT is required to build the image name}"
        registry="${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}"
        tag="${tag:-${LIGHTRIDER_MATCHMAKER_TAG:-${EDGEGAP_APP_VERSION:-{{ edgegap-default-tag }}}}}"
        image="$registry/$EDGEGAP_REGISTRY_PROJECT/{{ image_name }}:$tag"
      fi
    fi
    ssh_key="${ssh_key/#\~\//$HOME/}"
    ssh_opts=(-p "$ssh_port")
    if [[ -n "$ssh_key" ]]; then
      test -f "$ssh_key" || {
        echo "SSH key not found: $ssh_key" >&2
        exit 1
      }
      ssh_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
    fi
    registry_url="${EDGEGAP_REGISTRY_URL:-}"
    registry_user="${EDGEGAP_REGISTRY_USERNAME:-}"
    registry_token="${EDGEGAP_REGISTRY_TOKEN:-}"
    ssh "${ssh_opts[@]}" "root@$host" \
      "REGISTRY_URL=$(printf '%q' "$registry_url") REGISTRY_USER=$(printf '%q' "$registry_user") REGISTRY_TOKEN=$(printf '%q' "$registry_token") IMAGE=$(printf '%q' "$image") IMAGE_ENV=$(printf '%q' "{{ image_env }}") SERVICE=$(printf '%q' "{{ service }}") RESTART=$(printf '%q' "$restart") bash -s" <<'REMOTE'
    set -euo pipefail
    if [[ -n "${REGISTRY_URL:-}" && -n "${REGISTRY_USER:-}" && -n "${REGISTRY_TOKEN:-}" ]]; then
      printf '%s' "$REGISTRY_TOKEN" | podman login "$REGISTRY_URL" --username "$REGISTRY_USER" --password-stdin
    fi
    podman pull "$IMAGE"
    service_file="/etc/systemd/system/${SERVICE}.service"
    if [[ -f "$service_file" ]]; then
      if grep -Fq "Environment=${IMAGE_ENV}=" "$service_file"; then
        sed -i "s#^Environment=${IMAGE_ENV}=.*#Environment=${IMAGE_ENV}=${IMAGE}#" "$service_file"
      else
        echo "service file $service_file does not define Environment=${IMAGE_ENV}=..." >&2
        exit 1
      fi
      systemctl daemon-reload
    fi
    if [[ "$SERVICE" == "lightrider-static-server" ]]; then
      help="$(podman run --rm --entrypoint /app/lightrider-server "$IMAGE" --help 2>&1 || true)"
      if [[ "$help" != *"--matchmaker"* ]]; then
        cat >&2 <<EOF
    Static server image does not look like the lightyear-matchmaker build:
      $IMAGE

    Expected '/app/lightrider-server --help' to contain '--matchmaker'.
    This usually means the VPS pulled an old Bevygap-era image/tag.

    Observed help output:
    $help
    EOF
        exit 1
      fi
    fi
    if [[ "$RESTART" == "1" || "$RESTART" == "true" || "$RESTART" == "yes" ]]; then
      systemctl restart "$SERVICE"
      systemctl --no-pager --full status "$SERVICE" || true
    fi
    REMOTE
    echo "Pulled $image on $host for {{ service }}"

# Does not install services or update web/matchmaker.
# Pull game-server image on the VPS and restart only `lightrider-static-server`.
control-host-pull-game-server *args:
    just _remote-pull-service lightrider-static-server LIGHTRIDER_STATIC_SERVER_IMAGE lightrider-server {{ args }}

# Does not install services or update web/game-server.
# Pull matchmaker image on the VPS and restart only `lightrider-matchmaker`.
control-host-pull-matchmaker *args:
    just _remote-pull-service lightrider-matchmaker LIGHTRIDER_MATCHMAKER_IMAGE lightrider-matchmaker {{ args }}

# Does not install services or update matchmaker/game-server.
# Pull web-client image on the VPS and restart only `lightrider-webclient`.
control-host-pull-webclient *args:
    just _remote-pull-service lightrider-webclient LIGHTRIDER_WEBCLIENT_IMAGE lightrider-webclient {{ args }}

# Does not regenerate env files or create missing systemd units.
# Pull all images on the VPS and restart existing services only.
control-host-pull-all *args:
    just control-host-pull-matchmaker {{ args }}
    just control-host-pull-webclient {{ args }}
    just control-host-pull-game-server {{ args }}

# Legacy name for `control-host-pull-game-server`.
static-server-pull-game-server *args:
    just control-host-pull-game-server {{ args }}

# Legacy name for `control-host-pull-matchmaker`.
static-server-pull-matchmaker *args:
    just control-host-pull-matchmaker {{ args }}

# Legacy name for `control-host-pull-webclient`.
static-server-pull-webclient *args:
    just control-host-pull-webclient {{ args }}

# Legacy name for `control-host-pull-all`.
static-server-pull-all *args:
    just control-host-pull-all {{ args }}

# Reuses the main deploy flow with SKIP_IMAGE_BUILD=1; parsing lives there.
# Full control-host install/update using already-pushed images.
control-host-deploy-pull *args:
    SKIP_IMAGE_BUILD=1 just control-host-deploy {{ args }}

# Legacy name for `control-host-deploy-pull`.
deploy-web-server-pull *args:
    just control-host-deploy-pull {{ args }}

# It resolves image tags, matchmaker/game versions, NATS settings, Caddy/web
# settings, static-server settings, and persistent netcode identity. The file
# is intentionally in secrets/ by default because it contains private keys.
# Lower-level: generate the local env file consumed by `web-server-install`.
web-server-env-template tag=edgegap-default-tag file=static-default-env host=static-default-host edgegap_version="":
    #!/usr/bin/env bash
    set -euo pipefail
    # Keep caller-provided deployment overrides distinct from values sourced out
    # of an older generated env file. The old file is useful for secrets, but it
    # must not pin a redeploy to an obsolete image tag such as "dev".
    caller_has_matchmaker_game=0
    caller_has_matchmaker_version=0
    caller_has_nats_namespace=0
    if [[ "${LIGHTRIDER_MATCHMAKER_GAME+x}" == "x" ]]; then
      caller_has_matchmaker_game=1
      caller_matchmaker_game="$LIGHTRIDER_MATCHMAKER_GAME"
    fi
    if [[ "${LIGHTRIDER_MATCHMAKER_VERSION+x}" == "x" ]]; then
      caller_has_matchmaker_version=1
      caller_matchmaker_version="$LIGHTRIDER_MATCHMAKER_VERSION"
    fi
    if [[ "${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE+x}" == "x" ]]; then
      caller_has_nats_namespace=1
      caller_nats_namespace="$LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE"
    fi
    caller_deploy_vars=(
      LIGHTRIDER_WEB_UPSTREAM_PORT
      LIGHTRIDER_CADDY_DEFAULT_UPSTREAM_PORT
      LIGHTRIDER_MATCHMAKER_ROUTE
      LIGHTRIDER_NATS_PUBLIC_PORT
      LIGHTRIDER_NATS_MONITOR_PUBLIC_PORT
      LIGHTRIDER_NATS_STORE_HOST_DIR
      LIGHTRIDER_MANAGE_STATIC_SERVER
      LIGHTRIDER_RUN_STATIC_SERVER
      LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE
      EDGEGAP_API_KEY
      EDGEGAP_API_TOKEN
      GAMEFLOW_GAME_ID
      GAMEFLOW_API_KEY
      GAMEFLOW_MODE
      GAMEFLOW_BUILD_ID
      GAMEFLOW_REGION
      GAMEFLOW_API_BASE_URL
      GAMEFLOW_API_KEY_ENV
      GAMEFLOW_API_KEY_PATH
    )
    for var in "${caller_deploy_vars[@]}"; do
      if [[ "${!var+x}" == "x" ]]; then
        printf -v "caller_deploy_has_$var" '%s' "1"
        printf -v "caller_deploy_value_$var" '%s' "${!var}"
      else
        printf -v "caller_deploy_has_$var" '%s' "0"
      fi
    done
    if [[ -f "{{ file }}" && "${FORCE:-0}" != "1" ]]; then
      echo "{{ file }} already exists; set FORCE=1 to overwrite" >&2
      exit 1
    fi
    if [[ -f "{{ file }}" ]]; then
      source "{{ file }}"
    fi
    if [[ -f secrets/edgegap.env ]]; then
      source secrets/edgegap.env
    fi
    if [[ -f secrets/prod-netcode.env ]]; then
      source secrets/prod-netcode.env
    fi
    if [[ "${FORCE:-0}" == "1" ]]; then
      unset LIGHTRIDER_MATCHMAKER_IMAGE
      unset LIGHTRIDER_STATIC_SERVER_IMAGE
      unset LIGHTRIDER_MATCHMAKER_TAG
      unset EDGEGAP_APP_VERSION
      if [[ "$caller_has_matchmaker_game" == "1" ]]; then
        LIGHTRIDER_MATCHMAKER_GAME="$caller_matchmaker_game"
      else
        unset LIGHTRIDER_MATCHMAKER_GAME
      fi
      if [[ "$caller_has_matchmaker_version" == "1" ]]; then
        LIGHTRIDER_MATCHMAKER_VERSION="$caller_matchmaker_version"
      else
        unset LIGHTRIDER_MATCHMAKER_VERSION
      fi
      if [[ "$caller_has_nats_namespace" == "1" ]]; then
        LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="$caller_nats_namespace"
      else
        unset LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE
      fi
      for var in "${caller_deploy_vars[@]}"; do
        caller_has_var="caller_deploy_has_$var"
        caller_value_var="caller_deploy_value_$var"
        if [[ "${!caller_has_var}" == "1" ]]; then
          printf -v "$var" '%s' "${!caller_value_var}"
        else
          unset "$var"
        fi
      done
    fi
    if [[ -z "${LIGHTRIDER_PROTOCOL_ID:-}" ]]; then
      LIGHTRIDER_PROTOCOL_ID="1"
    fi
    if [[ -z "${LIGHTRIDER_PRIVATE_KEY:-}" ]]; then
      LIGHTRIDER_PRIVATE_KEY="$(openssl rand -hex 32)"
    fi
    if [[ -z "${NATS_PASSWORD:-}" || "${NATS_PASSWORD:-}" == "lightrider" ]]; then
      NATS_PASSWORD="$(openssl rand -hex 24)"
    fi
    matchmaker_game="${LIGHTRIDER_MATCHMAKER_GAME:-${EDGEGAP_APP_NAME:-lightrider}}"
    matchmaker_version="${LIGHTRIDER_MATCHMAKER_VERSION:-${EDGEGAP_APP_VERSION:-{{ tag }}}}"
    edgegap_app_version="{{ edgegap_version }}"
    if [[ -z "$edgegap_app_version" ]]; then
      edgegap_app_version="${EDGEGAP_APP_VERSION:-$matchmaker_version}"
    fi
    if [[ -n "{{ edgegap_version }}" ]]; then
      LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE_OVERRIDE:-${matchmaker_game}_${matchmaker_version}}"
    fi
    web_domain="${LIGHTRIDER_WEB_DOMAIN:-}"
    web_domain="${web_domain#http://}"
    web_domain="${web_domain#https://}"
    web_domain="${web_domain%%/*}"
    enable_https="${LIGHTRIDER_ENABLE_HTTPS:-}"
    if [[ -z "$enable_https" && -n "$web_domain" ]]; then
      enable_https=1
    fi
    enable_https="${enable_https:-0}"
    if [[ "$enable_https" == "true" || "$enable_https" == "yes" || "$enable_https" == "on" ]]; then
      enable_https=1
    fi
    if [[ "$enable_https" == "1" && -z "$web_domain" ]]; then
      echo "LIGHTRIDER_WEB_DOMAIN is required when LIGHTRIDER_ENABLE_HTTPS=1" >&2
      exit 1
    fi
    if [[ "$enable_https" == "1" ]]; then
      default_matchmaker_cors="https://${web_domain}"
      default_matchmaker_url="wss://${web_domain}/matchmaker/ws"
    else
      default_matchmaker_cors="http://{{ host }}"
      default_matchmaker_url="ws://{{ host }}:${LIGHTRIDER_MATCHMAKER_PUBLIC_PORT:-3000}/ws"
    fi
    mkdir -p "$(dirname "{{ file }}")"
    write_env() {
      printf '%s=' "$1"
      printf '%q\n' "$2"
    }
    {
      echo "# Lightrider web-server/control-host install env."
      echo "# This file is shell-sourced locally, then converted to a container env file on the VPS."
      echo "# Keep LIGHTRIDER_PROTOCOL_ID/LIGHTRIDER_PRIVATE_KEY in sync with the game-server image."
      write_env LIGHTRIDER_MATCHMAKER_IMAGE "${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}/${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}/lightrider-matchmaker:{{ tag }}"
      write_env LIGHTRIDER_WEBCLIENT_IMAGE "${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}/${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}/lightrider-webclient:{{ tag }}"
      write_env LIGHTRIDER_MATCHMAKER_TAG "{{ tag }}"
      write_env LIGHTRIDER_MATCHMAKER_GAME "$matchmaker_game"
      write_env LIGHTRIDER_MATCHMAKER_VERSION "$matchmaker_version"
      write_env EDGEGAP_APP_NAME "${EDGEGAP_APP_NAME:-$matchmaker_game}"
      write_env EDGEGAP_APP_VERSION "$edgegap_app_version"
      write_env EDGEGAP_API_KEY "${EDGEGAP_API_KEY:-${EDGEGAP_API_TOKEN:-}}"
      write_env EDGEGAP_REGISTRY_URL "${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}"
      write_env EDGEGAP_REGISTRY_PROJECT "${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}"
      write_env EDGEGAP_REGISTRY_USERNAME "${EDGEGAP_REGISTRY_USERNAME:-}"
      write_env EDGEGAP_REGISTRY_TOKEN "${EDGEGAP_REGISTRY_TOKEN:-}"
      write_env LIGHTRIDER_PROTOCOL_ID "$LIGHTRIDER_PROTOCOL_ID"
      write_env LIGHTRIDER_PRIVATE_KEY "$LIGHTRIDER_PRIVATE_KEY"
      write_env LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE "1"
      write_env LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE "${LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE:-nats_static}"
      write_env LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE "${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE:-${matchmaker_game}_${matchmaker_version}}"
      write_env NATS_USER "${NATS_USER:-lightrider}"
      write_env NATS_PASSWORD "$NATS_PASSWORD"
      write_env NATS_ALLOW_INSECURE "${NATS_ALLOW_INSECURE:-1}"
      write_env LIGHTRIDER_ENABLE_HTTPS "$enable_https"
      write_env LIGHTRIDER_WEB_DOMAIN "$web_domain"
      write_env LIGHTRIDER_WEB_UPSTREAM_PORT "${LIGHTRIDER_WEB_UPSTREAM_PORT:-8080}"
      write_env LIGHTRIDER_CADDY_DEFAULT_UPSTREAM_PORT "${LIGHTRIDER_CADDY_DEFAULT_UPSTREAM_PORT:-8080}"
      write_env LIGHTRIDER_MATCHMAKER_PUBLIC_PORT "${LIGHTRIDER_MATCHMAKER_PUBLIC_PORT:-3000}"
      write_env LIGHTRIDER_MATCHMAKER_ROUTE "${LIGHTRIDER_MATCHMAKER_ROUTE:-}"
      write_env LIGHTRIDER_NATS_PUBLIC_PORT "${LIGHTRIDER_NATS_PUBLIC_PORT:-4222}"
      write_env LIGHTRIDER_NATS_MONITOR_PUBLIC_PORT "${LIGHTRIDER_NATS_MONITOR_PUBLIC_PORT:-8222}"
      if [[ -n "${LIGHTRIDER_NATS_STORE_HOST_DIR:-}" ]]; then write_env LIGHTRIDER_NATS_STORE_HOST_DIR "$LIGHTRIDER_NATS_STORE_HOST_DIR"; fi
      write_env LIGHTRIDER_CADDY_EMAIL "${LIGHTRIDER_CADDY_EMAIL:-}"
      write_env MATCHMAKER_CORS "${MATCHMAKER_CORS:-$default_matchmaker_cors}"
      write_env LIGHTRIDER_MATCHMAKER_URL "${LIGHTRIDER_MATCHMAKER_URL:-$default_matchmaker_url}"
      write_env LIGHTRIDER_RUN_STATIC_SERVER "${LIGHTRIDER_RUN_STATIC_SERVER:-1}"
      if [[ -n "${LIGHTRIDER_MANAGE_STATIC_SERVER:-}" ]]; then write_env LIGHTRIDER_MANAGE_STATIC_SERVER "$LIGHTRIDER_MANAGE_STATIC_SERVER"; fi
      write_env LIGHTRIDER_STATIC_SERVER_IMAGE "${LIGHTRIDER_STATIC_SERVER_IMAGE:-${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}/${EDGEGAP_REGISTRY_PROJECT:-lightyear-6qgcf4w4mrq7}/lightrider-server:{{ tag }}}"
      write_env LIGHTRIDER_STATIC_PUBLIC_IP "${LIGHTRIDER_STATIC_PUBLIC_IP:-{{ host }}}"
      write_env LIGHTRIDER_STATIC_PORT "${LIGHTRIDER_STATIC_PORT:-7777}"
      write_env LIGHTRIDER_STATIC_REQUEST_ID "${LIGHTRIDER_STATIC_REQUEST_ID:-linode-us-east-1}"
      write_env LIGHTRIDER_STATIC_COUNTRY_CODE "${LIGHTRIDER_STATIC_COUNTRY_CODE:-US}"
      write_env LIGHTRIDER_STATIC_REGION "${LIGHTRIDER_STATIC_REGION:-us-east}"
      if [[ -n "${NATS_TLS_CERT:-}" ]]; then write_env NATS_TLS_CERT "$NATS_TLS_CERT"; fi
      if [[ -n "${NATS_TLS_KEY:-}" ]]; then write_env NATS_TLS_KEY "$NATS_TLS_KEY"; fi
      if [[ -n "${NATS_CA:-}" ]]; then write_env NATS_CA "$NATS_CA"; fi
      if [[ -n "${MATCHMAKER_NATS_HOST:-}" ]]; then write_env MATCHMAKER_NATS_HOST "$MATCHMAKER_NATS_HOST"; fi
      if [[ -n "${MATCHMAKER_NATS_INSECURE:-}" ]]; then write_env MATCHMAKER_NATS_INSECURE "$MATCHMAKER_NATS_INSECURE"; fi
      if [[ -n "${GAMEFLOW_GAME_ID:-}" ]]; then write_env GAMEFLOW_GAME_ID "$GAMEFLOW_GAME_ID"; fi
      if [[ -n "${GAMEFLOW_API_KEY:-}" ]]; then write_env GAMEFLOW_API_KEY "$GAMEFLOW_API_KEY"; fi
      if [[ -n "${GAMEFLOW_MODE:-}" ]]; then write_env GAMEFLOW_MODE "$GAMEFLOW_MODE"; fi
      if [[ -n "${GAMEFLOW_BUILD_ID:-}" ]]; then write_env GAMEFLOW_BUILD_ID "$GAMEFLOW_BUILD_ID"; fi
      if [[ -n "${GAMEFLOW_REGION:-}" ]]; then write_env GAMEFLOW_REGION "$GAMEFLOW_REGION"; fi
      if [[ -n "${GAMEFLOW_API_BASE_URL:-}" ]]; then write_env GAMEFLOW_API_BASE_URL "$GAMEFLOW_API_BASE_URL"; fi
      if [[ -n "${GAMEFLOW_API_KEY_ENV:-}" ]]; then write_env GAMEFLOW_API_KEY_ENV "$GAMEFLOW_API_KEY_ENV"; fi
      if [[ -n "${GAMEFLOW_API_KEY_PATH:-}" ]]; then write_env GAMEFLOW_API_KEY_PATH "$GAMEFLOW_API_KEY_PATH"; fi
    } > "{{ file }}"
    chmod 600 "{{ file }}"
    echo "Wrote {{ file }}"

# This catches missing image names, unsafe default NATS credentials, invalid
# netcode identity, missing provider secrets, and HTTPS bootstrap mistakes.
# Lower-level: validate a generated deployment env before VPS upload.
web-server-env-check file=static-default-env:
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{ file }}" || {
      echo "{{ file }} does not exist. Run: just web-server-env-template" >&2
      exit 1
    }
    set -a
    source "{{ file }}"
    set +a
    require_var() {
      local name="$1"
      if [[ -z "${!name:-}" ]]; then
        echo "required deployment env is empty or missing: $name" >&2
        exit 1
      fi
    }
    truthy() {
      case "${1:-}" in
        1|true|TRUE|True|yes|YES|Yes|y|Y|on|ON|On) return 0 ;;
        *) return 1 ;;
      esac
    }

    require_var LIGHTRIDER_MATCHMAKER_IMAGE
    require_var LIGHTRIDER_WEBCLIENT_IMAGE
    require_var LIGHTRIDER_MATCHMAKER_TAG
    require_var LIGHTRIDER_PROTOCOL_ID
    require_var LIGHTRIDER_PRIVATE_KEY
    require_var LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE
    require_var LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE
    require_var NATS_USER
    require_var NATS_PASSWORD

    if [[ ! "$LIGHTRIDER_PROTOCOL_ID" =~ ^[0-9]+$ || "$LIGHTRIDER_PROTOCOL_ID" == "0" ]]; then
      echo "LIGHTRIDER_PROTOCOL_ID must be a nonzero integer" >&2
      exit 1
    fi
    if [[ "$NATS_USER" == "lightrider" && "$NATS_PASSWORD" == "lightrider" ]]; then
      echo "NATS_USER/NATS_PASSWORD still use local dev defaults; generate production credentials first" >&2
      exit 1
    fi
    if [[ "$LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE" == "edgegap" ]]; then
      require_var EDGEGAP_API_KEY
    fi
    if [[ "$LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE" == "gameflow" ]]; then
      require_var GAMEFLOW_GAME_ID
      require_var GAMEFLOW_API_KEY
    fi
    if [[ -n "${LIGHTRIDER_MATCHMAKER_ROUTE:-}" && ! "$LIGHTRIDER_MATCHMAKER_ROUTE" =~ ^[A-Za-z0-9_-]+$ ]]; then
      echo "LIGHTRIDER_MATCHMAKER_ROUTE may only contain letters, digits, '_' and '-'" >&2
      exit 1
    fi
    if truthy "${LIGHTRIDER_RUN_STATIC_SERVER:-1}"; then
      require_var LIGHTRIDER_STATIC_SERVER_IMAGE
      require_var LIGHTRIDER_STATIC_PUBLIC_IP
      require_var LIGHTRIDER_STATIC_PORT
    fi
    if truthy "${LIGHTRIDER_ENABLE_HTTPS:-0}"; then
      require_var LIGHTRIDER_WEB_DOMAIN
      require_var LIGHTRIDER_MATCHMAKER_URL
      if [[ "$LIGHTRIDER_MATCHMAKER_URL" != wss://* ]]; then
        echo "LIGHTRIDER_MATCHMAKER_URL should use wss:// when HTTPS is enabled" >&2
        exit 1
      fi
    fi
    if [[ "$LIGHTRIDER_MATCHMAKER_IMAGE" == registry.edgegap.com/* || "$LIGHTRIDER_WEBCLIENT_IMAGE" == registry.edgegap.com/* || "${LIGHTRIDER_STATIC_SERVER_IMAGE:-}" == registry.edgegap.com/* ]]; then
      require_var EDGEGAP_REGISTRY_USERNAME
      require_var EDGEGAP_REGISTRY_TOKEN
    fi

    echo "web-server env ok: {{ file }}"

# run the remote installer. The remote installer pulls images, writes systemd
# units, starts/restarts services, and removes the temporary uploaded env file.
# Lower-level: upload env+installer to the VPS and run remote setup.
web-server-install host=static-default-host ssh_port=static-default-ssh-port env=static-default-env ssh_key="" service_name=static-default-service:
    #!/usr/bin/env bash
    set -euo pipefail
    test -f "{{ env }}" || {
      echo "{{ env }} does not exist. Run: just web-server-env-template" >&2
      exit 1
    }
    just web-server-env-check "{{ env }}"
    ssh_key="{{ ssh_key }}"
    if [[ "$ssh_key" == "~/"* ]]; then
      ssh_key="${HOME}/${ssh_key#~/}"
    fi
    ssh_opts=(-p "{{ ssh_port }}")
    scp_opts=(-P "{{ ssh_port }}")
    if [[ -n "$ssh_key" ]]; then
      test -f "$ssh_key" || {
        echo "SSH key not found: $ssh_key" >&2
        exit 1
      }
      ssh_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
      scp_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
    fi
    remote_dir="/tmp/lightrider-web-server-setup"
    ssh "${ssh_opts[@]}" "root@{{ host }}" "mkdir -p '$remote_dir'"
    scp "${scp_opts[@]}" deploy/setup_web_server_host.sh "root@{{ host }}:$remote_dir/setup_web_server_host.sh"
    scp "${scp_opts[@]}" "{{ env }}" "root@{{ host }}:$remote_dir/web-server.env"
    ssh "${ssh_opts[@]}" "root@{{ host }}" "bash '$remote_dir/setup_web_server_host.sh' --env-file '$remote_dir/web-server.env' --service-name '{{ service_name }}'; rm -f '$remote_dir/web-server.env'"

# Lower-level: minimal post-deploy HTTP(S) check for the public web endpoint.
web-server-health host=static-default-host scheme="http":
    #!/usr/bin/env bash
    set -euo pipefail
    url="{{ scheme }}://{{ host }}/"
    curl -fsSL --max-time 30 "$url" >/dev/null
    echo "web ok: $url"

# Reuse Caddy's certificate for NATS TLS after HTTPS is already working.
# This updates matchmaker/static-server env files on the VPS and restarts
# services so game servers can connect to NATS with TLS.
web-server-enable-nats-tls-from-caddy *args:
    #!/usr/bin/env bash
    set -euo pipefail
    host="{{ static-default-host }}"
    domain="{{ static-default-sslip-domain }}"
    ssh_port="{{ static-default-ssh-port }}"
    ssh_key=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        host=*) host="${arg#host=}" ;;
        domain=*) domain="${arg#domain=}" ;;
        web_domain=*) domain="${arg#web_domain=}" ;;
        ssh_port=*) ssh_port="${arg#ssh_port=}" ;;
        port=*) ssh_port="${arg#port=}" ;;
        ssh_key=*) ssh_key="${arg#ssh_key=}" ;;
        *)
          case "$positional" in
            0) host="$arg" ;;
            1) domain="$arg" ;;
            2) ssh_port="$arg" ;;
            3) ssh_key="$arg" ;;
            *)
              echo "unexpected extra argument for web-server-enable-nats-tls-from-caddy: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    domain="${domain#http://}"
    domain="${domain#https://}"
    domain="${domain%%/*}"
    if [[ -z "$host" || -z "$domain" ]]; then
      echo "usage: just web-server-enable-nats-tls-from-caddy host=<vps-ip> domain=<domain> [ssh_key=~/.ssh/key] [ssh_port=22]" >&2
      exit 2
    fi
    if [[ "$ssh_key" == "~/"* ]]; then
      ssh_key="${HOME}/${ssh_key#~/}"
    fi
    ssh_opts=(-p "$ssh_port")
    if [[ -n "$ssh_key" ]]; then
      test -f "$ssh_key" || {
        echo "SSH key not found: $ssh_key" >&2
        exit 1
      }
      ssh_opts+=(-i "$ssh_key" -o IdentitiesOnly=yes)
    fi
    ssh "${ssh_opts[@]}" "root@$host" "LIGHTRIDER_WEB_DOMAIN=$(printf '%q' "$domain") bash -s" <<'REMOTE'
    set -euo pipefail
    domain="${LIGHTRIDER_WEB_DOMAIN:?LIGHTRIDER_WEB_DOMAIN is required}"
    cert_root="/var/lib/caddy/.local/share/caddy/certificates"
    env_file="/etc/lightrider/lightrider-matchmaker.env"
    static_env_file="/etc/lightrider/lightrider-static-server.env"
    cert_dest="/etc/lightrider/nats-cert.pem"
    key_dest="/etc/lightrider/nats-key.pem"

    curl -fsSL --max-time 30 "https://${domain}/" >/dev/null

    cert="$(find "$cert_root" -type f -name "${domain}.crt" | head -n 1 || true)"
    key="$(find "$cert_root" -type f -name "${domain}.key" | head -n 1 || true)"
    if [[ -z "$cert" || -z "$key" ]]; then
      echo "Could not find Caddy certificate/key for ${domain} under ${cert_root}" >&2
      echo "Check: journalctl -u caddy -n 120 --no-pager" >&2
      exit 1
    fi

    install -m 644 "$cert" "$cert_dest"
    install -m 600 "$key" "$key_dest"

    tmp="$(mktemp)"
    grep -v -E '^(NATS_ALLOW_INSECURE|NATS_TLS_CERT|NATS_TLS_KEY|MATCHMAKER_NATS_HOST|LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS)=' "$env_file" > "$tmp" || true
    cat "$tmp" > "$env_file"
    rm -f "$tmp"
    {
      printf 'NATS_ALLOW_INSECURE=0\n'
      printf 'NATS_TLS_CERT=%s\n' "$cert_dest"
      printf 'NATS_TLS_KEY=%s\n' "$key_dest"
      printf 'MATCHMAKER_NATS_HOST=%s:4222\n' "$domain"
      printf 'LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS=1\n'
    } >> "$env_file"
    chmod 600 "$env_file"

    if [[ -f "$static_env_file" ]]; then
      tmp="$(mktemp)"
      grep -v -E '^(NATS_HOST|NATS_INSECURE|LIGHTYEAR_MATCHMAKER_NATS_URL|LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS)=' "$static_env_file" > "$tmp" || true
      cat "$tmp" > "$static_env_file"
      rm -f "$tmp"
      {
        printf 'NATS_HOST=%s:4222\n' "$domain"
        printf 'LIGHTYEAR_MATCHMAKER_NATS_URL=tls://%s:4222\n' "$domain"
        printf 'LIGHTYEAR_MATCHMAKER_REQUIRE_SECURE_NATS=1\n'
      } >> "$static_env_file"
      chmod 600 "$static_env_file"
    fi

    systemctl restart lightrider-matchmaker
    if systemctl list-unit-files lightrider-static-server.service >/dev/null 2>&1; then
      systemctl restart lightrider-static-server || true
    fi
    sleep 2
    systemctl --no-pager --full status lightrider-matchmaker || true
    if systemctl list-unit-files lightrider-static-server.service >/dev/null 2>&1; then
      systemctl --no-pager --full status lightrider-static-server || true
    fi
    if command -v openssl >/dev/null 2>&1; then
      timeout 10 openssl s_client -connect "${domain}:4222" -servername "$domain" -verify_return_error </dev/null >/tmp/lightrider-nats-tls-check.txt 2>&1 || {
        cat /tmp/lightrider-nats-tls-check.txt >&2
        exit 1
      }
      echo "nats tls ok: ${domain}:4222"
    else
      echo "openssl not found; skipped external NATS TLS check"
    fi
    REMOTE

# generates the deploy env, installs/updates the VPS services, and runs a web
# health check. Pass SKIP_IMAGE_BUILD=1 or use `control-host-deploy-pull` when
# images are already pushed.
# Full control-host deploy flow: build/push, generate env, install, health-check.
control-host-deploy *args:
    #!/usr/bin/env bash
    set -euo pipefail
    vps_host="{{ static-default-host }}"
    ssh_port="{{ static-default-ssh-port }}"
    ssh_key=""
    tag="${TAG:-$(git rev-parse --short HEAD 2>/dev/null || date +%Y%m%d%H%M%S)}"
    edgegap_version=""
    web_domain=""
    enable_https=""
    env_file="{{ static-default-env }}"
    service_name="{{ static-default-service }}"
    route=""
    web_upstream_port=""
    caddy_default_upstream_port=""
    matchmaker_public_port=""
    nats_port=""
    nats_monitor_port=""
    manage_static_server=""
    run_static_server=""
    allocation_source=""
    build_memory=""
    build_memory_swap=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        host=*) vps_host="${arg#host=}" ;;
        ssh_port=*) ssh_port="${arg#ssh_port=}" ;;
        port=*) ssh_port="${arg#port=}" ;;
        ssh_key=*) ssh_key="${arg#ssh_key=}" ;;
        tag=*) tag="${arg#tag=}" ;;
        edgegap_version=*) edgegap_version="${arg#edgegap_version=}" ;;
        version=*) edgegap_version="${arg#version=}" ;;
        domain=*) web_domain="${arg#domain=}" ;;
        web_domain=*) web_domain="${arg#web_domain=}" ;;
        https=*) enable_https="${arg#https=}" ;;
        enable_https=*) enable_https="${arg#enable_https=}" ;;
        env=*) env_file="${arg#env=}" ;;
        service_name=*) service_name="${arg#service_name=}" ;;
        route=*) route="${arg#route=}" ;;
        matchmaker_route=*) route="${arg#matchmaker_route=}" ;;
        web_upstream_port=*) web_upstream_port="${arg#web_upstream_port=}" ;;
        caddy_default_upstream_port=*) caddy_default_upstream_port="${arg#caddy_default_upstream_port=}" ;;
        matchmaker_public_port=*) matchmaker_public_port="${arg#matchmaker_public_port=}" ;;
        nats_port=*) nats_port="${arg#nats_port=}" ;;
        nats_public_port=*) nats_port="${arg#nats_public_port=}" ;;
        nats_monitor_port=*) nats_monitor_port="${arg#nats_monitor_port=}" ;;
        nats_monitor_public_port=*) nats_monitor_port="${arg#nats_monitor_public_port=}" ;;
        manage_static_server=*) manage_static_server="${arg#manage_static_server=}" ;;
        run_static_server=*) run_static_server="${arg#run_static_server=}" ;;
        allocation_source=*) allocation_source="${arg#allocation_source=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
        memory_swap=*) build_memory_swap="${arg#memory_swap=}" ;;
        build_memory_swap=*) build_memory_swap="${arg#build_memory_swap=}" ;;
        cpus=*) build_cpus="${arg#cpus=}" ;;
        build_cpus=*) build_cpus="${arg#build_cpus=}" ;;
        cpu_quota=*) build_cpu_quota="${arg#cpu_quota=}" ;;
        build_cpu_quota=*) build_cpu_quota="${arg#build_cpu_quota=}" ;;
        cpuset_cpus=*) build_cpuset_cpus="${arg#cpuset_cpus=}" ;;
        build_cpuset_cpus=*) build_cpuset_cpus="${arg#build_cpuset_cpus=}" ;;
        *)
          case "$positional" in
            0) vps_host="$arg" ;;
            1) ssh_port="$arg" ;;
            2) tag="$arg" ;;
            3) env_file="$arg" ;;
            4) ssh_key="$arg" ;;
            5) edgegap_version="$arg" ;;
            6) web_domain="$arg" ;;
            *)
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    if [[ -z "$vps_host" ]]; then
      echo "usage: just control-host-deploy host=<vps-ip-or-host> [ssh_port=<port>] [ssh_key=<key>] [tag=<tag>] [edgegap_version=<version>] [domain=<domain>] [https=1] [env=<file>] [service_name=<name>] [route=<name>] [web_upstream_port=<port>] [nats_port=<port>]" >&2
      echo "example: just control-host-deploy host=45.79.138.102 domain=play.example.com" >&2
      exit 2
    fi
    if [[ -z "$enable_https" && -n "$web_domain" ]]; then
      enable_https=1
    fi
    enable_https="${enable_https:-0}"
    if [[ "${SKIP_IMAGE_BUILD:-0}" == "1" ]]; then
      echo "Skipping image build/push; assuming tag $tag is already pushed."
    else
      just prod-images-build-push "$tag" "$build_memory" memory_swap="$build_memory_swap" cpus="$build_cpus" cpu_quota="$build_cpu_quota" cpuset_cpus="$build_cpuset_cpus"
    fi
    if [[ -z "$web_upstream_port" ]]; then
      if [[ "$service_name" == "lightrider-matchmaker" ]]; then
        web_upstream_port=8080
      else
        echo "web_upstream_port is required for non-default service_name=$service_name" >&2
        exit 2
      fi
    fi
    caddy_default_upstream_port="${caddy_default_upstream_port:-8080}"
    matchmaker_public_port="${matchmaker_public_port:-3000}"
    nats_port="${nats_port:-4222}"
    nats_monitor_port="${nats_monitor_port:-8222}"
    env_args=(
      LIGHTRIDER_ENABLE_HTTPS="$enable_https"
      LIGHTRIDER_WEB_DOMAIN="$web_domain"
      LIGHTRIDER_WEB_UPSTREAM_PORT="$web_upstream_port"
      LIGHTRIDER_CADDY_DEFAULT_UPSTREAM_PORT="$caddy_default_upstream_port"
      LIGHTRIDER_MATCHMAKER_PUBLIC_PORT="$matchmaker_public_port"
      LIGHTRIDER_MATCHMAKER_ROUTE="$route"
      LIGHTRIDER_NATS_PUBLIC_PORT="$nats_port"
      LIGHTRIDER_NATS_MONITOR_PUBLIC_PORT="$nats_monitor_port"
      FORCE=1
    )
    if [[ -n "$manage_static_server" ]]; then
      env_args+=(LIGHTRIDER_MANAGE_STATIC_SERVER="$manage_static_server")
    fi
    if [[ -n "$run_static_server" ]]; then
      env_args+=(LIGHTRIDER_RUN_STATIC_SERVER="$run_static_server")
    fi
    if [[ -n "$allocation_source" ]]; then
      env_args+=(LIGHTYEAR_MATCHMAKER_ALLOCATION_SOURCE="$allocation_source")
    fi
    env "${env_args[@]}" just web-server-env-template "$tag" "$env_file" "$vps_host" "$edgegap_version"
    just web-server-env-check "$env_file"
    just web-server-install "$vps_host" "$ssh_port" "$env_file" "$ssh_key" "$service_name"
    if [[ "$enable_https" == "1" || "$enable_https" == "true" || "$enable_https" == "yes" ]]; then
      just web-server-health "$web_domain" https
    else
      just web-server-health "$vps_host" http
    fi

# Legacy name for `control-host-deploy`.
deploy-web-server *args:
    just control-host-deploy {{ args }}
