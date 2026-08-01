#!/usr/bin/env bash
# OpenRacing Release Signing Script
#
# Signs release artifacts and generates SHA256 checksums
#
# Requirements: 19.5, 19.6
# - Sign all packages with release key
# - Generate SHA256 checksums
#
# Usage:
#   ./sign-release.sh --artifacts <dir> [--key <path>] [--output <dir>]
#
# Environment Variables:
#   OPENRACING_SIGNING_KEY - Path to Ed25519 private key (alternative to --key)
#   GPG_KEY_ID - GPG key ID for signing (if using GPG instead of Ed25519)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Default values
ARTIFACTS_DIR=""
OUTPUT_DIR=""
SIGNING_KEY=""
USE_GPG=false
GPG_KEY_ID="${GPG_KEY_ID:-}"
SKIP_SIGNING=false

# Colors
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

Sign release artifacts and generate checksums.

Options:
    --artifacts DIR     Directory containing artifacts to sign (required)
    --output DIR        Output directory for signed artifacts (default: same as artifacts)
    --key PATH          Path to Ed25519 private key for signing
    --gpg               Use GPG for signing instead of Ed25519
    --gpg-key-id ID     GPG key ID to use for signing
    --skip-signing      Generate checksums only, skip signing
    --help              Show this help message

Environment Variables:
    OPENRACING_SIGNING_KEY  Path to Ed25519 private key
    GPG_KEY_ID              GPG key ID for signing

Examples:
    $(basename "$0") --artifacts dist/ --key release-key.pem
    $(basename "$0") --artifacts dist/ --gpg --gpg-key-id ABC123
    $(basename "$0") --artifacts dist/ --skip-signing
EOF
    exit 0
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --artifacts)
            ARTIFACTS_DIR="$2"
            shift 2
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --key)
            SIGNING_KEY="$2"
            shift 2
            ;;
        --gpg)
            USE_GPG=true
            shift
            ;;
        --gpg-key-id)
            GPG_KEY_ID="$2"
            USE_GPG=true
            shift 2
            ;;
        --skip-signing)
            SKIP_SIGNING=true
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

# Validate arguments
if [[ -z "$ARTIFACTS_DIR" ]]; then
    log_error "Missing required argument: --artifacts"
    usage
fi

if [[ ! -d "$ARTIFACTS_DIR" ]]; then
    log_error "Artifacts directory does not exist: $ARTIFACTS_DIR"
    exit 1
fi

# Set output directory
if [[ -z "$OUTPUT_DIR" ]]; then
    OUTPUT_DIR="$ARTIFACTS_DIR"
fi

# Check for signing key from environment if not specified
if [[ -z "$SIGNING_KEY" && -n "${OPENRACING_SIGNING_KEY:-}" ]]; then
    SIGNING_KEY="$OPENRACING_SIGNING_KEY"
fi

log_info "=========================================="
log_info "OpenRacing Release Signing"
log_info "=========================================="
log_info "Artifacts:    $ARTIFACTS_DIR"
log_info "Output:       $OUTPUT_DIR"
log_info "Skip signing: $SKIP_SIGNING"
if [[ "$USE_GPG" == "true" ]]; then
    log_info "Signing:      GPG (Key ID: ${GPG_KEY_ID:-auto})"
elif [[ -n "$SIGNING_KEY" ]]; then
    log_info "Signing:      Ed25519"
fi
log_info ""

# Create output directory if needed
mkdir -p "$OUTPUT_DIR"

# Find all artifacts to process
ARTIFACT_PATTERNS=("*.tar.gz" "*.zip" "*.deb" "*.rpm" "*.msi" "*.dmg")
ARTIFACTS=()

for pattern in "${ARTIFACT_PATTERNS[@]}"; do
    while IFS= read -r -d '' file; do
        ARTIFACTS+=("$file")
    done < <(find "$ARTIFACTS_DIR" -maxdepth 1 -name "$pattern" -print0 2>/dev/null || true)
done

