#!/bin/sh
set -eu

# Lokal installer bootstrap
# Usage: curl -fsSL https://get.<placeholder>/install | sh

REPO="rwxdash/lokal-installer"
BINARY_NAME="lokal"
INSTALL_DIR="/usr/local/bin"

main() {
    OS="$(uname -s)"
    case "$OS" in
        Linux) ;;
        *) printf 'Error: Lokal requires Linux. Detected: %s\n' "$OS" >&2; exit 1 ;;
    esac

    ARCH="$(uname -m)"
    case "$ARCH" in
        x86_64)  ARCH="amd64" ;;
        aarch64) ARCH="arm64" ;;
        arm64)   ARCH="arm64" ;;
        *) printf 'Error: Unsupported architecture: %s\n' "$ARCH" >&2; exit 1 ;;
    esac

    # Cache sudo credentials early (single password prompt)
    SUDO=""
    if [ "$(id -u)" -ne 0 ]; then
        if command -v sudo >/dev/null 2>&1; then
            printf 'Root access required for installation. You may be prompted for your password.\n'
            sudo true || { printf 'Error: sudo authentication failed.\n' >&2; exit 1; }
            SUDO="sudo"
        else
            printf 'Error: Not running as root and sudo is not available.\n' >&2
            exit 1
        fi
    fi

    if command -v curl >/dev/null 2>&1; then
        FETCH="curl -fsSL -o"
    elif command -v wget >/dev/null 2>&1; then
        FETCH="wget -qO"
    else
        printf 'Error: Neither curl nor wget found.\n' >&2
        exit 1
    fi

    RELEASE_URL="https://github.com/$REPO/releases/latest/download/${BINARY_NAME}-linux-${ARCH}"

    printf 'Downloading %s for linux/%s...\n' "$BINARY_NAME" "$ARCH"
    TMP=$(mktemp)

    $FETCH "$TMP" "$RELEASE_URL" || {
        rm -f "$TMP"
        printf 'Error: Failed to download from %s\n' "$RELEASE_URL" >&2
        exit 1
    }

    # Verify ELF binary (catches HTML error pages)
    if ! head -c 4 "$TMP" | grep -q "ELF"; then
        rm -f "$TMP"
        printf 'Error: Downloaded file is not a valid binary.\n' >&2
        exit 1
    fi

    $SUDO install -m 755 "$TMP" "$INSTALL_DIR/$BINARY_NAME"
    rm -f "$TMP"

    printf 'Installed %s to %s/%s\n' "$BINARY_NAME" "$INSTALL_DIR" "$BINARY_NAME"

    # Run the installer (sudo ticket is already cached)
    exec "$INSTALL_DIR/$BINARY_NAME" install "$@"
}

main "$@"
