#!/usr/bin/env sh
# shellcheck shell=dash

# Installer for smista <https://smista.ai>
#
# Options
#
#   -y, -f, --yes, --force
#     Skip the confirmation prompt during installation
#
#   -v=VERSION, --version=VERSION, --version VERSION
#     Install a specific version instead of the latest release
#     (skips the Homebrew path, which always tracks the latest release)
#
#   -h, --help
#     Show usage
#
# Environment
#
#   BIN_DIR   - directory the binary is installed into (default: /usr/local/bin)
#   PLATFORM  - override platform detection (linux, macos)
#   ARCH      - override architecture detection (x86_64, aarch64)

set -eu

GITHUB_REPO="smista-ai/smista.ai"
GITHUB_URL="https://github.com/${GITHUB_REPO}"
ISSUES_URL="${GITHUB_URL}/issues/new"
BREW_FORMULA="smista-ai/smista/smista"
VERSION=""
FORCE=""
SUDO=""

printf '\n'

BOLD="$(tput bold 2>/dev/null || printf '')"
GREY="$(tput setaf 0 2>/dev/null || printf '')"
UNDERLINE="$(tput smul 2>/dev/null || printf '')"
RED="$(tput setaf 1 2>/dev/null || printf '')"
GREEN="$(tput setaf 2 2>/dev/null || printf '')"
YELLOW="$(tput setaf 3 2>/dev/null || printf '')"
BLUE="$(tput setaf 4 2>/dev/null || printf '')"
MAGENTA="$(tput setaf 5 2>/dev/null || printf '')"
NO_COLOR="$(tput sgr0 2>/dev/null || printf '')"

info() {
    printf '%s\n' "${BOLD}${GREY}>${NO_COLOR} $*"
}

warn() {
    printf '%s\n' "${YELLOW}! $*${NO_COLOR}"
}

error() {
    printf '%s\n' "${RED}x $*${NO_COLOR}" >&2
}

completed() {
    printf '%s\n' "${GREEN}✓${NO_COLOR} $*"
}

has() {
    command -v "$1" 1>/dev/null 2>&1
}

usage() {
    cat <<EOF
smista installer <https://smista.ai>

Downloads and installs the latest smista release for your platform, using
Homebrew when available.

USAGE:
    curl -sSLf https://smista.ai/install/install.sh | sh -s -- [OPTIONS]

OPTIONS:
    -y, -f, --yes, --force       Skip the confirmation prompt
    -v=X.Y.Z, --version=X.Y.Z    Install a specific version (skips Homebrew)
    -h, --help                   Show this help

ENVIRONMENT:
    BIN_DIR     Directory the binary is installed into (default: /usr/local/bin)
    PLATFORM    Override platform detection (linux, macos)
    ARCH        Override architecture detection (x86_64, aarch64)
EOF
}

confirm() {
    if [ -z "${FORCE}" ]; then
        printf "%s " "${MAGENTA}?${NO_COLOR} $* ${BOLD}[y/N]${NO_COLOR}"
        set +e
        read -r yn </dev/tty
        rc=$?
        set -e
        if [ ${rc} -ne 0 ]; then
            error "Error reading from prompt (please re-run with the '--yes' option)"
            exit 1
        fi
        if [ "${yn}" != "y" ] && [ "${yn}" != "yes" ]; then
            error 'Aborting (please answer "yes" to continue)'
            exit 1
        fi
    fi
}

download() {
    output="$1"
    url="$2"

    if has curl; then
        set -- curl --fail --silent --location --output "${output}" "${url}"
    elif has wget; then
        set -- wget --quiet --output-document="${output}" "${url}"
    elif has fetch; then
        set -- fetch --quiet --output="${output}" "${url}"
    else
        error "No HTTP download program (curl, wget, fetch) found, exiting…"
        return 1
    fi

    "$@" && return 0
    rc=$?
    error "Download failed (exit code ${rc}): ${BLUE}$*${NO_COLOR}"
    return "${rc}"
}

