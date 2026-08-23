# Remove Podman builder cache layers. Use only when build cache is stale or
# disk pressure is high; the next production image build will be slower.
clean-edgegap-cache:
    podman builder prune -f

# Stage the multi-repo image build context used by the game-server,
# matchmaker, and web-client images. The Dockerfiles expect these sibling
# directories under .edgegap-build/context.
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

# Log in to the registry configured by secrets/edgegap.env.
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
    deploy/build_image.sh server {{ args }}

edgegap-push tag=edgegap-default-tag: edgegap-login
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-server:{{ tag }}"
    deploy/push_image.sh "$image"

edgegap-build-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ edgegap-default-tag }}"
    build_memory=""
    build_memory_swap=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
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
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just edgegap-build tag="$tag" memory="$build_memory" memory_swap="$build_memory_swap" cpus="$build_cpus" cpu_quota="$build_cpu_quota" cpuset_cpus="$build_cpuset_cpus"
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

# Build only the matchmaker/control image: NATS + lightyear_matchmaker_server.
matchmaker-build *args: edgegap-context
    deploy/build_image.sh matchmaker {{ args }}

# Push only the matchmaker/control image for an already-built tag.
matchmaker-push tag=edgegap-default-tag: edgegap-login
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-matchmaker:{{ tag }}"
    deploy/push_image.sh "$image"

# Build only the static browser client image: nginx + WASM/assets.
webclient-build *args: edgegap-context
    deploy/build_image.sh webclient {{ args }}

# Push only the static browser client image for an already-built tag.
webclient-push tag=edgegap-default-tag: edgegap-login
    #!/usr/bin/env bash
    set -euo pipefail
    source secrets/edgegap.env
    image="$EDGEGAP_REGISTRY_URL/$EDGEGAP_REGISTRY_PROJECT/lightrider-webclient:{{ tag }}"
    deploy/push_image.sh "$image"

matchmaker-build-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ edgegap-default-tag }}"
    build_memory=""
    build_memory_swap=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
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
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just matchmaker-build tag="$tag" memory="$build_memory" memory_swap="$build_memory_swap" cpus="$build_cpus" cpu_quota="$build_cpu_quota" cpuset_cpus="$build_cpuset_cpus"
    just matchmaker-push "$tag"

webclient-build-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ edgegap-default-tag }}"
    build_memory=""
    build_memory_swap=""
    build_cpus=""
    build_cpu_quota=""
    build_cpuset_cpus=""
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
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
              echo "unexpected extra argument: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    just webclient-build tag="$tag" memory="$build_memory" memory_swap="$build_memory_swap" cpus="$build_cpus" cpu_quota="$build_cpu_quota" cpuset_cpus="$build_cpuset_cpus"
    just webclient-push "$tag"

# Common build limit kwargs are forwarded to each image build.
# Build all production images locally: game server, matchmaker, and web client.
prod-images-build *args:
    just edgegap-build {{ args }}
    just matchmaker-build {{ args }}
    just webclient-build {{ args }}

# Build-resource kwargs are accepted and ignored for build-push arg reuse.
# Push all production images for a tag.
prod-images-push *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ edgegap-default-tag }}"
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        memory=*|build_memory=*|memory_swap=*|build_memory_swap=*|cpus=*|build_cpus=*|cpu_quota=*|build_cpu_quota=*|cpuset_cpus=*|build_cpuset_cpus=*)
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
    just webclient-push "$tag"

# Build then push all production images.
prod-images-build-push *args:
    just prod-images-build {{ args }}
    just prod-images-push {{ args }}

# Cross-compile Linux artifacts locally, then stage them for runtime-only images.
# Requires `zig` and `cargo-zigbuild` for the native server/matchmaker binaries.
prebuilt-artifacts *args:
    deploy/prebuilt_images.sh artifacts {{ args }}

# Build runtime-only images that copy locally prebuilt artifacts. Run
# `prebuilt-artifacts` first, or use `prebuilt-images-build-push`.
prebuilt-images-build *args:
    deploy/prebuilt_images.sh images {{ args }}

# Push already-built runtime-only prebuilt images.
prebuilt-images-push *args:
    deploy/prebuilt_images.sh push {{ args }}

# One-command path: cross-compile server + matchmaker, build web WASM locally,
# build runtime-only images, then push all three images.
prebuilt-images-build-push *args:
    deploy/prebuilt_images.sh build-push {{ args }}

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

edgegap-release-sync *args:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="{{ edgegap-default-tag }}"
    nats_host="45.79.138.102:4222"
    app="lightrider"
    version=""
    env_file="secrets/web-server.env"
    positional=0
    for arg in {{ args }}; do
      case "$arg" in
        tag=*) tag="${arg#tag=}" ;;
        nats_host=*) nats_host="${arg#nats_host=}" ;;
        host=*) nats_host="${arg#host=}" ;;
        app=*) app="${arg#app=}" ;;
        version=*) version="${arg#version=}" ;;
        edgegap_version=*) version="${arg#edgegap_version=}" ;;
        env_file=*) env_file="${arg#env_file=}" ;;
        env=*) env_file="${arg#env=}" ;;
        *)
          case "$positional" in
            0) tag="$arg" ;;
            1) nats_host="$arg" ;;
            2) app="$arg" ;;
            3) version="$arg" ;;
            4) env_file="$arg" ;;
            *)
              echo "unexpected extra argument for edgegap-release-sync: $arg" >&2
              exit 2
              ;;
          esac
          positional=$((positional + 1))
          ;;
      esac
    done
    source secrets/edgegap.env
    source secrets/prod-netcode.env
    if [[ -f "$env_file" ]]; then
      source "$env_file"
    fi
    edgegap_version="$version"
    if [[ -z "$edgegap_version" ]]; then
      edgegap_version="${EDGEGAP_APP_VERSION:-dev}"
    fi
    export NATS_HOST="$nats_host"
    export NATS_USER="${NATS_USER:-lightrider}"
    export NATS_PASSWORD="${NATS_PASSWORD:-lightrider}"
    if [[ -z "${EDGEGAP_NATS_INSECURE:-}" ]]; then
      export EDGEGAP_NATS_INSECURE="${NATS_ALLOW_INSECURE:-0}"
    fi
    export LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE="${LIGHTYEAR_MATCHMAKER_NATS_NAMESPACE_OVERRIDE:-${app}_${edgegap_version}}"
    just edgegap-app-sync "$tag" "$edgegap_version" "$app"
    just edgegap-app-verify "$tag" "$edgegap_version" "$app"
