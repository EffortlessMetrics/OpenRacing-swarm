#!/bin/bash
# OpenRacing Linux Package Build Script
#
# Builds Linux distribution packages:
# - .deb (Debian/Ubuntu)
# - .rpm (Fedora/RHEL)
# - tarball (generic Linux)
#
# Requirements: 19.1 - Linux packages: .deb, .rpm, and tarball
#
# Usage:
#   ./build-packages.sh --bin-path <path> [--version <version>] [--output <dir>]
#
# Dependencies:
#   - dpkg-deb (for .deb packages)
#   - rpmbuild (for .rpm packages)
#   - tar, gzip (for tarballs)

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Default values
BIN_PATH=""
OUTPUT_DIR="dist"
VERSION=""
ARCH="amd64"
MAINTAINER="OpenRacing Contributors <openracing@example.com>"
DESCRIPTION="Professional racing wheel force feedback software suite"
HOMEPAGE="https://github.com/EffortlessMetrics/OpenRacing"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "${BLUE}[STEP]${NC} $1"; }

usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Build Linux distribution packages for OpenRacing.

Options:
    --bin-path PATH     Path to compiled binaries (required)
    --version VERSION   Version string (default: from Cargo.toml)
    --output DIR        Output directory (default: dist)
    --arch ARCH         Architecture: amd64, arm64 (default: amd64)
    --deb-only          Build only .deb package
    --rpm-only          Build only .rpm package
    --tarball-only      Build only tarball
    --help              Show this help message

Examples:
    $(basename "$0") --bin-path target/release
    $(basename "$0") --bin-path target/release --version 1.0.0 --output packages
EOF
    exit 0
}

# Parse command line arguments
BUILD_DEB=true
BUILD_RPM=true
BUILD_TARBALL=true

while [[ $# -gt 0 ]]; do
    case $1 in
        --bin-path)
            BIN_PATH="$2"
            shift 2
            ;;
        --version)
            VERSION="$2"
            shift 2
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --arch)
            ARCH="$2"
            shift 2
            ;;
        --deb-only)
            BUILD_RPM=false
            BUILD_TARBALL=false
            shift
            ;;
        --rpm-only)
            BUILD_DEB=false
            BUILD_TARBALL=false
            shift
            ;;
        --tarball-only)
            BUILD_DEB=false
            BUILD_RPM=false
            shift
            ;;
        --help)
            usage
            ;;
        *)
            log_error "Unknown option: $1"
            usage
            ;;
    esac
done

# Validate required arguments
if [[ -z "$BIN_PATH" ]]; then
    log_error "Missing required argument: --bin-path"
    usage
fi

if [[ ! -d "$BIN_PATH" ]]; then
    log_error "Binary path does not exist: $BIN_PATH"
    exit 1
fi

# Get version from Cargo.toml if not specified
if [[ -z "$VERSION" ]]; then
    if [[ -f "$PROJECT_ROOT/Cargo.toml" ]]; then
        VERSION=$(grep -m1 '^version = ' "$PROJECT_ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
        log_info "Detected version from Cargo.toml: $VERSION"
    else
        log_error "Could not determine version. Please specify --version"
        exit 1
    fi
fi

# Map architecture names
case "$ARCH" in
    amd64|x86_64)
        DEB_ARCH="amd64"
        RPM_ARCH="x86_64"
        ;;
    arm64|aarch64)
        DEB_ARCH="arm64"
        RPM_ARCH="aarch64"
        ;;
    *)
        log_error "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Create output directory
mkdir -p "$OUTPUT_DIR"

log_info "=========================================="
log_info "OpenRacing Linux Package Builder"
log_info "=========================================="
log_info "Version:     $VERSION"
log_info "Architecture: $ARCH"
log_info "Binary path: $BIN_PATH"
log_info "Output:      $OUTPUT_DIR"
log_info ""

# Verify required binaries exist
REQUIRED_BINARIES=("wheeld" "wheelctl")
OPTIONAL_BINARIES=("openracing" "wheel-ui")

