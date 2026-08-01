#!/bin/bash
# Racing Wheel Suite Linux Installation Script

set -euo pipefail

# Configuration
INSTALL_PREFIX="${INSTALL_PREFIX:-$HOME/.local}"
SERVICE_USER="${SERVICE_USER:-$USER}"
SKIP_UDEV="${SKIP_UDEV:-false}"
SKIP_RTKIT="${SKIP_RTKIT:-false}"

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
    
    if ! command -v systemctl &> /dev/null; then
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
    
    # Copy binaries (assuming they're in the current directory or a bin/ subdirectory)
    local bin_source="."
    if [ -d "bin" ]; then
        bin_source="bin"
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
    local contract_source="contract/control-stream"
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
    if [ -d "config" ]; then
        cp -r config/* "$INSTALL_PREFIX/share/racing-wheel-suite/config/"
    fi
    
    # Install documentation
    if [ -f "README.md" ]; then
        cp README.md "$INSTALL_PREFIX/share/doc/racing-wheel-suite/"
    fi
    if [ -f "LICENSE" ]; then
        cp LICENSE "$INSTALL_PREFIX/share/doc/racing-wheel-suite/"
    fi
}

install_systemd_service() {
    log_info "Installing systemd user service..."
    
    local service_dir="$HOME/.config/systemd/user"
    mkdir -p "$service_dir"
    
    # Generate service file from template
    local service_file="$service_dir/openracing.service"
    sed "s|%INSTALL_PATH%|$INSTALL_PREFIX|g" packaging/linux/wheeld.service.template > "$service_file"
    
    # Reload systemd and enable service
    systemctl --user daemon-reload
    systemctl --user enable openracing.service
    
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
    
    if [ "$EUID" -eq 0 ]; then
        # Running as root
        cp packaging/linux/99-racing-wheel-suite.rules "$udev_rules_file"
        cp packaging/linux/90-racing-wheel-quirks.conf "$modprobe_conf"
        cp packaging/linux/99-racing-wheel-suite.hwdb "$hwdb_file"
        systemd-hwdb update
        udevadm control --reload-rules
        udevadm trigger
        log_info "udev rules installed system-wide"
        log_info "hwdb entries installed (joystick classification for racing peripherals)"
        log_info "HID quirks (modprobe.d) installed — reboot or reload usbhid for Asetek wheels"
    else
        # Not running as root - provide instructions
        log_warn "Not running as root. udev rules need to be installed manually:"
        log_warn "sudo cp packaging/linux/99-racing-wheel-suite.rules $udev_rules_file"
        log_warn "sudo cp packaging/linux/90-racing-wheel-quirks.conf $modprobe_conf"
        log_warn "sudo cp packaging/linux/99-racing-wheel-suite.hwdb $hwdb_file"
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
    
    # Check binaries
    for binary in wheeld wheelctl; do
        if ! command -v "$binary" &> /dev/null; then
            log_error "$binary not found in PATH"
            log_error "Make sure $INSTALL_PREFIX/bin is in your PATH"
            return 1
        fi
    done
    
    # Check service file
    if ! systemctl --user list-unit-files openracing.service &> /dev/null; then
        log_error "Systemd service not found"
        return 1
    fi
    
    log_info "Installation verification successful"
    return 0
}

print_post_install_instructions() {
    log_info "Installation complete!"
    echo
    echo "Next steps:"
    echo "1. Add $INSTALL_PREFIX/bin to your PATH if not already done"
    echo "2. Install udev rules (if not done automatically):"
    echo "   sudo cp packaging/linux/99-racing-wheel-suite.rules /etc/udev/rules.d/"
    echo "   sudo cp packaging/linux/90-racing-wheel-quirks.conf /etc/modprobe.d/"
    echo "   sudo cp packaging/linux/99-racing-wheel-suite.hwdb /etc/udev/hwdb.d/"
    echo "   sudo systemd-hwdb update"
    echo "   sudo udevadm control --reload-rules && sudo udevadm trigger"
    echo "3. Add your user to required groups:"
    echo "   sudo usermod -a -G input,plugdev $USER"
    echo "4. Log out and back in for group changes to take effect"
    echo "   (Reboot if using Asetek wheels — the modprobe.d conf needs a reload)"
    echo "5. Start the service:"
    echo "   systemctl --user start openracing.service"
    echo "6. Launch the UI (if installed):"
    echo "   openracing"
    echo
    echo "For troubleshooting, check logs with:"
    echo "   journalctl --user -u openracing.service -f"
}

main() {
    log_info "Racing Wheel Suite Linux Installer"
    log_info "Install prefix: $INSTALL_PREFIX"
    log_info "Service user: $SERVICE_USER"
    
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
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --prefix=PATH     Installation prefix (default: ~/.local)"
            echo "  --skip-udev       Skip udev rules installation"
            echo "  --skip-rtkit      Skip rtkit dependency check"
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
