deploy-help:
    #!/usr/bin/env bash
    set -euo pipefail
    cat <<'EOF'
    Deployment entry points:
      just prod-images-build-push tag=<tag>
      just prebuilt-images-build-push tag=<tag>
      just edgegap-release-sync tag=<tag> nats_host=<host:4222> app=lightrider version=<edgegap-version>
      just control-host-deploy host=<vps-host> tag=<tag> [domain=<domain>] [https=1]
      just control-host-deploy-pull host=<vps-host> tag=<already-pushed-tag>
      just control-host-pull-game-server host=<vps-host> tag=<already-pushed-tag>

    Recipe layering:
      control-host-deploy
        -> prod-images-build-push
        -> web-server-env-template
        -> web-server-env-check
        -> web-server-install
        -> web-server-health
      control-host-deploy-pull
        -> control-host-deploy with SKIP_IMAGE_BUILD=1
      control-host-pull-{game-server,matchmaker,webclient}
        -> pull/restart one existing systemd service only
      prebuilt-images-build-push
        -> local cargo-zigbuild/native + local WASM build
        -> runtime-only podman images
        -> push all three images

    GameFlow-compatible aliases:
      just gameflow-build tag=<tag>
      just gameflow-push tag=<tag>
      just gameflow-build-push tag=<tag>
      just gameflow-server-zip output=.edgegap-build/gameflow/server.zip port=7898
      just gameflow-sync tag=<tag> nats_host=<host:4222> [version=<edgegap-version>]
      just gameflow-deploy-host host=<vps-host> tag=<tag> [domain=<domain>] [https=1]
      just gameflow-deploy-host-pull host=<vps-host> tag=<already-pushed-tag>
      just gameflow-smoke-local

    Deployment files live under deploy/. Full notes: deploy/DEPLOYMENT.md
    EOF

# GameFlow-compatible wrappers. These keep stable, explicit recipe names for
# deployment runners while preserving the older Edgegap/VPS recipe names.
gameflow-help:
    just deploy-help

gameflow-build tag=edgegap-default-tag:
    just prod-images-build "{{ tag }}"

gameflow-push tag=edgegap-default-tag:
    just prod-images-push "{{ tag }}"

gameflow-build-push tag=edgegap-default-tag:
    just prod-images-build-push "{{ tag }}"

gameflow-server-zip output=".edgegap-build/gameflow/server.zip" port="7898": edgegap-context
    GAMEFLOW_PORT="{{ port }}" deploy/gameflow_server_zip.sh "{{ output }}"

gameflow-sync tag=edgegap-default-tag nats_host="45.79.138.102:4222" app="lightrider" version="":
    just edgegap-release-sync "{{ tag }}" "{{ nats_host }}" "{{ app }}" "{{ version }}"

gameflow-deploy-host *args:
    just control-host-deploy {{ args }}

gameflow-deploy-host-pull *args:
    just control-host-deploy-pull {{ args }}

gameflow-smoke-local seconds="8" config="config/test.ron" port="7777" matchmaker_port="3000" client_id="3001" release="false":
    just lightyear-matchmaker-local-smoke "{{ seconds }}" "{{ config }}" "{{ port }}" "{{ matchmaker_port }}" "{{ client_id }}" "{{ release }}"
