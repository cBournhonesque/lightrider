#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  tools/edgegap_app_version.sh desired --tag <image-tag> --version <edgegap-version> [--app lightrider]
  tools/edgegap_app_version.sh show --version <edgegap-version> [--app lightrider]
  tools/edgegap_app_version.sh diff --tag <image-tag> --version <edgegap-version> [--app lightrider]
  tools/edgegap_app_version.sh sync --tag <image-tag> --version <edgegap-version> [--app lightrider] [--create-app]
  tools/edgegap_app_version.sh verify --tag <image-tag> --version <edgegap-version> [--app lightrider]

Commands:
  desired   Print the redacted desired app-version shape. Does not call Edgegap.
  show      Fetch and print the current Edgegap app version, redacted.
  diff      Fetch current state and show a redacted diff against desired state.
  sync      Create or update the Edgegap app version. This is the only mutating command.
  verify    Fetch current state and fail if it does not match desired deploy-critical fields.

Configuration:
  The script automatically sources secrets/edgegap.env and secrets/prod-netcode.env if present.
  Required for Edgegap API commands: EDGEGAP_API_KEY, EDGEGAP_API_TOKEN, or EDGEGAP_TOKEN.
  Required for desired/diff/sync/verify:
    EDGEGAP_REGISTRY_URL, EDGEGAP_REGISTRY_PROJECT,
    NATS_HOST, NATS_USER, NATS_PASSWORD,
    LIGHTRIDER_PROTOCOL_ID, LIGHTRIDER_PRIVATE_KEY.
  Optional desired env:
    BEVYGAP_NATS_NAMESPACE, BEVYGAP_SESSION_MAPPING_TTL_MS,
    BEVYGAP_UNCLAIMED_SESSION_TTL_SECS, BEVYGAP_ACTIVE_CONNECTION_TTL_SECS,
    BEVYGAP_CERT_DIGEST_TTL_SECS.

  Authorization is sent exactly as provided, so keep the "token " prefix in the secret value.
EOF
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for env_file in secrets/edgegap.env secrets/prod-netcode.env secrets/nats.env; do
  if [[ -f "$env_file" ]]; then
    # shellcheck disable=SC1090
    source "$env_file"
  fi
done

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "edgegap-app-version: missing required command '$1'" >&2
    exit 1
  fi
}

truthy() {
  case "${1:-}" in
    1|true|TRUE|True|yes|YES|Yes|y|Y|on|ON|On) return 0 ;;
    *) return 1 ;;
  esac
}

urlencode() {
  jq -nr --arg value "$1" '$value | @uri'
}

require_var() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    echo "edgegap-app-version: missing required env $name" >&2
    exit 1
  fi
}

command="${1:-}"
if [[ -z "$command" || "$command" == "-h" || "$command" == "--help" ]]; then
  usage
  exit 0
fi
shift

app="${EDGEGAP_APP_NAME:-lightrider}"
version="${EDGEGAP_APP_VERSION:-}"
tag="${EDGEGAP_IMAGE_TAG:-}"
create_app=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)
      app="${2:?missing value for --app}"
      shift 2
      ;;
    --version)
      version="${2:?missing value for --version}"
      shift 2
      ;;
    --tag)
      tag="${2:?missing value for --tag}"
      shift 2
      ;;
    --create-app)
      create_app=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "edgegap-app-version: unknown argument '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

case "$command" in
  desired|show|diff|sync|verify) ;;
  *)
    echo "edgegap-app-version: unknown command '$command'" >&2
    usage >&2
    exit 1
    ;;
esac

require_cmd curl
require_cmd jq
require_cmd diff
require_cmd sha256sum

if [[ "$command" != "show" && -z "$tag" ]]; then
  echo "edgegap-app-version: --tag is required for $command" >&2
  exit 1
fi
if [[ -z "$version" ]]; then
  echo "edgegap-app-version: --version is required" >&2
  exit 1
fi

api_base="${EDGEGAP_API_BASE_URL:-https://api.edgegap.com}"
api_base="${api_base%/}"
api_token="${EDGEGAP_API_KEY:-${EDGEGAP_API_TOKEN:-${EDGEGAP_TOKEN:-}}}"