if [[ ${#ARTIFACTS[@]} -eq 0 ]]; then
    log_error "No artifacts found in $ARTIFACTS_DIR"
    exit 1
fi

log_info "Found ${#ARTIFACTS[@]} artifact(s) to process:"
for artifact in "${ARTIFACTS[@]}"; do
    log_info "  - $(basename "$artifact")"
done
log_info ""

# ============================================
# Generate SHA256 Checksums
# ============================================
log_step "Generating SHA256 checksums..."

CHECKSUMS_FILE="$OUTPUT_DIR/SHA256SUMS.txt"
> "$CHECKSUMS_FILE"

for artifact in "${ARTIFACTS[@]}"; do
    filename=$(basename "$artifact")
    
    # Generate individual checksum file
    sha256sum "$artifact" | sed "s|$ARTIFACTS_DIR/||" > "${artifact}.sha256"
    log_info "  Created: ${filename}.sha256"
    
    # Add to combined checksums file
    sha256sum "$artifact" | sed "s|$ARTIFACTS_DIR/||" >> "$CHECKSUMS_FILE"
done

log_info "  Created: SHA256SUMS.txt"

# ============================================
# Generate SHA512 Checksums (optional, for extra verification)
# ============================================
log_step "Generating SHA512 checksums..."

SHA512_FILE="$OUTPUT_DIR/SHA512SUMS.txt"
> "$SHA512_FILE"

for artifact in "${ARTIFACTS[@]}"; do
    sha512sum "$artifact" | sed "s|$ARTIFACTS_DIR/||" >> "$SHA512_FILE"
done

log_info "  Created: SHA512SUMS.txt"

# ============================================
# Sign Artifacts
# ============================================
if [[ "$SKIP_SIGNING" == "true" ]]; then
    log_warn "Skipping artifact signing (--skip-signing specified)"
elif [[ "$USE_GPG" == "true" ]]; then
    log_step "Signing artifacts with GPG..."
    
    if ! command -v gpg &> /dev/null; then
        log_error "GPG not found. Install gnupg or use --skip-signing"
        exit 1
    fi
    
    GPG_ARGS=("--armor" "--detach-sign")
    if [[ -n "$GPG_KEY_ID" ]]; then
        GPG_ARGS+=("--local-user" "$GPG_KEY_ID")
    fi
    
    for artifact in "${ARTIFACTS[@]}"; do
        filename=$(basename "$artifact")
        gpg "${GPG_ARGS[@]}" --output "${artifact}.asc" "$artifact"
        log_info "  Signed: ${filename}.asc"
    done
    
    # Sign the checksums file
    gpg "${GPG_ARGS[@]}" --output "${CHECKSUMS_FILE}.asc" "$CHECKSUMS_FILE"
    log_info "  Signed: SHA256SUMS.txt.asc"
    
elif [[ -n "$SIGNING_KEY" ]]; then
    log_step "Signing artifacts with Ed25519..."
    
    if [[ ! -f "$SIGNING_KEY" ]]; then
        log_error "Signing key not found: $SIGNING_KEY"
        exit 1
    fi
    
    # Check for openssl or minisign
    if command -v minisign &> /dev/null; then
        # Use minisign for Ed25519 signatures
        for artifact in "${ARTIFACTS[@]}"; do
            filename=$(basename "$artifact")
            minisign -S -s "$SIGNING_KEY" -m "$artifact"
            log_info "  Signed: ${filename}.minisig"
        done
        
        minisign -S -s "$SIGNING_KEY" -m "$CHECKSUMS_FILE"
        log_info "  Signed: SHA256SUMS.txt.minisig"
        
    elif command -v openssl &> /dev/null; then
        # Use OpenSSL for signing (Ed25519 or RSA depending on key type)
        for artifact in "${ARTIFACTS[@]}"; do
            filename=$(basename "$artifact")
            openssl dgst -sha256 -sign "$SIGNING_KEY" -out "${artifact}.sig" "$artifact"
            # Also create base64-encoded signature
            base64 < "${artifact}.sig" > "${artifact}.sig.b64"
            log_info "  Signed: ${filename}.sig"
        done
        
        openssl dgst -sha256 -sign "$SIGNING_KEY" -out "${CHECKSUMS_FILE}.sig" "$CHECKSUMS_FILE"
        base64 < "${CHECKSUMS_FILE}.sig" > "${CHECKSUMS_FILE}.sig.b64"
        log_info "  Signed: SHA256SUMS.txt.sig"
    else
        log_error "No signing tool found. Install minisign or openssl"
        exit 1
    fi
else
    log_warn "No signing key provided. Artifacts will not be signed."
    log_warn "Use --key, --gpg, or set OPENRACING_SIGNING_KEY environment variable"
fi

# ============================================
# Generate Release Manifest
# ============================================
log_step "Generating release manifest..."

MANIFEST_FILE="$OUTPUT_DIR/MANIFEST.json"

# Get version from first artifact name
VERSION="unknown"
for artifact in "${ARTIFACTS[@]}"; do
    if [[ $(basename "$artifact") =~ openracing-([0-9]+\.[0-9]+\.[0-9]+[^-]*) ]]; then
        VERSION="${BASH_REMATCH[1]}"
        break
    fi
done

cat > "$MANIFEST_FILE" << EOF
{
  "product": "OpenRacing",
  "version": "$VERSION",
  "release_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "artifacts": [
EOF

first=true
for artifact in "${ARTIFACTS[@]}"; do
    filename=$(basename "$artifact")
    size=$(stat -f%z "$artifact" 2>/dev/null || stat -c%s "$artifact" 2>/dev/null || echo "0")
    sha256=$(sha256sum "$artifact" | cut -d' ' -f1)
    
    if [[ "$first" == "true" ]]; then
        first=false
    else
        echo "," >> "$MANIFEST_FILE"
    fi
    
    cat >> "$MANIFEST_FILE" << EOF
    {
      "filename": "$filename",
      "size_bytes": $size,
      "sha256": "$sha256"
    }
EOF
done

cat >> "$MANIFEST_FILE" << EOF

  ],
  "checksums": {
    "sha256": "SHA256SUMS.txt",
    "sha512": "SHA512SUMS.txt"
  },
  "signed": $([[ "$SKIP_SIGNING" == "false" && (-n "$SIGNING_KEY" || "$USE_GPG" == "true") ]] && echo "true" || echo "false")
}
EOF

log_info "  Created: MANIFEST.json"

# ============================================
# Summary
# ============================================
log_info ""
log_info "=========================================="
log_info "Signing Complete!"
log_info "=========================================="
log_info ""
log_info "Output files in: $OUTPUT_DIR"
log_info ""

# List all generated files
log_info "Generated files:"
shopt -s nullglob
for file in "$OUTPUT_DIR"/*.sha256 "$OUTPUT_DIR"/*.sig* "$OUTPUT_DIR"/*.asc "$OUTPUT_DIR"/*.minisig "$OUTPUT_DIR"/SHA256SUMS.txt "$OUTPUT_DIR"/SHA512SUMS.txt "$OUTPUT_DIR"/MANIFEST.json; do
    if [[ -f "$file" ]]; then
        log_info "  $(basename "$file")"
    fi
done
shopt -u nullglob

log_info ""
log_info "Verification commands:"
log_info "  sha256sum -c SHA256SUMS.txt"
if [[ "$USE_GPG" == "true" ]]; then
    log_info "  gpg --verify SHA256SUMS.txt.asc SHA256SUMS.txt"
fi
