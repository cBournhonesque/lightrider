#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: deploy/gameflow_server_zip.sh [output.zip]

Builds the GameFlow server upload archive. Run through:

  just gameflow-server-zip output=.edgegap-build/gameflow/server.zip port=7898

The archive root contains Dockerfile plus the multi-repo Rust source tree that
Dockerfile expects.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
context_root="$repo_root/.edgegap-build/context"
output="${1:-.edgegap-build/gameflow/server.zip}"
port="${GAMEFLOW_PORT:-7898}"

if [[ "$port" == *[!0-9]* || -z "$port" || "$port" -lt 1 || "$port" -gt 65535 ]]; then
  echo "gameflow-server-zip: GAMEFLOW_PORT must be an integer from 1 to 65535" >&2
  exit 2
fi

for path in \
  "$context_root/lightrider" \
  "$context_root/lightyear" \
  "$context_root/lightyear-matchmaker" \
  "$context_root/bevy_replicon"; do
  if [[ ! -d "$path" ]]; then
    echo "gameflow-server-zip: missing staged context at $path" >&2
    echo "Run: just edgegap-context" >&2
    exit 1
  fi
done

if ! command -v zip >/dev/null 2>&1; then
  echo "gameflow-server-zip: zip is required" >&2
  exit 1
fi

case "$output" in
  /*) output_path="$output" ;;
  *) output_path="$repo_root/$output" ;;
esac

stage_root="$repo_root/.edgegap-build/gameflow/server-zip-root"
rm -rf "$stage_root"
mkdir -p "$stage_root" "$(dirname "$output_path")"

sed "s/__GAMEFLOW_PORT__/${port}/g" \
  "$repo_root/deploy/Dockerfile.gameflow-server" \
  > "$stage_root/Dockerfile"

rsync -a --delete "$context_root/lightrider/" "$stage_root/lightrider/"
rsync -a --delete "$context_root/lightyear/" "$stage_root/lightyear/"
rsync -a --delete "$context_root/lightyear-matchmaker/" "$stage_root/lightyear-matchmaker/"
rsync -a --delete "$context_root/bevy_replicon/" "$stage_root/bevy_replicon/"

if [[ -f "$context_root/host-ca-certificates.crt" ]]; then
  cp "$context_root/host-ca-certificates.crt" "$stage_root/host-ca-certificates.crt"
else
  : > "$stage_root/host-ca-certificates.crt"
fi

rm -f "$output_path"
(
  cd "$stage_root"
  zip -qr -X "$output_path" .
)

echo "Wrote $output_path"