require_api() {
  if [[ -z "$api_token" ]]; then
    echo "edgegap-app-version: set EDGEGAP_API_KEY, EDGEGAP_API_TOKEN, or EDGEGAP_TOKEN" >&2
    exit 1
  fi
}

require_desired_env() {
  EDGEGAP_REGISTRY_URL="${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}"
  export EDGEGAP_REGISTRY_URL
  require_var EDGEGAP_REGISTRY_PROJECT
  require_var NATS_HOST
  require_var NATS_USER
  require_var NATS_PASSWORD
  require_var LIGHTRIDER_PROTOCOL_ID
  require_var LIGHTRIDER_PRIVATE_KEY
}

api_request() {
  local method="$1"
  local path="$2"
  local body_file="${3:-}"
  local output_file="$4"
  local curl_args=(
    -sS
    -o "$output_file"
    -w "%{http_code}"
    -X "$method"
    -H "Authorization: $api_token"
    -H "Accept: application/json"
  )
  if [[ -n "$body_file" ]]; then
    curl_args+=(-H "Content-Type: application/json" --data-binary "@$body_file")
  fi
  curl "${curl_args[@]}" "$api_base$path"
}

build_desired_payload() {
  local mode="${1:-create}"
  local nats_insecure=""
  local secure_nats="${BEVYGAP_REQUIRE_SECURE_NATS:-1}"
  if truthy "${EDGEGAP_NATS_INSECURE:-${NATS_INSECURE:-}}"; then
    nats_insecure="1"
    secure_nats="0"
  fi

  local include_registry_credentials=1
  if [[ "${EDGEGAP_INCLUDE_REGISTRY_CREDENTIALS:-1}" == "0" ]]; then
    include_registry_credentials=0
  fi
  local bevygap_nats_namespace="${BEVYGAP_NATS_NAMESPACE:-${app}_${version}}"

  local private_username=""
  local private_token=""
  if [[ "$include_registry_credentials" == "1" ]]; then
    private_username="${EDGEGAP_REGISTRY_USERNAME:-}"
    private_token="${EDGEGAP_REGISTRY_TOKEN:-}"
  fi

  jq -n \
    --arg payload_mode "$mode" \
    --arg name "$version" \
    --arg repository "${EDGEGAP_REGISTRY_URL:-registry.edgegap.com}" \
    --arg image "${EDGEGAP_REGISTRY_PROJECT}/lightrider-server" \
    --arg tag "$tag" \
    --arg private_username "$private_username" \
    --arg private_token "$private_token" \
    --arg req_cpu "${EDGEGAP_REQ_CPU:-1024}" \
    --arg req_memory "${EDGEGAP_REQ_MEMORY:-1024}" \
    --arg game_port "${EDGEGAP_GAME_PORT:-7777}" \
    --arg protocol "${EDGEGAP_GAME_PROTOCOL:-UDP}" \
    --arg config_path "${LIGHTRIDER_CONFIG:-/app/config/default.ron}" \
    --arg nats_host "$NATS_HOST" \
    --arg nats_user "$NATS_USER" \
    --arg nats_password "$NATS_PASSWORD" \
    --arg bevygap_nats_namespace "$bevygap_nats_namespace" \
    --arg protocol_id "$LIGHTRIDER_PROTOCOL_ID" \
    --arg private_key "$LIGHTRIDER_PRIVATE_KEY" \
    --arg require_production_netcode "${LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE:-1}" \
    --arg secure_nats "$secure_nats" \
    --arg nats_insecure "$nats_insecure" \
    --arg nats_ca "${NATS_CA:-}" \
    --arg nats_ca_contents "${NATS_CA_CONTENTS:-}" \
    --arg session_mapping_ttl_ms "${BEVYGAP_SESSION_MAPPING_TTL_MS:-}" \
    --arg unclaimed_session_ttl_secs "${BEVYGAP_UNCLAIMED_SESSION_TTL_SECS:-}" \
    --arg active_connection_ttl_secs "${BEVYGAP_ACTIVE_CONNECTION_TTL_SECS:-}" \
    --arg cert_digest_ttl_secs "${BEVYGAP_CERT_DIGEST_TTL_SECS:-}" \
    --arg session_kind "${EDGEGAP_SESSION_KIND:-Seat}" \
    --arg session_sockets "${EDGEGAP_SESSION_SOCKETS:-50}" \
    --arg session_empty_ttl "${EDGEGAP_SESSION_EMPTY_TTL:-5}" \
    --arg session_max_duration "${EDGEGAP_SESSION_MAX_DURATION:-60}" \
    --arg force_cache "${EDGEGAP_FORCE_CACHE:-false}" \
    '
    def bool($v): ($v | ascii_downcase) as $x | ($x == "1" or $x == "true" or $x == "yes" or $x == "on");
    def env_item($key; $value; $secret):
      {key: $key, value: $value} + (if $secret then {is_secret: true} else {} end);
    def maybe_env($key; $value; $secret):
      if $value == "" then empty else env_item($key; $value; $secret) end;

    {
      name: $name,
      is_active: true,
      docker_repository: $repository,
      docker_image: $image,
      docker_tag: $tag,
      use_telemetry: true,
      force_cache: bool($force_cache),
      session_config: {
        kind: $session_kind,
        sockets: ($session_sockets | tonumber),
        autodeploy: true,
        empty_ttl: ($session_empty_ttl | tonumber),
        session_max_duration: ($session_max_duration | tonumber)
      },
      ports: [
        {
          name: "game",
          port: ($game_port | tonumber),
          protocol: $protocol,
          to_check: false
        }
      ],
      envs: [
        env_item("PORT"; $game_port; false),
        env_item("LIGHTRIDER_CONFIG"; $config_path; false),
        env_item("LIGHTRIDER_BEVYGAP"; "1"; false),
        env_item("LIGHTRIDER_PROTOCOL_ID"; $protocol_id; false),
        env_item("LIGHTRIDER_PRIVATE_KEY"; $private_key; true),
        env_item("LIGHTRIDER_REQUIRE_PRODUCTION_NETCODE"; $require_production_netcode; false),
        env_item("NATS_HOST"; $nats_host; false),
        env_item("NATS_USER"; $nats_user; false),
        env_item("NATS_PASSWORD"; $nats_password; true),
        env_item("BEVYGAP_NATS_NAMESPACE"; $bevygap_nats_namespace; false),
        env_item("BEVYGAP_REQUIRE_SECURE_NATS"; $secure_nats; false),
        maybe_env("NATS_INSECURE"; $nats_insecure; false),
        maybe_env("NATS_CA"; $nats_ca; false),
        maybe_env("NATS_CA_CONTENTS"; $nats_ca_contents; true),
        maybe_env("BEVYGAP_SESSION_MAPPING_TTL_MS"; $session_mapping_ttl_ms; false),
        maybe_env("BEVYGAP_UNCLAIMED_SESSION_TTL_SECS"; $unclaimed_session_ttl_secs; false),
        maybe_env("BEVYGAP_ACTIVE_CONNECTION_TTL_SECS"; $active_connection_ttl_secs; false),
        maybe_env("BEVYGAP_CERT_DIGEST_TTL_SECS"; $cert_digest_ttl_secs; false)
      ]
    }
    + (if $payload_mode == "patch" then {} else {
      req_cpu: ($req_cpu | tonumber),
      req_memory: ($req_memory | tonumber)
    } end)
    + (if $private_username != "" then {private_username: $private_username} else {} end)
    + (if $private_token != "" then {private_token: $private_token} else {} end)
    '
}

