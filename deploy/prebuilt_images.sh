#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  deploy/prebuilt_images.sh artifacts [zig_target=<target>] [native_jobs=<n>] [nofile_limit=<n>] [wasm_bindgen_version=<version>]
  deploy/prebuilt_images.sh images [tag=<tag>] [platform=linux/amd64]
  deploy/prebuilt_images.sh push [tag=<tag>] [push_retries=<n>]
  deploy/prebuilt_images.sh build-push [tag=<tag>] [zig_target=<target>] [platform=linux/amd64] [native_jobs=<n>] [nofile_limit=<n>] [push_retries=<n>]

Defaults:
  zig_target=x86_64-unknown-linux-gnu.2.36
  platform=linux/amd64
  native_jobs=1
  nofile_limit=8192
  push_retries=4
  wasm_bindgen_version=0.2.126

Prerequisites for artifacts:
  brew install zig
  cargo install cargo-zigbuild

The native artifacts are Linux binaries. The web-client artifact is WASM plus
static web files. All artifacts are staged under .edgegap-build/context/prebuilt
and copied by the *.prebuilt Dockerfiles without compiling Rust in Docker.
EOF
}

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$repo_root"

cmd="${1:-}"
if [[ -z "$cmd" || "$cmd" == "-h" || "$cmd" == "--help" ]]; then
  usage
  exit 0
fi
shift

tag="$(printf '%s-%s' "$(git rev-parse --short HEAD 2>/dev/null || echo local)" "$(date -u +%Y%m%d%H%M%S)")"
zig_target="${PREBUILT_ZIG_TARGET:-x86_64-unknown-linux-gnu.2.36}"
platform="${PREBUILT_PLATFORM:-linux/amd64}"
native_jobs="${PREBUILT_CARGO_JOBS:-1}"
nofile_limit="${PREBUILT_NOFILE_LIMIT:-8192}"
push_retries="${PODMAN_PUSH_RETRIES:-4}"
wasm_bindgen_version="${WASM_BINDGEN_CLI_VERSION:-0.2.126}"

for arg in "$@"; do
  case "$arg" in
    tag=*) tag="${arg#tag=}" ;;
    zig_target=*) zig_target="${arg#zig_target=}" ;;
    target=*) zig_target="${arg#target=}" ;;
    platform=*) platform="${arg#platform=}" ;;
    native_jobs=*) native_jobs="${arg#native_jobs=}" ;;
    jobs=*) native_jobs="${arg#jobs=}" ;;
    nofile_limit=*) nofile_limit="${arg#nofile_limit=}" ;;
    push_retries=*) push_retries="${arg#push_retries=}" ;;
    wasm_bindgen_version=*) wasm_bindgen_version="${arg#wasm_bindgen_version=}" ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unexpected argument: $arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! "$native_jobs" =~ ^[1-9][0-9]*$ ]]; then
  echo "native_jobs must be a positive integer; got '$native_jobs'" >&2
  exit 2
fi
if [[ ! "$nofile_limit" =~ ^[1-9][0-9]*$ ]]; then
  echo "nofile_limit must be a positive integer; got '$nofile_limit'" >&2
  exit 2
fi
if [[ ! "$push_retries" =~ ^[1-9][0-9]*$ ]]; then
  echo "push_retries must be a positive integer; got '$push_retries'" >&2
  exit 2
fi

target_triple() {
  local target="$1"
  if [[ "$target" == *-linux-gnu.* ]]; then
    printf '%s\n' "${target%%.[0-9]*}"
  else
    printf '%s\n' "$target"
  fi
}

zig_cc_target() {
  local target="$1"
  case "$target" in
    x86_64-unknown-linux-gnu*)
      printf 'x86_64-linux-gnu%s\n' "${target#x86_64-unknown-linux-gnu}"
      ;;
    aarch64-unknown-linux-gnu*)
      printf 'aarch64-linux-gnu%s\n' "${target#aarch64-unknown-linux-gnu}"
      ;;
    *)
      echo "unsupported prebuilt zig target for cc-rs: $target" >&2
      exit 2
      ;;
  esac
}

native_triple="$(target_triple "$zig_target")"
native_env_triple="${native_triple//-/_}"
zig_cc_triple="$(zig_cc_target "$zig_target")"
native_target_dir="${PREBUILT_CARGO_TARGET_DIR:-$repo_root/.edgegap-build/zig-target}"
prebuilt_root="$repo_root/.edgegap-build/context/prebuilt"
prebuilt_bin="$prebuilt_root/bin"
prebuilt_web="$prebuilt_root/webclient"