for binary in "${REQUIRED_BINARIES[@]}"; do
    if [[ ! -f "$BIN_PATH/$binary" ]]; then
        log_error "Required binary not found: $BIN_PATH/$binary"
        exit 1
    fi
    log_info "Found: $binary"
done

for binary in "${OPTIONAL_BINARIES[@]}"; do
    if [[ -f "$BIN_PATH/$binary" ]]; then
        log_info "Found optional: $binary"
    fi
done


# ============================================
# Build Tarball
# ============================================
build_tarball() {
    log_step "Building tarball..."
    
    local tarball_name="openracing-${VERSION}-linux-${ARCH}"
    local tarball_dir="$OUTPUT_DIR/$tarball_name"
    
    # Clean and create directory
    rm -rf "$tarball_dir"
    mkdir -p "$tarball_dir"/{bin,config,docs,systemd}
    
    # Copy binaries
    cp "$BIN_PATH/wheeld" "$tarball_dir/bin/"
    cp "$BIN_PATH/wheelctl" "$tarball_dir/bin/"
    
    for binary in "${OPTIONAL_BINARIES[@]}"; do
        if [[ -f "$BIN_PATH/$binary" ]]; then
            cp "$BIN_PATH/$binary" "$tarball_dir/bin/"
        fi
    done

    # Package the external control-stream contract bundle (issue #179): the
    # pinned wire proto, a compatibility manifest (control_stream_v1 feature +
    # capture schema + min compatible versions), checksums, and a deterministic
    # replay fixture. External consumers use this to build a control_stream_v1
    # client without depending on OpenRacing engine/HID/FFB crates.
    local contract_dir="$tarball_dir/contract/control-stream"
    mkdir -p "$contract_dir"
    # The four assets that make up a coherent bundle (issue #179).
    local required_assets=(
        "control-stream-contract.json"
        "wheel.proto"
        "sample-capture.json"
        "SHA256SUMS"
    )
    if [[ -n "${CONTRACT_PATH:-}" && -d "${CONTRACT_PATH:-}" ]]; then
        # Reuse a pre-generated bundle (e.g. a cross-build where cargo is
        # unavailable on this host). Copy only the allowlisted assets by name and
        # reject symlinks, so a broad, stale, or crafted CONTRACT_PATH cannot add
        # unlisted files or pull host files into the release tarball. --check
        # below still verifies coherence.
        for asset in "${required_assets[@]}"; do
            if [[ -L "$CONTRACT_PATH/$asset" ]]; then
                echo "ERROR: refusing to copy symlinked contract asset $asset from CONTRACT_PATH" >&2
                return 1
            fi
            if [[ ! -f "$CONTRACT_PATH/$asset" ]]; then
                echo "ERROR: CONTRACT_PATH is missing contract asset $asset" >&2
                return 1
            fi
            cp -- "$CONTRACT_PATH/$asset" "$contract_dir/$asset"
        done
    elif command -v cargo >/dev/null 2>&1; then
        ( cd "$PROJECT_ROOT" && cargo run --quiet --locked -p openracing-tools \
            --bin control-stream-contract -- --out "$contract_dir" )
    else
        echo "ERROR: control-stream contract bundle cannot be produced:" >&2
        echo "       set CONTRACT_PATH to a pre-generated bundle or make cargo available." >&2
        return 1
    fi
    # The contract bundle is a required release artifact (issue #179). Verify it
    # exists and is coherent, and fail the package build otherwise — a tarball
    # missing or with a corrupt bundle must never ship. --check needs cargo, so
    # a CONTRACT_PATH bundle on a cargo-less host is a hard error too.
    for asset in "${required_assets[@]}"; do
        if [[ ! -f "$contract_dir/$asset" ]]; then
            echo "ERROR: control-stream contract bundle is missing $asset" >&2
            return 1
        fi
    done
    if ! command -v cargo >/dev/null 2>&1; then
        echo "ERROR: cargo is required to verify the control-stream contract bundle" >&2
        return 1
    fi
    ( cd "$PROJECT_ROOT" && cargo run --quiet --locked -p openracing-tools \
        --bin control-stream-contract -- --check "$contract_dir" )

    # Copy packaging files
    cp "$SCRIPT_DIR/99-racing-wheel-suite.rules" "$tarball_dir/"
    cp "$SCRIPT_DIR/wheeld.service.template" "$tarball_dir/systemd/wheeld.service"
    cp "$SCRIPT_DIR/install.sh" "$tarball_dir/"
    chmod +x "$tarball_dir/install.sh"
    
    # Copy hwdb and kernel quirks if present
    if [[ -f "$SCRIPT_DIR/99-racing-wheel-suite.hwdb" ]]; then
        cp "$SCRIPT_DIR/99-racing-wheel-suite.hwdb" "$tarball_dir/"
    fi
    if [[ -f "$SCRIPT_DIR/90-racing-wheel-quirks.conf" ]]; then
        cp "$SCRIPT_DIR/90-racing-wheel-quirks.conf" "$tarball_dir/"
    fi
    
    # Copy documentation
    if [[ -f "$PROJECT_ROOT/README.md" ]]; then
        cp "$PROJECT_ROOT/README.md" "$tarball_dir/docs/"
    fi
    if [[ -f "$PROJECT_ROOT/CHANGELOG.md" ]]; then
        cp "$PROJECT_ROOT/CHANGELOG.md" "$tarball_dir/docs/"
    fi
    if [[ -f "$PROJECT_ROOT/LICENSE-MIT" ]]; then
        cp "$PROJECT_ROOT/LICENSE-MIT" "$tarball_dir/docs/"
    fi
    if [[ -f "$PROJECT_ROOT/LICENSE-APACHE" ]]; then
        cp "$PROJECT_ROOT/LICENSE-APACHE" "$tarball_dir/docs/"
    fi
    if [[ -f "$PROJECT_ROOT/LICENSE" ]]; then
        cp "$PROJECT_ROOT/LICENSE" "$tarball_dir/docs/"
    fi
    
    # Create README for tarball
    cat > "$tarball_dir/README.txt" << EOF
OpenRacing v${VERSION}
====================

Professional racing wheel force feedback software suite.

Installation:
  ./install.sh --prefix=/usr/local

Or manually:
  1. Copy binaries from bin/ to /usr/local/bin/
  2. Copy 99-racing-wheel-suite.rules to /etc/udev/rules.d/
  3. Copy 99-racing-wheel-suite.hwdb to /etc/udev/hwdb.d/
  4. Copy 90-racing-wheel-quirks.conf to /etc/modprobe.d/
  5. Copy systemd/wheeld.service to ~/.config/systemd/user/
  6. Run: sudo udevadm control --reload-rules && sudo udevadm trigger
  7. Run: sudo systemd-hwdb update
  8. Run: systemctl --user enable --now wheeld.service

For more information, see docs/README.md
EOF
    
    # Create tarball
    cd "$OUTPUT_DIR"
    tar -czvf "${tarball_name}.tar.gz" "$tarball_name"
    rm -rf "$tarball_name"
    
    log_info "Created: $OUTPUT_DIR/${tarball_name}.tar.gz"
}