redacted_filter='
  def secret_key:
    (.key // "" | test("(PASSWORD|PRIVATE_KEY|TOKEN|SECRET|CA_CONTENTS)$"));
  def redact_envs:
    (.envs // [])
    | map({
        key,
        value: (if ((.is_secret // false) or secret_key) then "<secret>" else (.value // "") end),
        is_secret: ((.is_secret // false) or secret_key)
      })
    | sort_by(.key);
  {
    name,
    is_active,
    docker_repository,
    docker_image,
    docker_tag,
    private_username: (if has("private_username") then "<present>" else null end),
    private_token: (if has("private_token") then "<secret>" else null end),
    req_cpu,
    req_memory,
    use_telemetry,
    force_cache,
    session_config: {
      kind: .session_config.kind,
      sockets: .session_config.sockets,
      autodeploy: .session_config.autodeploy,
      empty_ttl: .session_config.empty_ttl,
      session_max_duration: .session_config.session_max_duration
    },
    ports: ((.ports // [])
      | map({
          name,
          port,
          protocol,
          to_check: (.to_check // false)
        })
      | sort_by(.name // "", .port, .protocol)),
    envs: redact_envs
  }
'

redact_payload() {
  jq "$redacted_filter" "$1"
}

write_manifest() {
  local desired_file="$1"
  local manifest=".edgegap-build/edgegap-app-version.json"
  local hash
  mkdir -p .edgegap-build
  hash="$(sha256sum "$desired_file" | awk '{print $1}')"
  jq \
    --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg app "$app" \
    --arg version "$version" \
    --arg image_tag "$tag" \
    --arg desired_sha256 "$hash" \
    "$redacted_filter + {
      generated_at: \$generated_at,
      app: \$app,
      version: \$version,
      image_tag: \$image_tag,
      desired_sha256: \$desired_sha256
    }" \
    "$desired_file" > "$manifest"
  echo "Wrote redacted desired manifest: $manifest"
}

get_app_version() {
  local output_file="$1"
  local app_enc version_enc
  app_enc="$(urlencode "$app")"
  version_enc="$(urlencode "$version")"
  api_request GET "/v1/app/${app_enc}/version/${version_enc}" "" "$output_file"
}

ensure_app_exists() {
  local app_file status app_enc payload_file
  app_file="$(mktemp)"
  app_enc="$(urlencode "$app")"
  status="$(api_request GET "/v1/app/${app_enc}" "" "$app_file")"
  if [[ "$status" =~ ^2 ]]; then
    rm -f "$app_file"
    return
  fi

  if [[ "$status" != "404" || "$create_app" != "1" ]]; then
    echo "edgegap-app-version: Edgegap app '$app' is missing or unreadable (HTTP $status)." >&2
    echo "edgegap-app-version: create the app first, or rerun sync with --create-app." >&2
    cat "$app_file" >&2 || true
    rm -f "$app_file"
    exit 1
  fi

  payload_file="$(mktemp)"
  jq -n \
    --arg name "$app" \
    --arg image "${EDGEGAP_APP_IMAGE_BASE64:-iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+/p9sAAAAASUVORK5CYII=}" \
    '{name: $name, is_active: true, image: $image}' > "$payload_file"
  status="$(api_request POST "/v1/app" "$payload_file" "$app_file")"
  if [[ ! "$status" =~ ^2 ]]; then
    echo "edgegap-app-version: failed to create Edgegap app '$app' (HTTP $status)." >&2
    cat "$app_file" >&2 || true
    rm -f "$app_file" "$payload_file"
    exit 1
  fi
  echo "Created Edgegap app '$app'"
  rm -f "$app_file" "$payload_file"
}

print_diff() {
  local current_file="$1"
  local desired_file="$2"
  local current_redacted desired_redacted
  current_redacted="$(mktemp)"
  desired_redacted="$(mktemp)"
  redact_payload "$current_file" > "$current_redacted"
  redact_payload "$desired_file" > "$desired_redacted"
  diff -u --label "current:${app}/${version}" "$current_redacted" --label "desired:${app}/${version}" "$desired_redacted" || true
  rm -f "$current_redacted" "$desired_redacted"
}

verify_current_matches_desired() {
  local current_file="$1"
  local desired_file="$2"
  local current_cmp desired_cmp
  current_cmp="$(mktemp)"
  desired_cmp="$(mktemp)"
  redact_payload "$current_file" > "$current_cmp"
  redact_payload "$desired_file" > "$desired_cmp"
  if cmp -s "$current_cmp" "$desired_cmp"; then
    echo "Edgegap app version matches desired deploy-critical state: ${app}/${version}"
    rm -f "$current_cmp" "$desired_cmp"
    return 0
  fi
  echo "edgegap-app-version: current app version differs from desired state" >&2
  diff -u --label "current:${app}/${version}" "$current_cmp" --label "desired:${app}/${version}" "$desired_cmp" >&2 || true
  rm -f "$current_cmp" "$desired_cmp"
  return 1
}

case "$command" in
  desired)
    require_desired_env
    desired_file="$(mktemp)"
    build_desired_payload > "$desired_file"
    redact_payload "$desired_file"
    write_manifest "$desired_file"
    rm -f "$desired_file"
    ;;
  show)
    require_api
    current_file="$(mktemp)"
    status="$(get_app_version "$current_file")"
    if [[ ! "$status" =~ ^2 ]]; then
      echo "edgegap-app-version: failed to fetch ${app}/${version} (HTTP $status)" >&2
      cat "$current_file" >&2 || true
      rm -f "$current_file"
      exit 1
    fi
    redact_payload "$current_file"
    rm -f "$current_file"
    ;;
  diff)
    require_api
    require_desired_env
    desired_file="$(mktemp)"
    current_file="$(mktemp)"
    build_desired_payload > "$desired_file"
    status="$(get_app_version "$current_file")"
    if [[ "$status" == "404" ]]; then
      echo "Edgegap app version ${app}/${version} does not exist; sync would create it."
      redact_payload "$desired_file"
      write_manifest "$desired_file"
      rm -f "$desired_file" "$current_file"
      exit 0
    fi
    if [[ ! "$status" =~ ^2 ]]; then
      echo "edgegap-app-version: failed to fetch ${app}/${version} (HTTP $status)" >&2
      cat "$current_file" >&2 || true
      rm -f "$desired_file" "$current_file"
      exit 1
    fi
    print_diff "$current_file" "$desired_file"
    write_manifest "$desired_file"
    rm -f "$desired_file" "$current_file"
    ;;
  sync)
    require_api
    require_desired_env
    ensure_app_exists
    desired_file="$(mktemp)"
    current_file="$(mktemp)"
    patch_file="$(mktemp)"
    build_desired_payload > "$desired_file"
    build_desired_payload patch > "$patch_file"
    status="$(get_app_version "$current_file")"
    app_enc="$(urlencode "$app")"
    version_enc="$(urlencode "$version")"
    if [[ "$status" == "404" ]]; then
      echo "Creating Edgegap app version ${app}/${version}"
      status="$(api_request POST "/v1/app/${app_enc}/version" "$desired_file" "$current_file")"
    elif [[ "$status" =~ ^2 ]]; then
      echo "Updating Edgegap app version ${app}/${version}"
      status="$(api_request PATCH "/v1/app/${app_enc}/version/${version_enc}" "$patch_file" "$current_file")"
    else
      echo "edgegap-app-version: failed to fetch ${app}/${version} before sync (HTTP $status)" >&2
      cat "$current_file" >&2 || true
      rm -f "$desired_file" "$current_file" "$patch_file"
      exit 1
    fi
    if [[ ! "$status" =~ ^2 ]]; then
      echo "edgegap-app-version: sync failed for ${app}/${version} (HTTP $status)" >&2
      cat "$current_file" >&2 || true
      rm -f "$desired_file" "$current_file" "$patch_file"
      exit 1
    fi
    status="$(get_app_version "$current_file")"
    if [[ ! "$status" =~ ^2 ]]; then
      echo "edgegap-app-version: failed to refetch ${app}/${version} after sync (HTTP $status)" >&2
      cat "$current_file" >&2 || true
      rm -f "$desired_file" "$current_file" "$patch_file"
      exit 1
    fi
    verify_current_matches_desired "$current_file" "$desired_file"
    write_manifest "$desired_file"
    rm -f "$desired_file" "$current_file" "$patch_file"
    ;;
  verify)
    require_api
    require_desired_env
    desired_file="$(mktemp)"
    current_file="$(mktemp)"
    build_desired_payload > "$desired_file"
    status="$(get_app_version "$current_file")"
    if [[ ! "$status" =~ ^2 ]]; then
      echo "edgegap-app-version: failed to fetch ${app}/${version} (HTTP $status)" >&2
      cat "$current_file" >&2 || true
      rm -f "$desired_file" "$current_file"
      exit 1
    fi
    verify_current_matches_desired "$current_file" "$desired_file"
    write_manifest "$desired_file"
    rm -f "$desired_file" "$current_file"
    ;;
esac
