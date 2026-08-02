#!/bin/bash
# Racing Wheel Suite Linux Installation Script

set -euo pipefail

# Directory holding this script. It is the root of an extracted release
# tarball when installing from a package, and packaging/linux/ when running
# from a source checkout. Assets are located relative to it so the script
# works in both layouts and from any working directory.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Resolve a packaged asset by trying each candidate path under SCRIPT_DIR.
# Echoes the first match; returns non-zero when none exist.
find_asset() {
    local candidate
    for candidate in "$@"; do
        if [ -f "$SCRIPT_DIR/$candidate" ]; then
            echo "$SCRIPT_DIR/$candidate"
            return 0
        fi
    done
    return 1
}

# Configuration
INSTALL_PREFIX="${INSTALL_PREFIX:-$HOME/.local}"
# $USER is not always exported (containers, some sudo and cron contexts), and
# `set -u` would abort on it, so fall back to the effective user name.
SERVICE_USER="${SERVICE_USER:-${USER:-$(id -un)}}"
SKIP_UDEV="${SKIP_UDEV:-false}"
SKIP_RTKIT="${SKIP_RTKIT:-false}"
SKIP_SYSTEMD="${SKIP_SYSTEMD:-false}"
UNINSTALL="false"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

check_dependencies() {
    log_info "Checking system dependencies..."
    
    # Check for required system packages
    local missing_packages=()
    
    if [ "$SKIP_SYSTEMD" != "true" ] && ! command -v systemctl &> /dev/null; then
        missing_packages+=("systemd")
    fi
    
    if ! command -v udevadm &> /dev/null && [ "$SKIP_UDEV" != "true" ]; then
        missing_packages+=("udev")
    fi
    
    # Check for rtkit (optional but recommended)
    if ! command -v rtkit-daemon &> /dev/null && [ "$SKIP_RTKIT" != "true" ]; then
        log_warn "rtkit-daemon not found. Real-time scheduling may not work optimally."
        log_warn "Install rtkit package for best performance, or set SKIP_RTKIT=true"
    fi
    
    if [ ${#missing_packages[@]} -ne 0 ]; then
        log_error "Missing required packages: ${missing_packages[*]}"
        log_error "Please install them using your distribution's package manager"
        exit 1
    fi
    
    # Check user groups
    if ! groups "$SERVICE_USER" | grep -q "input"; then
        log_warn "User $SERVICE_USER is not in 'input' group"
        log_warn "Add user to input group: sudo usermod -a -G input $SERVICE_USER"
    fi
    
    if ! groups "$SERVICE_USER" | grep -q "plugdev"; then
        log_warn "User $SERVICE_USER is not in 'plugdev' group (if it exists)"
        log_warn "This may be required on some distributions"
    fi
}

install_binaries() {
    log_info "Installing binaries to $INSTALL_PREFIX/bin..."
    
    mkdir -p "$INSTALL_PREFIX/bin"
    mkdir -p "$INSTALL_PREFIX/share/racing-wheel-suite"
    mkdir -p "$INSTALL_PREFIX/share/openracing/contract/control-stream"
    mkdir -p "$INSTALL_PREFIX/share/doc/racing-wheel-suite"
    
    # Binaries live in bin/ inside an extracted tarball; fall back to the
    # script directory itself so a hand-assembled layout still works. Both
    # candidates are anchored to SCRIPT_DIR rather than the caller's working
    # directory, so the installer works from anywhere.
    local bin_source="$SCRIPT_DIR"
    if [ -d "$SCRIPT_DIR/bin" ]; then
        bin_source="$SCRIPT_DIR/bin"
    fi

    for binary in wheeld wheelctl; do
        if [ -f "$bin_source/$binary" ]; then
            cp "$bin_source/$binary" "$INSTALL_PREFIX/bin/"
            chmod +x "$INSTALL_PREFIX/bin/$binary"
            log_info "Installed $binary"
        else
            log_error "Binary $binary not found in $bin_source/"
            exit 1
        fi
    done
    
    # Optional binaries
    for binary in openracing; do
        if [ -f "$bin_source/$binary" ]; then
            cp "$bin_source/$binary" "$INSTALL_PREFIX/bin/"
            chmod +x "$INSTALL_PREFIX/bin/$binary"
            log_info "Installed $binary (optional)"
        fi
    done

    # Install the versioned external control-stream contract beside the
    # binaries. Keep the allowlist explicit so unrelated files from a package
    # cannot become part of the installed consumer surface.
    local contract_source="$SCRIPT_DIR/contract/control-stream"
    local contract_target="$INSTALL_PREFIX/share/openracing/contract/control-stream"
    for asset in control-stream-contract.json wheel.proto sample-capture.json SHA256SUMS; do
        if [ -f "$contract_source/$asset" ]; then
            cp "$contract_source/$asset" "$contract_target/"
        else
            log_error "Required control-stream contract asset not found: $contract_source/$asset"
            exit 1
        fi
    done
    
    # Install configuration templates
    mkdir -p "$INSTALL_PREFIX/share/racing-wheel-suite/config"
    # `cp -r dir/*` would fail under `set -e` when the directory is empty,
    # because the glob stays unexpanded; `dir/.` with an emptiness guard does
    # not have that problem.
    if [ -d "$SCRIPT_DIR/config" ] && [ -n "$(ls -A "$SCRIPT_DIR/config")" ]; then
        cp -r "$SCRIPT_DIR/config/." "$INSTALL_PREFIX/share/racing-wheel-suite/config/"
    fi

    # Install documentation. The tarball puts these under docs/, so the
    # find_asset lookup below covers both that and a flat layout, and copies
    # the changelog and both license files rather than just README/LICENSE.
    local doc
    for doc in README.md CHANGELOG.md LICENSE LICENSE-MIT LICENSE-APACHE; do
        local doc_src
        if doc_src="$(find_asset "docs/$doc" "$doc")"; then
            cp "$doc_src" "$INSTALL_PREFIX/share/doc/racing-wheel-suite/"
        fi
    done
}

install_systemd_service() {
    if [ "$SKIP_SYSTEMD" = "true" ]; then
        log_info "Skipping systemd user service installation"
        return
    fi
    log_info "Installing systemd user service..."
    
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"
    
    # Generate service file from template. The tarball ships it under
    # systemd/; a source checkout keeps it next to this script.
    # Covers the tarball layout under systemd/ and a source checkout, and both
    # the openracing.service and wheeld.service unit filenames that packages
    # have shipped.
    local template
    if ! template="$(find_asset \
        "systemd/openracing.service" \
        "systemd/wheeld.service" \
        "wheeld.service.template" \
        "packaging/linux/wheeld.service.template")"; then
        log_error "Could not find the wheeld service template next to $SCRIPT_DIR"
        exit 1
    fi

    # INSTALL_PREFIX is user supplied. In a sed replacement `&` expands to the
    # matched text and `|` would close the expression, so a prefix such as
    # /opt/R&D would render a broken ExecStart. Escape both, plus backslashes.
    local escaped_prefix
    escaped_prefix="$(printf '%s' "$INSTALL_PREFIX" | sed -e 's/[\\&|]/\\&/g')"

    local service_file="$service_dir/openracing.service"
    sed "s|%INSTALL_PATH%|$escaped_prefix|g" "$template" > "$service_file"

    log_info "Wrote $service_file"

    # A systemd user bus is not always reachable (plain SSH sessions without
    # lingering, containers, chroots). The unit file is already in place, so
    # report the remaining commands instead of aborting the whole install.
    if ! systemctl --user daemon-reload 2>/dev/null; then
        log_warn "No systemd user session available, so the unit was not enabled."
        log_warn "Once you have a user session, run:"
        log_warn "  systemctl --user daemon-reload"
        log_warn "  systemctl --user enable --now openracing.service"
        log_warn "To keep it running without an active login: sudo loginctl enable-linger $SERVICE_USER"
        return 0
    fi

    if ! systemctl --user enable openracing.service; then
        log_warn "Could not enable openracing.service; enable it manually with:"
        log_warn "  systemctl --user enable --now openracing.service"
        return 0
    fi

    log_info "Systemd service installed and enabled"
    log_info "Start with: systemctl --user start openracing.service"
}

install_udev_rules() {
    if [ "$SKIP_UDEV" = "true" ]; then
        log_info "Skipping udev rules installation"
        return
    fi
    
    log_info "Installing udev rules..."
    
    local udev_rules_file="/etc/udev/rules.d/99-racing-wheel-suite.rules"
    local modprobe_conf="/etc/modprobe.d/90-racing-wheel-quirks.conf"
    local hwdb_file="/etc/udev/hwdb.d/99-racing-wheel-suite.hwdb"

    # These sit at the tarball root and in packaging/linux/ alike, so a single
    # SCRIPT_DIR lookup covers both layouts.
    local rules_src quirks_src hwdb_src
    if ! rules_src="$(find_asset "99-racing-wheel-suite.rules")"; then
        log_error "Could not find 99-racing-wheel-suite.rules next to $SCRIPT_DIR"
        exit 1
    fi
    quirks_src="$(find_asset "90-racing-wheel-quirks.conf")" || quirks_src=""
    hwdb_src="$(find_asset "99-racing-wheel-suite.hwdb")" || hwdb_src=""

    if [ "$EUID" -eq 0 ]; then
        # Running as root
        cp "$rules_src" "$udev_rules_file"
        if [ -n "$quirks_src" ]; then
            cp "$quirks_src" "$modprobe_conf"
        fi
        if [ -n "$hwdb_src" ]; then
            cp "$hwdb_src" "$hwdb_file"
        fi
        systemd-hwdb update
        udevadm control --reload-rules
        udevadm trigger
        log_info "udev rules installed system-wide"
        log_info "hwdb entries installed (joystick classification for racing peripherals)"
        log_info "HID quirks (modprobe.d) installed — reboot or reload usbhid for Asetek wheels"
    else
        # Not running as root - provide instructions
        log_warn "Not running as root. udev rules need to be installed manually:"
        log_warn "sudo cp $rules_src $udev_rules_file"
        if [ -n "$quirks_src" ]; then
            log_warn "sudo cp $quirks_src $modprobe_conf"
        fi
        if [ -n "$hwdb_src" ]; then
            log_warn "sudo cp $hwdb_src $hwdb_file"
        fi
        log_warn "sudo systemd-hwdb update"
        log_warn "sudo udevadm control --reload-rules"
        log_warn "sudo udevadm trigger"
        log_warn "Reboot (or reload usbhid) for Asetek wheel quirks to take effect"
    fi
}

setup_directories() {
    log_info "Setting up user directories..."
    
    local config_dir="$HOME/.config/racing-wheel-suite"
    local data_dir="$HOME/.local/share/racing-wheel-suite"
    local cache_dir="$HOME/.cache/racing-wheel-suite"
    
    mkdir -p "$config_dir"/{profiles,plugins}
    mkdir -p "$data_dir"/{logs,blackbox}
    mkdir -p "$cache_dir"
    
    # Set appropriate permissions
    chmod 700 "$config_dir"
    chmod 755 "$data_dir"
    chmod 755 "$cache_dir"
    
    log_info "Created configuration directories"
}

verify_installation() {
    log_info "Verifying installation..."
    
    # The installed files are what this script is responsible for. Whether the
    # prefix happens to be on PATH is the user's shell configuration, so it is
    # reported as a warning rather than failing an otherwise good install.
    for binary in wheeld wheelctl; do
        if [ ! -x "$INSTALL_PREFIX/bin/$binary" ]; then
            log_error "$INSTALL_PREFIX/bin/$binary was not installed"
            return 1
        fi
    done

    for binary in wheeld wheelctl; do
        if ! command -v "$binary" &> /dev/null; then
            log_warn "$binary is installed but not on your PATH."
            log_warn "Add it with: export PATH=\"$INSTALL_PREFIX/bin:\$PATH\""
            break
        fi
    done

    for asset in control-stream-contract.json wheel.proto sample-capture.json SHA256SUMS; do
        if [ ! -f "$INSTALL_PREFIX/share/openracing/contract/control-stream/$asset" ]; then
            log_error "Installed control-stream contract asset is missing: $asset"
            return 1
        fi
    done

    # Check the unit file on disk rather than asking systemd. `systemctl --user
    # list-unit-files` needs a reachable user bus, which is exactly what
    # install_systemd_service tolerates the absence of, so querying it here
    # would fail an install that actually succeeded.
    if [ "$SKIP_SYSTEMD" != "true" ] \
        && [ ! -f "$HOME/.config/systemd/user/openracing.service" ]; then
        log_error "Systemd unit was not written to $HOME/.config/systemd/user/"
        return 1
    fi

    log_info "Installation verification successful"
    return 0
}

uninstall_installation() {
    log_info "Removing OpenRacing from $INSTALL_PREFIX..."
    if [ "$SKIP_SYSTEMD" != "true" ] && command -v systemctl &> /dev/null; then
        systemctl --user disable --now openracing.service &> /dev/null || true
        rm -f "$HOME/.config/systemd/user/openracing.service"
        systemctl --user daemon-reload &> /dev/null || true
    fi
    rm -f "$INSTALL_PREFIX/bin/wheeld" "$INSTALL_PREFIX/bin/wheelctl" "$INSTALL_PREFIX/bin/openracing"
    rm -rf "$INSTALL_PREFIX/share/openracing/contract/control-stream"
    log_info "OpenRacing binaries and control-stream contract assets removed"
}

print_post_install_instructions() {
    log_info "Installation complete!"
    echo
    echo "Next steps:"
    echo "1. Add $INSTALL_PREFIX/bin to your PATH if not already done:"
    echo "   export PATH=\"$INSTALL_PREFIX/bin:\$PATH\""
    echo "2. Install udev rules (if not done automatically):"
    # The quirks and hwdb files are optional package contents, so only suggest
    # copying the ones this package actually shipped.
    local rules_src quirks_src hwdb_src
    rules_src="$(find_asset "99-racing-wheel-suite.rules")" || rules_src=""
    quirks_src="$(find_asset "90-racing-wheel-quirks.conf")" || quirks_src=""
    hwdb_src="$(find_asset "99-racing-wheel-suite.hwdb")" || hwdb_src=""

    if [ -n "$rules_src" ]; then
        echo "   sudo cp $rules_src /etc/udev/rules.d/"
    fi
    if [ -n "$quirks_src" ]; then
        echo "   sudo cp $quirks_src /etc/modprobe.d/"
    fi
    if [ -n "$hwdb_src" ]; then
        echo "   sudo cp $hwdb_src /etc/udev/hwdb.d/"
        echo "   sudo systemd-hwdb update"
    fi
    echo "   sudo udevadm control --reload-rules && sudo udevadm trigger"
    echo "3. Add your user to required groups:"
    echo "   sudo usermod -a -G input,plugdev $SERVICE_USER"
    echo "4. Log out and back in for group changes to take effect"
    echo "   (Reboot if using Asetek wheels — the modprobe.d conf needs a reload)"
    echo "5. Start the service:"
    echo "   systemctl --user enable --now openracing.service"
    echo "6. Confirm it is running:"
    echo "   wheelctl health"
    echo
    echo "For troubleshooting, check logs with:"
    echo "   journalctl --user -u openracing.service -f"
}

main() {
    log_info "Racing Wheel Suite Linux Installer"
    log_info "Install prefix: $INSTALL_PREFIX"
    log_info "Service user: $SERVICE_USER"
    
    if [ "$UNINSTALL" = "true" ]; then
        uninstall_installation
        return
    fi

    check_dependencies
    install_binaries
    install_systemd_service
    install_udev_rules
    setup_directories
    
    if verify_installation; then
        print_post_install_instructions
    else
        log_error "Installation verification failed"
        exit 1
    fi
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --prefix=*)
            INSTALL_PREFIX="${1#*=}"
            shift
            ;;
        --skip-udev)
            SKIP_UDEV="true"
            shift
            ;;
        --skip-rtkit)
            SKIP_RTKIT="true"
            shift
            ;;
        --skip-systemd)
            SKIP_SYSTEMD="true"
            shift
            ;;
        --uninstall)
            UNINSTALL="true"
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --prefix=PATH     Installation prefix (default: ~/.local)"
            echo "  --skip-udev       Skip udev rules installation"
            echo "  --skip-rtkit      Skip rtkit dependency check"
            echo "  --skip-systemd    Skip systemd service installation and verification"
            echo "  --uninstall       Remove installed binaries and contract assets"
            echo "  --help            Show this help"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

main
