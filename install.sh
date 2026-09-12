#!/bin/bash

set -euo pipefail
IFS=$'\n\t'

# Colors for output (only when stdout is a TTY)
if [ -t 1 ]; then
  RED=$'\033[0;31m'
  GREEN=$'\033[0;32m'
  NC=$'\033[0m'  # No Color
else
  RED=''
  GREEN=''
  NC=''
fi

# Script constants
SELF_REPO="listepo/ketch"
BINARY_NAME="ketch"
DEFAULT_ROOT="${HOME}/.ketch"
DEFAULT_INSTALL_DIR="${HOME}/.ketch/bin"

# State for cleanup
TEMP_DIR=""

# Print help message
print_help() {
  cat <<EOF
Usage: install.sh [OPTIONS]

Install ketch, a Rust CLI for managing GitHub-released apps on macOS.

OPTIONS:
  --version <TAG>      Install specific version (default: latest)
  --root <DIR>         Ketch store root (default: $DEFAULT_ROOT)
  --install-dir <DIR>  Bootstrap location: where this script places a ketch
                       binary on your PATH (default: <root>/bin)
  --no-modify-path     Don't modify PATH in shell config files
  --help               Show this help message
EOF
}

# Cleanup function
cleanup() {
  if [ -n "${TEMP_DIR}" ] && [ -d "${TEMP_DIR}" ]; then
    rm -rf "${TEMP_DIR}"
  fi
}
trap cleanup EXIT

# Parse arguments. INSTALL_DIR stays empty unless given so we can tell an
# explicit --install-dir from the default <root>/bin after ROOT is resolved.
VERSION=""
ROOT=""
INSTALL_DIR=""
INSTALL_DIR_EXPLICIT=0
NO_MODIFY_PATH=0

while [ $# -gt 0 ]; do
  case "$1" in
    --version)
      shift
      VERSION="$1"
      shift
      ;;
    --root)
      shift
      ROOT="$1"
      shift
      ;;
    --install-dir)
      shift
      INSTALL_DIR="$1"
      INSTALL_DIR_EXPLICIT=1
      shift
      ;;
    --no-modify-path)
      NO_MODIFY_PATH=1
      shift
      ;;
    --help|-h)
      print_help
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      print_help
      exit 1
      ;;
  esac
done

ROOT="${ROOT:-${DEFAULT_ROOT}}"
INSTALL_DIR="${INSTALL_DIR:-${ROOT%/}/bin}"

