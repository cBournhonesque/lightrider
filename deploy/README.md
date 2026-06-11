# Lightrider Deployment

Deployment-specific files live here:

- `Dockerfile.server`: game-server image for Edgegap/static deployments.
- `Dockerfile.matchmaker`: matchmaker/control image with NATS, nginx, and the WASM client.
- `DEPLOYMENT.md`: full operator guide.
- `edgegap_app_version.sh`: Edgegap app-version sync/verify helper.
- `setup_web_server_host.sh`: VPS/control-host installer.
- `server-container-entrypoint.sh`: game-server image entrypoint.
- `matchmaker-container-entrypoint.sh`: matchmaker/control image entrypoint.
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
just deploy-web-server host=<vps-host> tag=<tag>
```

GameFlow-compatible wrappers are also available:

```bash
just gameflow-build-push tag=<tag>
just gameflow-sync tag=<tag> nats_host=<host:4222>
just gameflow-deploy-host host=<vps-host> tag=<tag>
just gameflow-smoke-local
```

The Dockerfiles are designed for the staged multi-repo context produced by
`just edgegap-context`, not for `podman build .` from this directory.