# Currently supporting:
#   - macos
#   - linux
detect_platform() {
    platform="$(uname -s | tr '[:upper:]' '[:lower:]')"

    case "${platform}" in
        darwin) platform="macos" ;;
        *) ;;
    esac

    printf '%s' "${platform}"
}

# Currently supporting:
#   - x86_64
#   - aarch64
detect_arch() {
    arch="$(uname -m | tr '[:upper:]' '[:lower:]')"

    case "${arch}" in
        amd64) arch="x86_64" ;;
        arm64) arch="aarch64" ;;
        *) ;;
    esac

    # `uname -m` may report a 64-bit kernel on a 32-bit userland
    if [ "$(getconf LONG_BIT 2>/dev/null || printf '64')" -eq 32 ]; then
        arch="${arch}-32bit"
    fi

    printf '%s' "${arch}"
}

unsupported() {
    error "$1"
    info "On Windows, use the PowerShell installer instead: irm https://smista.ai/install/install.ps1 | iex"
    info "Alternatively you can install smista with Cargo <https://www.rust-lang.org/tools/install>: cargo install smista --locked"
    exit 1
}

resolve_version() {
    if [ -n "${VERSION}" ]; then
        return 0
    fi
    info "Resolving the latest smista version…"
    api_response="${TMP_DIR}/latest-release.json"
    if ! download "${api_response}" "https://api.github.com/repos/${GITHUB_REPO}/releases/latest"; then
        error "Could not query the latest smista release."
        warn "If no release has been published yet, pass a version explicitly with '--version=X.Y.Z'."
        warn "If you believe this is a bug, please report an issue at <${ISSUES_URL}>"
        exit 1
    fi
    VERSION="$(sed -n 's/.*"tag_name":[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' "${api_response}" | head -n 1)"
    if [ -z "${VERSION}" ]; then
        error "Could not parse the latest smista version from the GitHub API response."
        warn "Pass a version explicitly with '--version=X.Y.Z', or report an issue at <${ISSUES_URL}>"
        exit 1
    fi
}

sha256_of() {
    file="$1"
    if has sha256sum; then
        sha256sum "${file}" | awk '{print $1}'
    elif has shasum; then
        shasum -a 256 "${file}" | awk '{print $1}'
    elif has openssl; then
        openssl dgst -sha256 "${file}" | awk '{print $NF}'
    else
        printf ''
    fi
}

verify_checksum() {
    archive="$1"
    checksum_file="$2"
    expected="$(tr -d '[:space:]' < "${checksum_file}")"
    actual="$(sha256_of "${archive}")"
    if [ -z "${actual}" ]; then
        warn "No SHA-256 tool found (sha256sum, shasum, openssl); skipping checksum verification."
        return 0
    fi
    if [ "${expected}" != "${actual}" ]; then
        error "Checksum mismatch for the downloaded archive (expected ${expected}, got ${actual})."
        error "Please retry, and report an issue at <${ISSUES_URL}> if the problem persists."
        exit 1
    fi
    info "Checksum verified"
}

test_writeable() {
    path="${1:-}/.smista-install-test"
    if touch "${path}" 2>/dev/null; then
        rm "${path}"
        return 0
    else
        return 1
    fi
}

elevate_priv() {
    if has sudo; then
        if ! sudo -v; then
            error "Superuser not granted, aborting installation"
            exit 1
        fi
        SUDO="sudo"
    elif has doas; then
        SUDO="doas"
    else
        error "Could not find \"sudo\" or \"doas\", needed to install smista into ${BIN_DIR}."
        info "Re-run this script as root, or set BIN_DIR to a writeable directory."
        exit 1
    fi
}

install_with_brew() {
    if brew list smista >/dev/null 2>&1; then
        info "smista is already installed with Homebrew; upgrading…"
        brew update
        brew upgrade "${BREW_FORMULA}" || info "smista is already up to date"
    else
        info "Installing ${GREEN}smista${NO_COLOR} with Homebrew…"
        brew install "${BREW_FORMULA}"
    fi
}

