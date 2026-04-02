use std::path::Path;

pub fn render_env(
    license_key: &str,
    semerkant_url: &str,
    health_token: &str,
    lk_api_key: &str,
    lk_api_secret: &str,
) -> String {
    format!(
        "\
LICENSE_KEY={license_key}
SEMERKANT_URL={semerkant_url}
HEALTH_TOKEN={health_token}
LIVEKIT_URL=livekit:7880
LIVEKIT_API_KEY={lk_api_key}
LIVEKIT_API_SECRET={lk_api_secret}
LIVEKIT_WEBHOOK_URL=http://alacahoyuk:3000/voice/webhook
CADDY_ADMIN_URL=http://caddy:2019
"
    )
}

pub fn render_livekit_yaml(lk_api_key: &str, lk_api_secret: &str) -> String {
    format!(
        "\
port: 7880
rtc:
  port_range_start: 50000
  port_range_end: 51000
  tcp_port: 7881

keys:
  {lk_api_key}: {lk_api_secret}

webhook:
  api_key: {lk_api_key}
  urls:
    - http://alacahoyuk:3000/voice/webhook
"
    )
}

pub fn render_caddyfile(domain: &str) -> String {
    format!(
        "\
{{
    admin 0.0.0.0:2019
}}

{domain} {{
    tls /etc/caddy/tls/cert.pem /etc/caddy/tls/key.pem
    handle_path /lk/* {{
        reverse_proxy livekit:7880
    }}
    reverse_proxy alacahoyuk:3000
}}
"
    )
}

pub fn render_docker_compose(image: &str, uid: u32, gid: u32) -> String {
    format!(
        "\
services:
  alacahoyuk:
    image: {image}
    pull_policy: always
    user: \"{uid}:{gid}\"
    restart: unless-stopped
    env_file: .env
    volumes:
      - ./data:/opt/alacahoyuk/data
      - ./tls:/opt/alacahoyuk/tls
    expose:
      - \"3000\"

  livekit:
    image: livekit/livekit-server:latest
    pull_policy: always
    restart: unless-stopped
    volumes:
      - ./livekit.yaml:/etc/livekit.yaml:ro
    ports:
      - \"7881:7881\"
      - \"50000-51000:50000-51000/udp\"
    command: --config /etc/livekit.yaml

  caddy:
    image: caddy:2-alpine
    pull_policy: always
    restart: unless-stopped
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile:ro
      - ./tls:/etc/caddy/tls:ro
      - caddy_data:/data
    ports:
      - \"80:80\"
      - \"443:443\"
    depends_on:
      - alacahoyuk
      - livekit

volumes:
  caddy_data:
"
    )
}

pub fn render_readme(server_address: &str, lokal_dir: &Path) -> String {
    let dir = lokal_dir.display();
    format!(
        "\
Lokal Server
============

Public IP:  {server_address}
Directory:  {dir}

Services
--------
alacahoyuk          Server engine (Rust/Axum, SQLite with SQLCipher)
caddy               Reverse proxy, TLS termination, auto-HTTPS
livekit             Voice and video (WebRTC media server)

Files
-----
.env                License key, health token, LiveKit credentials (chmod 600)
docker-compose.yml  Service definitions (alacahoyuk, caddy, livekit)
Caddyfile           Reverse proxy rules and TLS certificate paths
livekit.yaml        LiveKit server configuration and webhook URLs
data/               Encrypted database and server files
tls/                TLS certificate and private key (managed by central server)

Commands
--------
Status:             cd {dir} && docker compose ps
Logs:               cd {dir} && docker compose logs -f
Logs (service):     cd {dir} && docker compose logs -f alacahoyuk
Restart:            cd {dir} && docker compose restart
Pull + restart:     cd {dir} && docker compose pull && docker compose up -d
Stop:               cd {dir} && docker compose down
Start:              cd {dir} && docker compose up -d
Update:             lokal update
Uninstall:          lokal uninstall

Ports
-----
80/tcp              HTTP (redirects to HTTPS)
443/tcp             HTTPS (Caddy reverse proxy -> alacahoyuk:3000)
7881/tcp            LiveKit TCP fallback (used when UDP is blocked by client network)
50000-51000/udp     LiveKit media (WebRTC audio/video, ~500 concurrent participants)

Backups
-------
Server data lives in {dir}/data/. Back it up with your VM provider's
snapshot feature, or manually:

  tar czf lokal-backup-$(date +%Y%m%d).tar.gz -C {dir} data/

TLS certificates are managed by the central server and rotated
automatically. You do not need to back them up.

Troubleshooting
---------------
Check service health:
  curl -s https://localhost/health -k

View recent alacahoyuk logs:
  cd {dir} && docker compose logs --tail 50 alacahoyuk

Restart a single service:
  cd {dir} && docker compose restart alacahoyuk

Report issues:
  https://github.com/rwxdash/lokal-installer
"
    )
}
