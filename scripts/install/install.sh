#!/bin/sh
# LinkFYR installer (defensive).
# - HTTPS only, SHA-256 verification, arch detection, no silent sudo.
# - Refuses to run when piped without --yes (prevents `curl | sh` foot-guns).
# Usage: install.sh [--version v0.1.0] [--prefix ~/.linkfyr] [--yes] [--uninstall]
set -eu

REPO="PotenFYR-Studios/linkfyr"
BASE="https://github.com/${REPO}/releases"
VERSION="latest"
CHANNEL="stable"
PREFIX="${HOME}/.linkfyr"
ASSUME_YES=0

say() { printf '%s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

# Piping protection.
if [ -t 0 ] && [ -t 1 ]; then :; else
  case " $* " in
    *"--yes"*) : ;;
    *) die "refusing to run from a pipe without --yes. Download first, inspect, then run: sh install.sh --yes" ;;
  esac
fi

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --prefix) PREFIX="$2"; shift 2 ;;
    --channel) CHANNEL="$2"; shift 2 ;;
    --yes) ASSUME_YES=1; shift ;;
    --uninstall)
      rm -rf "${PREFIX:?}/bin" 2>/dev/null || true
      say "Removed ${PREFIX}/bin. Remove PATH entry manually if desired."
      exit 0 ;;
    *) die "unknown flag: $1" ;;
  esac
done

# Never escalate without asking.
SUDO=""
if [ "$(id -u)" -eq 0 ]; then
  die "running as root is not supported for user installs; use --prefix=/usr/local if you really want a system install"
fi

# Dependencies.
for tool in curl tar sha256sum uname; do
  command -v "$tool" >/dev/null 2>&1 || die "missing dependency: $tool"
done

OS="$(uname -s)"
ARCH="$(uname -m)"
case "${OS}:${ARCH}" in
  Linux:x86_64)  os=linux;  arch=x64 ;;
  Linux:aarch64|Linux:arm64) os=linux; arch=arm64 ;;
  Darwin:arm64)  os=macos;  arch=arm64 ;;
  Darwin:x86_64) os=macos;  arch=x64 ;;
  *) die "unsupported platform ${OS}:${ARCH}" ;;
esac

# Resolve version.
if [ "$VERSION" = "latest" ]; then
  VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p')"
  [ -n "$VERSION" ] || die "could not resolve latest release"
fi
VER="${VERSION#v}"

NAME="linkfyr-cli_${VER}_${os}_${arch}.tar.gz"
URL="${BASE}/download/${VERSION}/${NAME}"

# Checksums BEFORE downloading the payload.
SUMS="$(curl -fsSL "${BASE}/download/${VERSION}/SHA256SUMS")" || die "SHA256SUMS not found for ${VERSION} (we never install unverified builds)"
EXPECTED="$(printf '%s\n' "$SUMS" | grep " ${NAME}\$" | awk '{print $1}')"
[ -n "$EXPECTED" ] || die "no checksum entry for ${NAME}"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
say "Downloading ${URL}"
curl -fsSL --proto '=https' --tlsv1.2 -o "${TMP}/${NAME}" "$URL"

ACTUAL="$(sha256sum "${TMP}/${NAME}" | awk '{print $1}')"
[ "$ACTUAL" = "$EXPECTED" ] || die "checksum mismatch: expected ${EXPECTED}, got ${ACTUAL}"

mkdir -p "${PREFIX}/bin"
tar xzf "${TMP}/${NAME}" -C "${PREFIX}/bin"
chmod +x "${PREFIX}/bin/linkfyr"

case ":$PATH:" in
  *":${PREFIX}/bin:"*) : ;;
  *) say "Add to PATH:  export PATH=\"${PREFIX}/bin:\$PATH\"  (add to your shell profile)" ;;
esac

say "Installed linkfyr ${VERSION} -> ${PREFIX}/bin/linkfyr"
say "Try: linkfyr status"