# ============================================
# Build Debian Package
# ============================================
build_deb() {
    log_step "Building Debian package..."
    
    # Check for dpkg-deb
    if ! command -v dpkg-deb &> /dev/null; then
        log_warn "dpkg-deb not found, skipping .deb package"
        return 0
    fi
    
    local deb_name="openracing_${VERSION}_${DEB_ARCH}"
    local deb_dir="$OUTPUT_DIR/deb-build"
    
    # Clean and create directory structure
    rm -rf "$deb_dir"
    mkdir -p "$deb_dir/DEBIAN"
    mkdir -p "$deb_dir/usr/bin"
    mkdir -p "$deb_dir/usr/lib/systemd/user"
    mkdir -p "$deb_dir/etc/udev/rules.d"
    mkdir -p "$deb_dir/etc/udev/hwdb.d"
    mkdir -p "$deb_dir/etc/modprobe.d"
    mkdir -p "$deb_dir/usr/share/doc/openracing"
    mkdir -p "$deb_dir/usr/share/openracing/config"
    
    # Copy binaries
    cp "$BIN_PATH/wheeld" "$deb_dir/usr/bin/"
    cp "$BIN_PATH/wheelctl" "$deb_dir/usr/bin/"
    chmod 755 "$deb_dir/usr/bin/wheeld"
    chmod 755 "$deb_dir/usr/bin/wheelctl"
    
    for binary in "${OPTIONAL_BINARIES[@]}"; do
        if [[ -f "$BIN_PATH/$binary" ]]; then
            cp "$BIN_PATH/$binary" "$deb_dir/usr/bin/"
            chmod 755 "$deb_dir/usr/bin/$binary"
        fi
    done
    
    # Copy systemd service
    sed "s|%INSTALL_PATH%|/usr|g" "$SCRIPT_DIR/wheeld.service.template" > "$deb_dir/usr/lib/systemd/user/openracing.service"
    
    # Copy udev rules
    cp "$SCRIPT_DIR/99-racing-wheel-suite.rules" "$deb_dir/etc/udev/rules.d/"
    
    # Copy hwdb for joystick classification
    if [[ -f "$SCRIPT_DIR/99-racing-wheel-suite.hwdb" ]]; then
        cp "$SCRIPT_DIR/99-racing-wheel-suite.hwdb" "$deb_dir/etc/udev/hwdb.d/"
    fi
    
    # Copy kernel HID quirks (Asetek/Simagic ALWAYS_POLL)
    if [[ -f "$SCRIPT_DIR/90-racing-wheel-quirks.conf" ]]; then
        cp "$SCRIPT_DIR/90-racing-wheel-quirks.conf" "$deb_dir/etc/modprobe.d/"
    fi
    
    # Copy documentation
    if [[ -f "$PROJECT_ROOT/README.md" ]]; then
        cp "$PROJECT_ROOT/README.md" "$deb_dir/usr/share/doc/openracing/"
    fi
    if [[ -f "$PROJECT_ROOT/CHANGELOG.md" ]]; then
        cp "$PROJECT_ROOT/CHANGELOG.md" "$deb_dir/usr/share/doc/openracing/"
    fi
    
    # Create copyright file
    cat > "$deb_dir/usr/share/doc/openracing/copyright" << EOF
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: openracing
Source: $HOMEPAGE

Files: *
Copyright: 2024 OpenRacing Contributors
License: MIT or Apache-2.0

License: MIT
 Permission is hereby granted, free of charge, to any person obtaining a copy
 of this software and associated documentation files (the "Software"), to deal
 in the Software without restriction.

License: Apache-2.0
 Licensed under the Apache License, Version 2.0.
 See /usr/share/common-licenses/Apache-2.0 for the full license text.
EOF
    
    # Create control file
    cat > "$deb_dir/DEBIAN/control" << EOF
Package: openracing
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${DEB_ARCH}
Depends: libc6 (>= 2.31), libudev1
Recommends: rtkit
Suggests: libwebkit2gtk-4.1-0
Maintainer: ${MAINTAINER}
Description: ${DESCRIPTION}
 OpenRacing provides real-time force feedback processing at 1kHz
 with safety-critical design for sim-racing enthusiasts.
 .
 Features:
  - Real-time FFB at 1kHz with sub-millisecond latency
  - Multi-game integration: iRacing, ACC, AMS2, rFactor 2
  - Safety-critical design with FMEA analysis
  - Plugin architecture (WASM + native)
Homepage: ${HOMEPAGE}
EOF
    
    # Create postinst script
    cat > "$deb_dir/DEBIAN/postinst" << 'EOF'
#!/bin/bash
set -e

# Reload udev rules
if command -v udevadm &> /dev/null; then
    udevadm control --reload-rules || true
    udevadm trigger || true
fi

# Update hwdb for joystick classification
if command -v systemd-hwdb &> /dev/null; then
    systemd-hwdb update || true
fi

# Reload systemd user daemon for all logged-in users
if command -v systemctl &> /dev/null; then
    # System-wide daemon reload
    systemctl daemon-reload || true
fi

echo "OpenRacing installed successfully!"
echo ""
echo "To start the service for your user:"
echo "  systemctl --user enable --now openracing.service"
echo ""
echo "To verify installation:"
echo "  wheelctl health"

exit 0
EOF
    chmod 755 "$deb_dir/DEBIAN/postinst"
    
    # Create prerm script
    cat > "$deb_dir/DEBIAN/prerm" << 'EOF'
#!/bin/bash
set -e

# Stop user service if running (best effort)
if command -v systemctl &> /dev/null; then
    systemctl --user stop openracing.service 2>/dev/null || true
    systemctl --user disable openracing.service 2>/dev/null || true
fi

exit 0
EOF
    chmod 755 "$deb_dir/DEBIAN/prerm"
    
    # Create postrm script
    cat > "$deb_dir/DEBIAN/postrm" << 'EOF'
#!/bin/bash
set -e

# Reload udev rules
if command -v udevadm &> /dev/null; then
    udevadm control --reload-rules || true
fi

exit 0
EOF
    chmod 755 "$deb_dir/DEBIAN/postrm"
    
    # Build the package
    dpkg-deb --build --root-owner-group "$deb_dir" "$OUTPUT_DIR/${deb_name}.deb"
    
    # Clean up
    rm -rf "$deb_dir"
    
    log_info "Created: $OUTPUT_DIR/${deb_name}.deb"
}

