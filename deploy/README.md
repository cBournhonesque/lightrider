# Lightrider Deployment

Deployment-specific files live here:

- `Dockerfile.server`: game-server image for Edgegap/static deployments.
- `Dockerfile.matchmaker`: matchmaker/control image with NATS and `lightyear_matchmaker_server`.
- `Dockerfile.webclient`: static web-client image with nginx and the WASM client.
- `DEPLOYMENT.md`: full operator guide.
- `edgegap_app_version.sh`: Edgegap app-version sync/verify helper.
- `build_image.sh`: shared production image build implementation used by the just recipes.
- `setup_web_server_host.sh`: VPS/control-host installer.
- `server-container-entrypoint.sh`: game-server image entrypoint.
- `matchmaker-container-entrypoint.sh`: matchmaker/control image entrypoint.
- `webclient-container-entrypoint.sh`: static web-client image entrypoint.
- `local.just`: local run, smoke, trace, load, and web build recipes.
- `edgegap.just`: Edgegap image, app-version, and release-sync recipes.
- `static.just`: static/VPS host deployment recipes.
- `gameflow.just`: GameFlow-compatible deployment aliases.

The root `justfile` imports the split files above, so command names stay the
same. Common entry points:

```bash
just deploy-help
just prod-images-build-push tag=<tag>
just edgegap-release-sync tag=<tag> nats_host=<host:4222>
just control-host-deploy host=<vps-host> tag=<tag>
just control-host-deploy-pull host=<vps-host> tag=<already-pushed-tag>
just control-host-pull-game-server host=<vps-host> tag=<already-pushed-tag>
```

Recipe layering:

```text
control-host-deploy
  prod-images-build-push
  web-server-env-template
  web-server-env-check
  web-server-install
  web-server-health

control-host-deploy-pull
  control-host-deploy with SKIP_IMAGE_BUILD=1

control-host-pull-game-server/matchmaker/webclient
  pull and restart one existing systemd service only
```

The `web-server-*` recipes are lower-level implementation steps. Prefer the
`control-host-*` recipes for normal deployment.

GameFlow-compatible wrappers are also available:

```bash
just gameflow-build-push tag=<tag>
just gameflow-sync tag=<tag> nats_host=<host:4222>
just gameflow-deploy-host host=<vps-host> tag=<tag>
just gameflow-smoke-local
```

The Dockerfiles are designed for the staged multi-repo context produced by
`just edgegap-context`, not for `podman build .` from this directory.
