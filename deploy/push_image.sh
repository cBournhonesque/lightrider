#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  cat <<'EOF'
Usage:
  deploy/push_image.sh <image>

Environment:
  PODMAN_PUSH_RETRIES=4
  PODMAN_PUSH_RETRY_SLEEP=10

Retries transient registry/network upload failures such as broken pipes while
writing image blobs.
EOF
  exit 0
fi

if [[ $# -ne 1 ]]; then
  cat <<'EOF'
Usage:
  deploy/push_image.sh <image>
EOF
  exit 2
fi

image="$1"
retries="${PODMAN_PUSH_RETRIES:-4}"
retry_sleep="${PODMAN_PUSH_RETRY_SLEEP:-10}"

if [[ ! "$retries" =~ ^[1-9][0-9]*$ ]]; then
  echo "PODMAN_PUSH_RETRIES must be a positive integer; got '$retries'" >&2
  exit 2
fi
if [[ ! "$retry_sleep" =~ ^[0-9]+$ ]]; then
  echo "PODMAN_PUSH_RETRY_SLEEP must be a non-negative integer; got '$retry_sleep'" >&2
  exit 2
fi

last_status=0
for attempt in $(seq 1 "$retries"); do
  echo "Pushing $image (attempt $attempt/$retries)"
  if podman push "$image"; then
    echo "Pushed $image"
    exit 0
  fi
  last_status=$?
  if [[ "$attempt" -lt "$retries" ]]; then
    delay=$((retry_sleep * attempt))
    echo "Push failed for $image with status $last_status; retrying in ${delay}s" >&2
    sleep "$delay"
  fi
done

echo "Failed to push $image after $retries attempts" >&2
exit "$last_status"
