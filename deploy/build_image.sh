#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  deploy/build_image.sh <server|matchmaker|webclient> [tag=<tag>] [memory=<limit>] [memory_swap=<limit|-1>] [cpus=<n>] [cpu_quota=<quota>] [cpuset_cpus=<list>]

Builds one production image from the staged .edgegap-build/context tree.
The just recipes run `edgegap-context` before calling this script.

Resource knobs:
  memory=24g        Passed to `podman build --memory`; this limits build containers.
  memory_swap=-1    Passed to `podman build --memory-swap`; -1 allows unlimited swap.
  cpus=4            Converted to `--cpu-period 100000 --cpu-quota 400000`.
  cpu_quota=200000  Passed directly to Podman for custom cgroup quota control.
  cpuset_cpus=0-3   Passed directly to Podman.

Environment overrides:
  PODMAN_BUILD_MEMORY, PODMAN_BUILD_CPUS, PODMAN_BUILD_CPU_QUOTA,
  PODMAN_BUILD_MEMORY_SWAP, PODMAN_BUILD_CPUSET_CPUS, PODMAN_CACHE_FROM,
  PODMAN_CACHE_TO, NO_CACHE.
EOF
}

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 2
fi
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

target="$1"
shift

if [[ -f secrets/edgegap.env ]]; then
  # shellcheck disable=SC1091
  source secrets/edgegap.env
fi
: "${EDGEGAP_REGISTRY_URL:=registry.edgegap.com}"
: "${EDGEGAP_REGISTRY_PROJECT:?EDGEGAP_REGISTRY_PROJECT is required}"

tag="$(git rev-parse --short HEAD 2>/dev/null || date +%Y%m%d%H%M%S)"
build_memory=""
build_memory_swap=""
build_cpus=""
build_cpu_quota=""
build_cpuset_cpus=""
positional=0

for arg in "$@"; do
  case "$arg" in
    -h|--help)
      usage
      exit 0
      ;;
    tag=*) tag="${arg#tag=}" ;;
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
        0) tag="$arg" ;;
        1) build_memory="$arg" ;;
        2) build_cpus="$arg" ;;
        3) build_cpu_quota="$arg" ;;
        4) build_cpuset_cpus="$arg" ;;
        *)
          echo "unexpected extra argument for $target image build: $arg" >&2
          exit 2
          ;;
      esac
      positional=$((positional + 1))
      ;;
  esac
done

case "$target" in
  server)
    image_name="lightrider-server"
    dockerfile=".edgegap-build/context/lightrider/deploy/Dockerfile.server"
    image_file=".edgegap-build/server-image.txt"
    build_args=(
      --build-arg "SERVER_CARGO_JOBS=${SERVER_CARGO_JOBS:-2}"
      --build-arg "SERVER_CARGO_INCREMENTAL=${SERVER_CARGO_INCREMENTAL:-0}"
      --build-arg "SERVER_RELEASE_OPT_LEVEL=${SERVER_RELEASE_OPT_LEVEL:-3}"
      --build-arg "SERVER_RELEASE_LTO=${SERVER_RELEASE_LTO:-false}"
      --build-arg "SERVER_RELEASE_CODEGEN_UNITS=${SERVER_RELEASE_CODEGEN_UNITS:-16}"
    )
    ;;
  matchmaker)
    image_name="lightrider-matchmaker"
    dockerfile=".edgegap-build/context/lightrider/deploy/Dockerfile.matchmaker"
    image_file=".edgegap-build/matchmaker-image.txt"
    build_args=(
      --build-arg "MATCHMAKER_CARGO_JOBS=${MATCHMAKER_CARGO_JOBS:-2}"
      --build-arg "MATCHMAKER_CARGO_INCREMENTAL=${MATCHMAKER_CARGO_INCREMENTAL:-0}"
      --build-arg "MATCHMAKER_RELEASE_OPT_LEVEL=${MATCHMAKER_RELEASE_OPT_LEVEL:-2}"
      --build-arg "MATCHMAKER_RELEASE_LTO=${MATCHMAKER_RELEASE_LTO:-thin}"
      --build-arg "MATCHMAKER_RELEASE_CODEGEN_UNITS=${MATCHMAKER_RELEASE_CODEGEN_UNITS:-8}"
    )
    ;;
  webclient)
    image_name="lightrider-webclient"
    dockerfile=".edgegap-build/context/lightrider/deploy/Dockerfile.webclient"
    image_file=".edgegap-build/webclient-image.txt"
    build_args=(
      --build-arg "WEB_CARGO_JOBS=${WEB_CARGO_JOBS:-1}"
      --build-arg "WEB_CARGO_INCREMENTAL=${WEB_CARGO_INCREMENTAL:-0}"
      --build-arg "WEB_RELEASE_OPT_LEVEL=${WEB_RELEASE_OPT_LEVEL:-1}"
      --build-arg "WEB_RELEASE_LTO=${WEB_RELEASE_LTO:-false}"
      --build-arg "WEB_RELEASE_CODEGEN_UNITS=${WEB_RELEASE_CODEGEN_UNITS:-16}"
    )
    ;;
  *)
    echo "unknown production image target: $target" >&2
    usage >&2
    exit 2
    ;;