# ============================================
# Build RPM Package
# ============================================
build_rpm() {
    log_step "Building RPM package..."
    
    # Check for rpmbuild
    if ! command -v rpmbuild &> /dev/null; then
        log_warn "rpmbuild not found, skipping .rpm package"
        return 0
    fi
    
    local rpm_name="openracing-${VERSION}"
    local rpm_build_dir="$OUTPUT_DIR/rpm-build"
    
    # Clean and create RPM build structure
    rm -rf "$rpm_build_dir"
    mkdir -p "$rpm_build_dir"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}
    
    # Create source tarball for RPM
    local source_dir="$rpm_build_dir/SOURCES/openracing-${VERSION}"
    mkdir -p "$source_dir"/{bin,systemd,udev,hwdb,modprobe,docs}
    
    # Copy files
    cp "$BIN_PATH/wheeld" "$source_dir/bin/"
    cp "$BIN_PATH/wheelctl" "$source_dir/bin/"
    
    for binary in "${OPTIONAL_BINARIES[@]}"; do
        if [[ -f "$BIN_PATH/$binary" ]]; then
            cp "$BIN_PATH/$binary" "$source_dir/bin/"
        fi
    done
    
    sed "s|%INSTALL_PATH%|/usr|g" "$SCRIPT_DIR/wheeld.service.template" > "$source_dir/systemd/openracing.service"
    cp "$SCRIPT_DIR/99-racing-wheel-suite.rules" "$source_dir/udev/"
    
    # Copy hwdb and kernel quirks if present
    if [[ -f "$SCRIPT_DIR/99-racing-wheel-suite.hwdb" ]]; then
        cp "$SCRIPT_DIR/99-racing-wheel-suite.hwdb" "$source_dir/hwdb/"
    fi
    if [[ -f "$SCRIPT_DIR/90-racing-wheel-quirks.conf" ]]; then
        cp "$SCRIPT_DIR/90-racing-wheel-quirks.conf" "$source_dir/modprobe/"
    fi
    
    if [[ -f "$PROJECT_ROOT/README.md" ]]; then
        cp "$PROJECT_ROOT/README.md" "$source_dir/docs/"
    fi
    if [[ -f "$PROJECT_ROOT/CHANGELOG.md" ]]; then
        cp "$PROJECT_ROOT/CHANGELOG.md" "$source_dir/docs/"
    fi
    
    # Create source tarball
    cd "$rpm_build_dir/SOURCES"
    tar -czvf "openracing-${VERSION}.tar.gz" "openracing-${VERSION}"
    rm -rf "openracing-${VERSION}"
    
    # Create spec file
    cat > "$rpm_build_dir/SPECS/openracing.spec" << EOF
