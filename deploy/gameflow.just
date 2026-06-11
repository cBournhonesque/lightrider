deploy-help:
    #!/usr/bin/env bash
    set -euo pipefail
    cat <<'EOF'
    Deployment entry points:
      just prod-images-build-push tag=<tag>
      just edgegap-release-sync tag=<tag> nats_host=<host:4222> app=lightrider version=<edgegap-version>
      just deploy-web-server host=<vps-host> tag=<tag> [domain=<domain>] [https=1]
      just deploy-web-server-pull host=<vps-host> tag=<already-pushed-tag>

    GameFlow-compatible aliases:
      just gameflow-build tag=<tag>
      just gameflow-push tag=<tag>
      just gameflow-build-push tag=<tag>
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

gameflow-sync tag=edgegap-default-tag nats_host="45.79.138.102:4222" app="lightrider" version="":
    just edgegap-release-sync "{{ tag }}" "{{ nats_host }}" "{{ app }}" "{{ version }}"

gameflow-deploy-host *args:
    just deploy-web-server {{ args }}

gameflow-deploy-host-pull *args:
    just deploy-web-server-pull {{ args }}

gameflow-smoke-local seconds="8" config="config/test.ron" port="7777" matchmaker_port="3000" client_id="3001" release="false":
    just lightyear-matchmaker-local-smoke "{{ seconds }}" "{{ config }}" "{{ port }}" "{{ matchmaker_port }}" "{{ client_id }}" "{{ release }}"