# Both may be relative, and the script cds into a temp directory below: a
# relative path would be created inside it and deleted with it on exit, leaving
# nothing installed. Resolve them against the directory the user ran this in.
case "${ROOT}" in
  /*) ;;
  *) ROOT="${PWD}/${ROOT}" ;;
esac
case "${INSTALL_DIR}" in
  /*) ;;
  *) INSTALL_DIR="${PWD}/${INSTALL_DIR}" ;;
esac

# KETCH_ROOT is only --root (or its default), never derived from --install-dir.
KETCH_ROOT="${ROOT}"
export KETCH_ROOT

# Refuse to run as root
if [ "$(id -u)" -eq 0 ]; then
  echo "${RED}Error: Don't run this script as root.${NC}" >&2
  echo "ketch installs per-user into \$HOME; running with sudo creates root-owned files." >&2
  exit 1
fi

# Detect OS
OS="$(uname -s)"
if [ "${OS}" != "Darwin" ]; then
  echo "${RED}Error: ketch is macOS-only at the moment.${NC}" >&2
  echo "See https://github.com/${SELF_REPO}/roadmap for platform support." >&2
  exit 1
fi

# Detect architecture
ARCH="$(uname -m)"
# Check if running under Rosetta on Apple Silicon
if [ "${ARCH}" = "x86_64" ]; then
  TRANSLATED="$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)"
  if [ "${TRANSLATED}" = "1" ]; then
    # Running translated, so the real machine is arm64
    ARCH="arm64"
  fi
fi

# Map architecture to tarball name component
case "${ARCH}" in
  arm64)
    TARBALL_ARCH="aarch64"
    ;;
  x86_64)
    TARBALL_ARCH="x86_64"
    ;;
  *)
    echo "${RED}Error: Unsupported architecture: ${ARCH}${NC}" >&2
    exit 1
    ;;
esac

# Resolve version
if [ -z "${VERSION}" ]; then
  echo "Fetching latest release..."
  RELEASES_URL="https://api.github.com/repos/${SELF_REPO}/releases/latest"

  # Try curl first, fall back to wget
  if command -v curl >/dev/null 2>&1; then
    RELEASE_JSON="$(curl -fsSL "${RELEASES_URL}")" || {
      echo "${RED}Error: Failed to fetch latest release.${NC}" >&2
      exit 1
    }
  elif command -v wget >/dev/null 2>&1; then
    RELEASE_JSON="$(wget -q -O - "${RELEASES_URL}")" || {
      echo "${RED}Error: Failed to fetch latest release.${NC}" >&2
      exit 1
    }
  else
    echo "${RED}Error: curl or wget required but not found.${NC}" >&2
    exit 1
  fi

  # Parse tag_name without jq using grep/sed
  # `|| true`: a grep that matches nothing exits 1, and under `set -e` with
  # `pipefail` that would abort here instead of reaching the check below.
  VERSION="$(printf '%s\n' "${RELEASE_JSON}" | grep '"tag_name"' | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/' | head -1 || true)"

  if [ -z "${VERSION}" ]; then
    echo "${RED}Error: Could not determine latest version.${NC}" >&2
    exit 1
  fi
fi

echo "Installing ketch version ${VERSION}..."

# Create temp directory
TEMP_DIR="$(mktemp -d)" || {
  echo "${RED}Error: Failed to create temporary directory.${NC}" >&2
  exit 1
}

cd "${TEMP_DIR}"

# Determine download URLs
TARBALL_URL="https://github.com/${SELF_REPO}/releases/download/${VERSION}/ketch-${TARBALL_ARCH}-apple-darwin.tar.gz"
CHECKSUMS_URL="https://github.com/${SELF_REPO}/releases/download/${VERSION}/SHA256SUMS"

# Download tarball and checksums
echo "Downloading release assets..."
if command -v curl >/dev/null 2>&1; then
  curl -fsSL -o ketch.tar.gz "${TARBALL_URL}" || {
    echo "${RED}Error: Failed to download ${TARBALL_URL}${NC}" >&2
    exit 1
  }
  curl -fsSL -o SHA256SUMS "${CHECKSUMS_URL}" || {
    echo "${RED}Error: Failed to download checksums.${NC}" >&2
    exit 1
  }
else
  wget -q -O ketch.tar.gz "${TARBALL_URL}" || {
    echo "${RED}Error: Failed to download ${TARBALL_URL}${NC}" >&2
    exit 1
  }
  wget -q -O SHA256SUMS "${CHECKSUMS_URL}" || {
    echo "${RED}Error: Failed to download checksums.${NC}" >&2
    exit 1
  }
fi

# Verify checksum
echo "Verifying checksum..."
# shasum output: "hash  filename"
EXPECTED_HASH="$(grep "ketch-${TARBALL_ARCH}-apple-darwin.tar.gz" SHA256SUMS | awk '{print $1}' || true)"
ACTUAL_HASH="$(shasum -a 256 ketch.tar.gz | awk '{print $1}')"

if [ -z "${EXPECTED_HASH}" ]; then
  echo "${RED}Error: SHA256SUMS does not list ketch-${TARBALL_ARCH}-apple-darwin.tar.gz.${NC}" >&2
  echo "Refusing to install an unverified binary." >&2
  exit 1
fi

if [ "${EXPECTED_HASH}" != "${ACTUAL_HASH}" ]; then
  echo "${RED}Error: Checksum verification failed!${NC}" >&2
  echo "Expected: ${EXPECTED_HASH}" >&2
  echo "Actual:   ${ACTUAL_HASH}" >&2
  exit 1
fi

# Extract tarball
echo "Extracting..."
tar -xzf ketch.tar.gz

# Find the binary (it might be in a subdirectory)
BINARY_PATH=""
if [ -f "${BINARY_NAME}" ]; then
  BINARY_PATH="./${BINARY_NAME}"
elif [ -f "ketch/${BINARY_NAME}" ]; then
  BINARY_PATH="./ketch/${BINARY_NAME}"
else
  # Try to find it
  BINARY_PATH="$(find . -name "${BINARY_NAME}" -type f 2>/dev/null | head -1 || true)"
  if [ -z "${BINARY_PATH}" ]; then
    echo "${RED}Error: Could not find ${BINARY_NAME} binary in archive.${NC}" >&2
    exit 1
  fi
fi

# Create the root bin dir the installed binary will live in.
mkdir -p "${ROOT}/bin" || {
  echo "${RED}Error: Failed to create install directory: ${ROOT}/bin${NC}" >&2
  exit 1
}

# Check if this is an upgrade
INSTALL_PATH="${ROOT}/bin/${BINARY_NAME}"
if [ -e "${INSTALL_PATH}" ]; then
  echo "Upgrading ketch..."
else
  echo "Installing ketch..."
fi

# Let ketch install itself. The downloaded binary is only used to run
# `self install`, which fetches this same release again through ketch's own
# pipeline: verified against SHA256SUMS, unpacked into the store, linked from
# the bin dir and recorded like any other package, so `ketch list` shows it
# and `ketch self update` is an ordinary upgrade.
chmod 755 "${BINARY_PATH}"
xattr -d com.apple.quarantine "${BINARY_PATH}" 2>/dev/null || true
"${BINARY_PATH}" self install || {
  echo "${RED}Error: ketch could not install itself into ${ROOT}.${NC}" >&2
  exit 1
}

# A bootstrap dir outside <root>/bin gets a link to the installed binary — a
# link and not a copy, because `ketch self update` replaces `<root>/bin/ketch`
# in place: a copy would go on running the version it was made from. `ln -s`
# also works across filesystems, where a hard link would not.
if [ "${INSTALL_DIR_EXPLICIT}" -eq 1 ]; then
  mkdir -p "${INSTALL_DIR}" || {
    echo "${RED}Error: Failed to create bootstrap directory: ${INSTALL_DIR}${NC}" >&2
    exit 1
  }
  # Compare where the two directories really are. `/tmp/x/bin` and
  # `/private/tmp/x/bin`, a doubled slash and a `./` in the middle all name one
  # directory, and a string compare says otherwise — then the link below points
  # the installed binary at itself and takes the install with it.
  INSTALL_DIR="$(cd "${INSTALL_DIR}" && pwd -P)"
  BIN_DIR="$(cd "${ROOT%/}/bin" && pwd -P)"
  if [ "${INSTALL_DIR}" != "${BIN_DIR}" ]; then
    BOOTSTRAP_PATH="${INSTALL_DIR}/${BINARY_NAME}"
    if ! ln -sfn "${INSTALL_PATH}" "${BOOTSTRAP_PATH}"; then
      cp "${INSTALL_PATH}" "${BOOTSTRAP_PATH}" || {
        echo "${RED}Error: Failed to place ${BINARY_NAME} in ${INSTALL_DIR}.${NC}" >&2
        exit 1
      }
    fi
    chmod 755 "${BOOTSTRAP_PATH}"
  fi
fi

# Wire up PATH. ketch owns this: `ketch path install` knows bash, zsh and fish,
# quotes the directory properly, and can undo itself — which is more than this
# script should be reimplementing.
PATH_SET=0
if [ "${NO_MODIFY_PATH}" -eq 0 ]; then
  echo "Setting up PATH..."
  if "${INSTALL_PATH}" path install; then
    PATH_SET=1
  else
    echo "${RED}Could not set up PATH automatically.${NC}" >&2
  fi
fi

# Success message
echo ""
echo "${GREEN}✓ ketch ${VERSION} installed successfully!${NC}"
echo ""
echo "Installed to: ${INSTALL_PATH}"
echo ""

if [ "${PATH_SET}" -eq 1 ]; then
  echo "PATH updated. Open a new shell, or run:"
  echo "  ${GREEN}exec \$SHELL${NC}"
else
  echo "To use ketch, add ${ROOT}/bin to your PATH:"
  echo "  ${GREEN}${INSTALL_PATH} path install${NC}"
fi

echo ""
echo "Getting started:"
"${INSTALL_PATH}" --help