raise_fd_limit() {
  local current
  current="$(ulimit -n 2>/dev/null || printf '0')"
  case "$current:$nofile_limit" in
    *[!0-9:]*)
      echo "Could not parse open-file limit current=$current target=$nofile_limit; continuing." >&2
      return 0
      ;;
  esac
  if (( current < nofile_limit )); then
    if ulimit -n "$nofile_limit" 2>/dev/null; then
      echo "Raised open-file limit from $current to $(ulimit -n)"
    else
      echo "Warning: could not raise open-file limit from $current to $nofile_limit." >&2
      echo "If linking fails with ProcessFdQuotaExceeded, run 'ulimit -n $nofile_limit' in this shell first." >&2
    fi
  fi
}

require_prebuilt_context() {
  if [[ ! -d .edgegap-build/context/lightrider ]]; then
    echo "prebuilt context is missing; run artifacts first" >&2
    exit 1
  fi
}

require_zigbuild() {
  command -v zig >/dev/null 2>&1 || {
    echo "zig is required. Install it with: brew install zig" >&2
    exit 1
  }
  command -v cargo-zigbuild >/dev/null 2>&1 || {
    echo "cargo-zigbuild is required. Install it with: cargo install cargo-zigbuild" >&2
    exit 1
  }
  rustup target add "$native_triple"
}

configure_zig_cc_env() {
  export CRATE_CC_NO_DEFAULTS="${CRATE_CC_NO_DEFAULTS:-1}"
  export "CC_${native_env_triple}=zig cc -target ${zig_cc_triple}"
  export "CXX_${native_env_triple}=zig c++ -target ${zig_cc_triple}"
  export "AR_${native_env_triple}=zig ar"
  export "RANLIB_${native_env_triple}=zig ranlib"
}

