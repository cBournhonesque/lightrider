#!/usr/bin/env bash
set -euo pipefail

web_port="${WEB_PORT:-8080}"
app_name="${LIGHTRIDER_MATCHMAKER_GAME:-${EDGEGAP_APP_NAME:-lightrider}}"
app_version="${LIGHTRIDER_MATCHMAKER_VERSION:-${EDGEGAP_APP_VERSION:-dev}}"
matchmaker_url="${LIGHTRIDER_MATCHMAKER_URL:-}"
matchmaker_upstream="${LIGHTRIDER_WEB_MATCHMAKER_UPSTREAM:-}"

js_string() {
  local value="${1:-}"
  value="${value//\\/\\\\}"
  value="${value//\"/\\\"}"
  value="${value//$'\n'/\\n}"
  printf '"%s"' "$value"
}

mkdir -p /run/nginx /var/log/nginx

cat > /usr/share/nginx/html/bootstrap.js <<BOOTSTRAP
window.LIGHTRIDER_BOOTSTRAP = {
  matchmaker_url: $(js_string "$matchmaker_url"),
  matchmaker_game: $(js_string "$app_name"),
  matchmaker_version: $(js_string "$app_version")
};
BOOTSTRAP

matchmaker_location=""
if [[ -n "$matchmaker_upstream" ]]; then
  matchmaker_location=$(cat <<NGINX
    location /matchmaker/ {
        proxy_pass ${matchmaker_upstream};
        proxy_http_version 1.1;
        proxy_set_header Upgrade \$http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host \$host;
        proxy_set_header X-Forwarded-For \$proxy_add_x_forwarded_for;
        proxy_read_timeout 300s;
    }

NGINX
)
fi

rm -f /etc/nginx/sites-enabled/default /etc/nginx/conf.d/default.conf
cat > /etc/nginx/conf.d/default.conf <<NGINX
server {
    listen ${web_port};
    server_name _;
    root /usr/share/nginx/html;
    index index.html;

    gzip on;
    gzip_static on;
    gzip_vary on;
    gzip_proxied any;
    gzip_comp_level 6;
    gzip_min_length 1024;
    gzip_types
        application/javascript
        application/wasm
        application/octet-stream
        application/json
        image/svg+xml
        text/css
        text/javascript
        text/plain;

${matchmaker_location}    location / {
        try_files \$uri \$uri/ /index.html;
    }

    location = /index.html {
        add_header Cache-Control "no-cache";
    }

    location = /bootstrap.js {
        add_header Cache-Control "no-cache";
    }

    location ~* ^/pkg/.+\\.wasm$ {
        add_header Cache-Control "public, max-age=31536000, immutable";
    }

    location /pkg/ {
        add_header Cache-Control "no-cache";
    }

    location /assets/ {
        add_header Cache-Control "public, max-age=86400";
    }
}
NGINX

nginx -g "daemon off;"
