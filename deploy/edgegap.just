clean-edgegap-cache:
    podman builder prune -f

# Stage the multi-repo image build context used by both game-server and
# matchmaker images. The Dockerfiles expect these sibling directories under
# .edgegap-build/context.
edgegap-context:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p .edgegap-build/context
    rm -rf .edgegap-build/context/bevygap
    rsync -a --delete \
      --exclude .edgegap-build \
      --exclude .git \
      --exclude logs \
      --exclude pkg \
      --exclude secrets \
      --exclude target \
      ./ .edgegap-build/context/lightrider/
    rsync -a --delete \
      --exclude .git \
      --exclude target \
      ../lightyear/ .edgegap-build/context/lightyear/
    rsync -a --delete \
      --exclude .git \
      --exclude target \
      ../lightyear-matchmaker/ .edgegap-build/context/lightyear-matchmaker/
    rsync -a --delete \
      --exclude .git \
      --exclude target \
      ../bevy_replicon/ .edgegap-build/context/bevy_replicon/
    if [[ -f /etc/ssl/certs/ca-certificates.crt ]]; then
      cp /etc/ssl/certs/ca-certificates.crt .edgegap-build/context/host-ca-certificates.crt
    else
      : > .edgegap-build/context/host-ca-certificates.crt
    fi
    cargo metadata \
      --manifest-path .edgegap-build/context/lightyear-matchmaker/Cargo.toml \
      --no-deps \
      --format-version 1 \
      >/dev/null

edgegap-login:
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    printf '%s' "$EDGEGAP_REGISTRY_TOKEN" | podman login "$EDGEGAP_REGISTRY_URL" \
      --username "$EDGEGAP_REGISTRY_USERNAME" \
      --password-stdin

# Build the deployable game-server image. The historical recipe name is kept
# because it also builds the image used by Edgegap app versions.
edgegap-build *args: edgegap-context
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    tag="{{ edgegap-default-tag }}"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
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
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-server:$tag"
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
    build_cmd=(podman build --layers)
    if [[ "${NO_CACHE:-0}" == "1" ]]; then
      build_cmd+=(--no-cache)
    fi
    build_memory="${PODMAN_BUILD_MEMORY:-$build_memory}"
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
    if [[ -n "$build_cpus" ]]; then
      if [[ ! "$build_cpus" =~ ^[0-9]+$ ]]; then
        echo "edgegap-build cpus must be an integer because podman build has no --cpus flag; got '$build_cpus'" >&2
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
    "${build_cmd[@]}" \
      --build-arg "SERVER_CARGO_JOBS=${SERVER_CARGO_JOBS:-2}" \
      --build-arg "SERVER_CARGO_INCREMENTAL=${SERVER_CARGO_INCREMENTAL:-0}" \
      --build-arg "SERVER_RELEASE_OPT_LEVEL=${SERVER_RELEASE_OPT_LEVEL:-3}" \
      --build-arg "SERVER_RELEASE_LTO=${SERVER_RELEASE_LTO:-false}" \
      --build-arg "SERVER_RELEASE_CODEGEN_UNITS=${SERVER_RELEASE_CODEGEN_UNITS:-16}" \
      --build-arg "LIGHTYEAR_MATCHMAKER_MANIFEST_SHA=$lightyear_matchmaker_manifest_sha" \
      -f .edgegap-build/context/lightrider/deploy/Dockerfile.server \
      -t "$image" \
      .edgegap-build/context
    echo "$image" > .edgegap-build/server-image.txt
    echo "Built $image"

edgegap-push tag=edgegap-default-tag: edgegap-login
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-server:{{ tag }}"
    podman push "$image"
    echo "Pushed $image"

edgegap-build-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ edgegap-default-tag }}"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
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
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just edgegap-build tag="$tag" memory="$build_memory" cpus="$build_cpus" cpu_quota="$build_cpu_quota" cpuset_cpus="$build_cpuset_cpus"
    just edgegap-push "$tag"

# Clear aliases for the game-server image path. These call the older
# edgegap-* recipes because the image is still the Edgegap app-version image.
game-server-build *args:
    just edgegap-build {{ args }}

game-server-push tag=edgegap-default-tag:
    just edgegap-push "{{ tag }}"

game-server-build-push *args:
    just edgegap-build-push {{ args }}

edgegap-app-show version="dev" app="lightrider":
    deploy/edgegap_app_version.sh show --app "{{ app }}" --version "{{ version }}"

edgegap-app-desired tag=edgegap-default-tag version="dev" app="lightrider":
    deploy/edgegap_app_version.sh desired --app "{{ app }}" --version "{{ version }}" --tag "{{ tag }}"

edgegap-app-diff tag=edgegap-default-tag version="dev" app="lightrider":
    deploy/edgegap_app_version.sh diff --app "{{ app }}" --version "{{ version }}" --tag "{{ tag }}"

edgegap-app-sync tag=edgegap-default-tag version="dev" app="lightrider":
    deploy/edgegap_app_version.sh sync --app "{{ app }}" --version "{{ version }}" --tag "{{ tag }}"

edgegap-app-verify tag=edgegap-default-tag version="dev" app="lightrider":
    deploy/edgegap_app_version.sh verify --app "{{ app }}" --version "{{ version }}" --tag "{{ tag }}"

