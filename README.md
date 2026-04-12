# Orda Installer

TUI installer for [Orda](https://joinorda.com) self-hosted servers. Handles provisioning, configuration, and service management on Linux VMs.

## Install

```bash
curl -fsSL https://get.orda.chat/install | sh
```

The bootstrap script detects your architecture, downloads the binary, caches sudo credentials, and starts the installer.

### Requirements

- Linux (amd64 or arm64)
- sudo access or root

## Usage

```
orda install [OPTIONS]
orda update [--orda-dir PATH]
orda uninstall [--orda-dir PATH] [--yes]
orda status [--orda-dir PATH]
```

### Install

Interactive TUI that walks through server setup:

1. License key validation
2. Server registration
3. Dependency installation (Docker, jq, chrony)
4. DNS propagation
5. System user and directory setup
6. TLS certificate provisioning
7. Firewall configuration (ufw + fail2ban)
8. Service configuration and launch
9. Health check verification

```bash
# Standard install
orda install

# Provide license key via flag
orda install --license-key <key>

# Dry run (works on any OS, no system changes)
orda install --dry-run

# Custom install directory
orda install --orda-dir /srv/orda
```

### Update

Pulls the latest Docker images and restarts services.

```bash
orda update
```

### Status

Shows container status, health check result, and disk usage.

```bash
orda status
```

### Uninstall

Stops services and removes the installation directory.

```bash
orda uninstall        # interactive confirmation
orda uninstall --yes  # skip confirmation
```

## What gets installed

| Component | Description |
|-----------|-------------|
| alacahoyuk | Server engine (Rust/Axum, encrypted SQLite) |
| caddy | Reverse proxy with automatic TLS |
| livekit | Voice and video (WebRTC media server) |

All services run as Docker containers under a dedicated `orda` system user.

### File layout

```
/opt/orda/
  .env                 License key, health token, LiveKit credentials
  docker-compose.yml   Service definitions
  Caddyfile            Reverse proxy configuration
  livekit.yaml         LiveKit server configuration
  data/                Encrypted database and server files
  tls/                 TLS certificate and private key
  README.txt           Quick reference (generated on install)
```

## Building from source

```bash
# Debug build
cargo build

# Release build (optimized for size)
cargo build --release

# Cross-compile for Linux (static musl binary)
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target aarch64-unknown-linux-musl
```

## Support

Orda is built and maintained by a single person. If you find it useful, a donation goes a long way toward keeping the project alive. A donation channel is being set up and will be linked here soon.

If you or your organization are interested in sponsoring or making a larger contribution before the donation channel is live, reach out at [hello@joinorda.com](mailto:hello@joinorda.com).

In the meantime, the best way to support the project is to use it, report bugs, and spread the word. If you have legal expertise, the [Privacy Policy](https://orda.chat/privacy) and [Terms of Service](https://orda.chat/terms) are open for review on [GitHub](https://github.com/orda-oss/web).

## Issues

Report bugs and feature requests at [github.com/orda-oss/installer/issues](https://github.com/orda-oss/installer/issues).