Name:           openracing
Version:        ${VERSION}
Release:        1%{?dist}
Summary:        ${DESCRIPTION}

License:        MIT or Apache-2.0
URL:            ${HOMEPAGE}
Source0:        %{name}-%{version}.tar.gz

BuildArch:      ${RPM_ARCH}
Requires:       systemd-libs
Recommends:     rtkit

%description
OpenRacing provides real-time force feedback processing at 1kHz
with safety-critical design for sim-racing enthusiasts.

Features:
- Real-time FFB at 1kHz with sub-millisecond latency
- Multi-game integration: iRacing, ACC, AMS2, rFactor 2
- Safety-critical design with FMEA analysis
- Plugin architecture (WASM + native)

%prep
%setup -q

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}%{_bindir}
mkdir -p %{buildroot}%{_userunitdir}
mkdir -p %{buildroot}%{_udevrulesdir}
mkdir -p %{buildroot}/etc/udev/hwdb.d
mkdir -p %{buildroot}/etc/modprobe.d
mkdir -p %{buildroot}%{_docdir}/%{name}

install -m 755 bin/wheeld %{buildroot}%{_bindir}/
install -m 755 bin/wheelctl %{buildroot}%{_bindir}/
install -m 644 systemd/openracing.service %{buildroot}%{_userunitdir}/
install -m 644 udev/99-racing-wheel-suite.rules %{buildroot}%{_udevrulesdir}/