matchmaker-build *args: edgegap-context
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    tag="{{ edgegap-default-tag }}"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
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
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-matchmaker:$tag"
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
    # Defaults are balanced for a 32G+ build machine. Drop the env values to
    # 1/false/16 if rustc is OOM-killed on a smaller host.
    build_cmd=(podman build --layers)
    if [[ "${NO_CACHE:-0}" == "1" ]]; then
      build_cmd+=(--no-cache)
    fi
    build_memory="${PODMAN_BUILD_MEMORY:-$build_memory}"
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
    if [[ -n "$build_cpus" ]]; then
      if [[ ! "$build_cpus" =~ ^[0-9]+$ ]]; then
        echo "matchmaker-build cpus must be an integer because podman build has no --cpus flag; got '$build_cpus'" >&2
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
    "${build_cmd[@]}" \
      --build-arg "MATCHMAKER_CARGO_JOBS=${MATCHMAKER_CARGO_JOBS:-2}" \
      --build-arg "MATCHMAKER_CARGO_INCREMENTAL=${MATCHMAKER_CARGO_INCREMENTAL:-0}" \
      --build-arg "MATCHMAKER_RELEASE_OPT_LEVEL=${MATCHMAKER_RELEASE_OPT_LEVEL:-2}" \
      --build-arg "MATCHMAKER_RELEASE_LTO=${MATCHMAKER_RELEASE_LTO:-thin}" \
      --build-arg "MATCHMAKER_RELEASE_CODEGEN_UNITS=${MATCHMAKER_RELEASE_CODEGEN_UNITS:-8}" \
      --build-arg "WEB_CARGO_JOBS=${WEB_CARGO_JOBS:-1}" \
      --build-arg "WEB_CARGO_INCREMENTAL=${WEB_CARGO_INCREMENTAL:-0}" \
      --build-arg "WEB_RELEASE_OPT_LEVEL=${WEB_RELEASE_OPT_LEVEL:-1}" \
      --build-arg "WEB_RELEASE_LTO=${WEB_RELEASE_LTO:-false}" \
      --build-arg "WEB_RELEASE_CODEGEN_UNITS=${WEB_RELEASE_CODEGEN_UNITS:-16}" \
      --build-arg "LIGHTYEAR_MATCHMAKER_MANIFEST_SHA=$lightyear_matchmaker_manifest_sha" \
      -f .edgegap-build/context/lightrider/deploy/Dockerfile.matchmaker \
      -t "$image" \
      .edgegap-build/context
    echo "$image" > .edgegap-build/matchmaker-image.txt
    echo "Built $image"

matchmaker-push tag=edgegap-default-tag: edgegap-login
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-matchmaker:{{ tag }}"
    podman push "$image"
    echo "Pushed $image"

matchmaker-build-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ edgegap-default-tag }}"
    build_memory=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*) build_memory="${arg#memory=}" ;;
        build_memory=*) build_memory="${arg#build_memory=}" ;;
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
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just matchmaker-build tag="$tag" memory="$build_memory" cpus="$build_cpus" cpu_quota="$build_cpu_quota" cpuset_cpus="$build_cpuset_cpus"
    just matchmaker-push "$tag"

prod-images-build *args:
    just edgegap-build {{ args }}
    just matchmaker-build {{ args }}

prod-images-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ edgegap-default-tag }}"
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*|build_memory=*|cpus=*|build_cpus=*|cpu_quota=*|build_cpu_quota=*|cpuset_cpus=*|build_cpuset_cpus=*)
          ;;
        *)
          case "$positional" in
            0) tag="$arg" ;;
            1|2|3|4) ;;
            *)
              echo "unexpected extra argument for prod-images-push: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just edgegap-push "$tag"
    just matchmaker-push "$tag"

prod-images-build-push *args:
    just prod-images-build {{ args }}
    just prod-images-push {{ args }}

github-release tag=edgegap-default-tag title="" notes="":
    #!/usr/bin/env bash
    set -euo pipefail
    command -v gh >/dev/null 2>&1 || {
      echo "gh CLI is required to create a GitHub release from the command line" >&2
      exit 1
    }
    release_title="{{ title }}"
    release_notes="{{ notes }}"
    if [[ -z "$release_title" ]]; then
      release_title="{{ tag }}"
    fi
    if [[ -z "$release_notes" ]]; then
      release_notes="Release {{ tag }}"
    fi
    gh release create "{{ tag }}" --title "$release_title" --notes "$release_notes"

edgegap-release-sync tag=edgegap-default-tag nats_host="45.79.138.102:4222" app="lightrider" version="" env_file="secrets/web-server.env":
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    source secrets/prod-netcode.env
    if [[ -f "{{ env_file }}" ]]; then
      source "{{ env_file }}"
    fi
    edgegap_version="{{ version }}"
    if [[ -z "$edgegap_version" ]]; then
      edgegap_version="{{ tag }}"
    fi
    export NATS_HOST="{{ nats_host }}"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    export EDGEGAP_NATS_INSECURE="${EDGEGAP_NATS_INSECURE:-1}"
    export LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE_OVERRIDE:-{{ app }}_${edgegap_version}}"
    just edgegap-app-sync "{{ tag }}" "$edgegap_version" "{{ app }}"
    just edgegap-app-verify "{{ tag }}" "$edgegap_version" "{{ app }}"