esac

if [[ "$target" == "server" || "$target" == "matchmaker" ]]; then
  lightyear_matchmaker_manifest_sha="$(
    find .edgegap-build/context/lightyear-matchmaker \
      \( -name Cargo.toml -o -name Cargo.lock \) \
      -type f \
      -print0 |
    sort -z |
    xargs -0 sha256sum |
    sha256sum |
    awk '{print $1}'
  )"
  build_args+=(--build-arg "LIGHTYEAR_MATCHMAKER_MANIFEST_SHA=$lightyear_matchmaker_manifest_sha")
fi

build_cmd=(podman build --layers)
if [[ "${NO_CACHE:-0}" == "1" ]]; then
  build_cmd+=(--no-cache)
fi

build_memory="${PODMAN_BUILD_MEMORY:-$build_memory}"
build_memory_swap="${PODMAN_BUILD_MEMORY_SWAP:-$build_memory_swap}"
build_cpus="${PODMAN_BUILD_CPUS:-$build_cpus}"
build_cpu_quota="${PODMAN_BUILD_CPU_QUOTA:-$build_cpu_quota}"
build_cpuset_cpus="${PODMAN_BUILD_CPUSET_CPUS:-$build_cpuset_cpus}"

if [[ -n "${PODMAN_CACHE_FROM:-}" ]]; then
  build_cmd+=(--cache-from "$PODMAN_CACHE_FROM")
fi
if [[ -n "${PODMAN_CACHE_TO:-}" ]]; then
  build_cmd+=(--cache-to "$PODMAN_CACHE_TO")
fi
if [[ -n "$build_memory" ]]; then
  build_cmd+=(--memory "$build_memory")
fi
if [[ -n "$build_memory_swap" ]]; then
  build_cmd+=(--memory-swap "$build_memory_swap")
fi
if [[ -n "$build_cpus" ]]; then
  if [[ ! "$build_cpus" =~ ^[0-9]+$ ]]; then
    echo "$target build cpus must be an integer because podman build has no --cpus flag; got '$build_cpus'" >&2
    exit 2
  fi
  build_cmd+=(--cpu-period 100000 --cpu-quota "$((build_cpus * 100000))")
fi
if [[ -n "$build_cpu_quota" ]]; then
  build_cmd+=(--cpu-quota "$build_cpu_quota")
fi
if [[ -n "$build_cpuset_cpus" ]]; then
  build_cmd+=(--cpuset-cpus "$build_cpuset_cpus")
fi

image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/$image_name:$tag"

printf 'Building %s\n' "$image"
if [[ -n "$build_memory" || -n "$build_memory_swap" || -n "$build_cpus" || -n "$build_cpu_quota" || -n "$build_cpuset_cpus" ]]; then
  printf 'Podman build limits: memory=%s memory_swap=%s cpus=%s cpu_quota=%s cpuset_cpus=%s\n' \
    "${build_memory:-<default>}" \
    "${build_memory_swap:-<default>}" \
    "${build_cpus:-<default>}" \
    "${build_cpu_quota:-<default>}" \
    "${build_cpuset_cpus:-<default>}"
fi

"${build_cmd[@]}" \
  "${build_args[@]}" \
  -f "$dockerfile" \
  -t "$image" \
  .edgegap-build/context

echo "$image" > "$image_file"
echo "Built $image"