if [ -f hwdb/99-racing-wheel-suite.hwdb ]; then
    install -m 644 hwdb/99-racing-wheel-suite.hwdb %{buildroot}/etc/udev/hwdb.d/
fi
if [ -f modprobe/90-racing-wheel-quirks.conf ]; then
    install -m 644 modprobe/90-racing-wheel-quirks.conf %{buildroot}/etc/modprobe.d/
fi

if [ -f docs/README.md ]; then
    install -m 644 docs/README.md %{buildroot}%{_docdir}/%{name}/
fi
if [ -f docs/CHANGELOG.md ]; then
    install -m 644 docs/CHANGELOG.md %{buildroot}%{_docdir}/%{name}/
fi

%post
udevadm control --reload-rules || true
udevadm trigger || true
systemd-hwdb update || true
echo "OpenRacing installed. Enable with: systemctl --user enable --now openracing.service"

%preun
systemctl --user stop openracing.service 2>/dev/null || true
systemctl --user disable openracing.service 2>/dev/null || true

%postun
udevadm control --reload-rules || true

%files
%{_bindir}/wheeld
%{_bindir}/wheelctl
%{_userunitdir}/openracing.service
%{_udevrulesdir}/99-racing-wheel-suite.rules
%config(noreplace) /etc/udev/hwdb.d/99-racing-wheel-suite.hwdb
%config(noreplace) /etc/modprobe.d/90-racing-wheel-quirks.conf
%{_docdir}/%{name}

%changelog
* $(date '+%a %b %d %Y') OpenRacing Contributors <openracing@example.com> - ${VERSION}-1
- Release ${VERSION}
EOF
    
    # Build RPM
    rpmbuild --define "_topdir $rpm_build_dir" -bb "$rpm_build_dir/SPECS/openracing.spec"
    
    # Move RPM to output directory
    find "$rpm_build_dir/RPMS" -name "*.rpm" -exec mv {} "$OUTPUT_DIR/" \;
    
    # Clean up
    rm -rf "$rpm_build_dir"
    
    log_info "Created: $OUTPUT_DIR/openracing-${VERSION}*.rpm"
}

# ============================================
# Main Build Process
# ============================================

if [[ "$BUILD_TARBALL" == "true" ]]; then
    build_tarball
fi

if [[ "$BUILD_DEB" == "true" ]]; then
    build_deb
fi

if [[ "$BUILD_RPM" == "true" ]]; then
    build_rpm
fi

# Generate checksums for all packages
log_step "Generating checksums..."
cd "$OUTPUT_DIR"
for file in *.tar.gz *.deb *.rpm 2>/dev/null; do
    if [[ -f "$file" ]]; then
        sha256sum "$file" > "${file}.sha256"
        log_info "Checksum: ${file}.sha256"
    fi
done

log_info ""
log_info "=========================================="
log_info "Build Complete!"
log_info "=========================================="
log_info ""
log_info "Output files in: $OUTPUT_DIR"
ls -la "$OUTPUT_DIR"