install_from_release() {
    case "${PLATFORM}" in
        linux) target="${ARCH}-unknown-linux-musl" ;;
        macos) target="${ARCH}-apple-darwin" ;;
        *) unsupported "${PLATFORM} is not supported by this installer." ;;
    esac

    asset="smista-v${VERSION}-${target}.tar.gz"
    url="${GITHUB_URL}/releases/download/v${VERSION}/${asset}"
    archive="${TMP_DIR}/${asset}"

    info "Downloading ${GREEN}smista v${VERSION}${NO_COLOR} from ${url} …"
    if ! download "${archive}" "${url}"; then
        error "Failed to download ${asset}."
        warn "Check that release v${VERSION} exists and provides artifacts for ${target}."
        warn "If you believe this is a bug, please report an issue at <${ISSUES_URL}>"
        exit 1
    fi

    if download "${archive}.sha256" "${url}.sha256"; then
        verify_checksum "${archive}" "${archive}.sha256"
    else
        warn "Could not download the checksum file; skipping checksum verification."
    fi

    info "Extracting archive …"
    tar -xzf "${archive}" -C "${TMP_DIR}"
    if [ ! -f "${TMP_DIR}/smista" ]; then
        error "The smista binary was not found in the downloaded archive."
        warn "Please report an issue at <${ISSUES_URL}>"
        exit 1
    fi

    if ! test_writeable "${BIN_DIR}"; then
        warn "Root permissions are required to install smista into ${BIN_DIR} …"
        elevate_priv
    fi
    info "Installing ${GREEN}smista${NO_COLOR} to ${BIN_DIR} …"
    ${SUDO} mkdir -p "${BIN_DIR}"
    ${SUDO} install -m 755 "${TMP_DIR}/smista" "${BIN_DIR}/smista"
}

# -- main --------------------------------------------------------------------

if [ -z "${PLATFORM-}" ]; then
    PLATFORM="$(detect_platform)"
fi

if [ -z "${ARCH-}" ]; then
    ARCH="$(detect_arch)"
fi

if [ -z "${BIN_DIR-}" ]; then
    BIN_DIR="/usr/local/bin"
fi

while [ "$#" -gt 0 ]; do
    case "$1" in
        -y | -f | --yes | --force)
            FORCE=1
            shift 1
            ;;
        -v=* | --version=*)
            VERSION="${1#*=}"
            shift 1
            ;;
        --version)
            if [ "$#" -lt 2 ]; then
                error "--version requires an argument"
                exit 1
            fi
            VERSION="$2"
            shift 2
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

# Homebrew always installs the latest release, so an explicit version request
# falls back to the plain binary download.
if has brew && [ -z "${VERSION}" ]; then
    METHOD="brew"
else
    METHOD="release"
fi

if [ "${METHOD}" = "release" ]; then
    case "${PLATFORM}" in
        linux | macos) ;;
        *) unsupported "${PLATFORM} is not supported by this installer." ;;
    esac
    case "${ARCH}" in
        x86_64 | aarch64) ;;
        *) unsupported "Unsupported architecture: ${ARCH}." ;;
    esac
    resolve_version
fi

printf '  %s\n' "${UNDERLINE}smista configuration${NO_COLOR}"
info "${BOLD}Method${NO_COLOR}:    ${GREEN}${METHOD}${NO_COLOR}"
info "${BOLD}Platform${NO_COLOR}:  ${GREEN}${PLATFORM}${NO_COLOR}"
info "${BOLD}Arch${NO_COLOR}:      ${GREEN}${ARCH}${NO_COLOR}"
if [ "${METHOD}" = "release" ]; then
    info "${BOLD}Version${NO_COLOR}:   ${GREEN}${VERSION}${NO_COLOR}"
    info "${BOLD}Bin dir${NO_COLOR}:   ${GREEN}${BIN_DIR}${NO_COLOR}"
fi
printf '\n'

confirm "Install ${GREEN}smista${NO_COLOR}?"

if [ "${METHOD}" = "brew" ]; then
    install_with_brew
else
    install_from_release
fi

completed "smista has successfully been installed on your system!"
info "Get started with the user guide <https://docs.smista.ai>"
info "If you encounter any issue, please report it at <${ISSUES_URL}>"

exit 0