native_binary_path() {
  local name="$1"
  local candidate
  for candidate in \
    "$native_target_dir/$native_triple/release/$name" \
    "$native_target_dir/$zig_target/release/$name"; do
    if [[ -x "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  echo "could not find built binary '$name' under $native_target_dir" >&2
  exit 1
}

install_wasm_bindgen() {
  local tool_root="$repo_root/.edgegap-build/tools"
  local tool_target="$repo_root/.edgegap-build/tools-target"
  local wasm_bindgen="$tool_root/bin/wasm-bindgen"
  local need=1
  if [[ -x "$wasm_bindgen" ]]; then
    local installed
    installed="$("$wasm_bindgen" --version | awk '{print $2}')"
    if [[ "$installed" == "$wasm_bindgen_version" ]]; then
      need=0
    fi
  fi
  if [[ "$need" == "1" ]]; then
    CARGO_TARGET_DIR="$tool_target" cargo install \
      wasm-bindgen-cli \
      --version "$wasm_bindgen_version" \
      --locked \
      --force \
      --root "$tool_root" >&2
  fi
  printf '%s\n' "$wasm_bindgen"
}

gzip_static_assets() {
  find "$prebuilt_web" -type f \
    \( -name '*.wasm' -o -name '*.js' -o -name '*.css' -o -name '*.html' -o -name '*.json' \) \
    -exec sh -c 'for file do gzip -9 -c "$file" > "$file.gz"; done' sh {} +
}

build_server_artifact() {
  require_zigbuild
  raise_fd_limit
  configure_zig_cc_env
  mkdir -p "$prebuilt_bin"
  echo "Building Linux game server with cargo-zigbuild target $zig_target, jobs=$native_jobs"
  CARGO_BUILD_JOBS="$native_jobs" \
  CARGO_TARGET_DIR="$native_target_dir" \
    cargo zigbuild --release -j "$native_jobs" \
      --target "$zig_target" \
      -p server \
      --features lightyear-matchmaker \
      --bin lightrider-server
  cp "$(native_binary_path lightrider-server)" "$prebuilt_bin/lightrider-server"
  file "$prebuilt_bin/lightrider-server" || true
}

build_matchmaker_artifact() {
  require_zigbuild
  raise_fd_limit
  configure_zig_cc_env
  mkdir -p "$prebuilt_bin"
  echo "Building Linux matchmaker with cargo-zigbuild target $zig_target, jobs=$native_jobs"
  CARGO_BUILD_JOBS="$native_jobs" \
  CARGO_TARGET_DIR="$native_target_dir" \
    cargo zigbuild --release -j "$native_jobs" \
      --manifest-path ../lightyear-matchmaker/Cargo.toml \
      --target "$zig_target" \
      -p lightyear_matchmaker_server \
      --bin lightyear_matchmaker_server
  cp "$(native_binary_path lightyear_matchmaker_server)" "$prebuilt_bin/lightyear_matchmaker_server"
  file "$prebuilt_bin/lightyear_matchmaker_server" || true
}

build_webclient_artifact() {
  echo "Building browser client WASM locally"
  rustup target add wasm32-unknown-unknown
  local wasm_bindgen
  wasm_bindgen="$(install_wasm_bindgen)"

  CARGO_INCREMENTAL="${WEB_CARGO_INCREMENTAL:-0}" \
  CARGO_BUILD_JOBS="${WEB_CARGO_JOBS:-1}" \
  CARGO_PROFILE_RELEASE_OPT_LEVEL="${WEB_RELEASE_OPT_LEVEL:-1}" \
  CARGO_PROFILE_RELEASE_LTO="${WEB_RELEASE_LTO:-false}" \
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${WEB_RELEASE_CODEGEN_UNITS:-16}" \
    cargo build --release -j "${WEB_CARGO_JOBS:-1}" \
      --package web_client \
      --features lightyear-matchmaker \
      --bin lightrider-web \
      --target wasm32-unknown-unknown

  rm -rf "$prebuilt_web"
  mkdir -p "$prebuilt_web/pkg" "$prebuilt_web/assets"
  "$wasm_bindgen" \
    --target web \
    --out-dir "$prebuilt_web/pkg" \
    "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/release/lightrider-web.wasm"
  cp web/index.html "$prebuilt_web/index.html"
  cp -R assets/. "$prebuilt_web/assets/"

  local wasm_file="$prebuilt_web/pkg/lightrider-web_bg.wasm"
  local wasm_hash
  wasm_hash="$(shasum -a 256 "$wasm_file" | cut -c1-16)"
  local hashed_wasm="$prebuilt_web/pkg/lightrider-web_bg.${wasm_hash}.wasm"
  mv "$wasm_file" "$hashed_wasm"
  sed -i.bak "s/lightrider-web_bg\\.wasm/lightrider-web_bg.${wasm_hash}.wasm/g" \
    "$prebuilt_web/pkg/lightrider-web.js" \
    "$prebuilt_web/index.html"
  rm -f "$prebuilt_web/pkg/lightrider-web.js.bak" "$prebuilt_web/index.html.bak"
  gzip_static_assets
}

build_artifacts() {
  just edgegap-context
  build_server_artifact
  build_matchmaker_artifact
  build_webclient_artifact
  echo "Prebuilt artifacts staged under $prebuilt_root"
}

load_registry_env() {
  if [[ -f secrets/edgegap.env ]]; then
    # shellcheck disable=SC1091
    source secrets/edgegap.env
  fi
  : "${EDGEGAP_REGISTRY_URL:=registry.edgegap.com}"
  : "${EDGEGAP_REGISTRY_PROJECT:?EDGEGAP_REGISTRY_PROJECT is required}"
}

image_for() {
  local name="$1"
  printf '%s/%s/%s:%s\n' "$EDGEGAP_REGISTRY_URL" "$EDGEGAP_REGISTRY_PROJECT" "$name" "$tag"
}

build_image() {
  local image_name="$1"
  local dockerfile="$2"
  local artifact="$3"
  require_prebuilt_context
  [[ -e "$artifact" ]] || {
    echo "missing prebuilt artifact: $artifact" >&2
    echo "run: just prebuilt-artifacts zig_target=$zig_target" >&2
    exit 1
  }
  local image
  image="$(image_for "$image_name")"
  echo "Building runtime-only image $image for $platform"
  podman build --layers \
    --platform "$platform" \
    -f "$dockerfile" \
    -t "$image" \
    .edgegap-build/context
  echo "$image" > ".edgegap-build/${image_name}-prebuilt-image.txt"
}

build_images() {
  load_registry_env
  build_image \
    lightrider-server \
    .edgegap-build/context/lightrider/deploy/Dockerfile.server.prebuilt \
    "$prebuilt_bin/lightrider-server"
  build_image \
    lightrider-matchmaker \
    .edgegap-build/context/lightrider/deploy/Dockerfile.matchmaker.prebuilt \
    "$prebuilt_bin/lightyear_matchmaker_server"
  build_image \
    lightrider-webclient \
    .edgegap-build/context/lightrider/deploy/Dockerfile.webclient.prebuilt \
    "$prebuilt_web/index.html"
}

push_images() {
  load_registry_env
  printf '%s' "$EDGEGAP_REGISTRY_TOKEN" | podman login "$EDGEGAP_REGISTRY_URL" \
    --username "$EDGEGAP_REGISTRY_USERNAME" \
    --password-stdin
  PODMAN_PUSH_RETRIES="$push_retries" deploy/push_image.sh "$(image_for lightrider-server)"
  PODMAN_PUSH_RETRIES="$push_retries" deploy/push_image.sh "$(image_for lightrider-matchmaker)"
  PODMAN_PUSH_RETRIES="$push_retries" deploy/push_image.sh "$(image_for lightrider-webclient)"
}

case "$cmd" in
  artifacts)
    build_artifacts
    ;;
  images)
    build_images
    ;;
  push)
    push_images
    ;;
  build-push)
    build_artifacts
    build_images
    push_images
    ;;
  *)
    echo "unknown command: $cmd" >&2
    usage >&2
    exit 2
    ;;
esac
