#!/usr/bin/env bash
# Downloads everything needed to set up a SkinDocJyotsna Mac into one folder that works offline.
#
# Run on any machine WITH internet (a Mac is best; Linux or Git Bash on Windows also work).
# Copy the resulting folder to the target Mac and double-click Install-DevMac.command,
# Install-BuildMac.command or Install-ClinicMac.command.
#
# VERSIONS ARE LOCKED: setup/macos/tools.lock records the exact version, URL and SHA-256 of
# every installer. Normal runs download exactly those files and refuse any checksum mismatch.
# --update-lock pins the newest stable releases (per setup/versions.env) and rewrites the lock.
# The first run (no lock yet) creates it. Files already in the bundle with the right checksum
# are not downloaded again, so an interrupted run can simply be restarted.
#
# Usage:
#   ./prepare-offline-bundle.sh [--output DIR] [--clt-dmg PATH] [--update-lock] [--resolve-only]
#
# Xcode Command Line Tools: Apple only lets signed-in developers download them. Download
# "Command Line Tools for Xcode <version>.dmg" from https://developer.apple.com/download/all/
# and pass it with --clt-dmg. Without it, the installer falls back to Apple's online installer.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$SCRIPT_DIR/lib/common.sh"

OUTPUT_DIR="$SCRIPT_DIR/../../dist/SkinDocJyotsna-OfflineBundle-macos"
CLT_DMG=""
UPDATE_LOCK=0
RESOLVE_ONLY=0
while [ $# -gt 0 ]; do
  case "$1" in
    --output) OUTPUT_DIR="$2"; shift 2 ;;
    --clt-dmg) CLT_DMG="$2"; shift 2 ;;
    --update-lock) UPDATE_LOCK=1; shift ;;
    --resolve-only) RESOLVE_ONLY=1; shift ;;
    -h|--help) sed -n '2,19p' "$0"; exit 0 ;;
    *) die "Unknown option: $1 (see --help)" ;;
  esac
done

POLICY="$SCRIPT_DIR/../versions.env"
LOCK="$SCRIPT_DIR/tools.lock"
ARTIFACTS="NODE RUST_PKG_ARM RUST_PKG_X64 RUST_STD_ARM RUST_STD_X64"
LOCK_KEYS="NODE_VERSION RUST_VERSION"
for id in $ARTIFACTS; do LOCK_KEYS="$LOCK_KEYS ${id}_FILE ${id}_URL ${id}_SHA256"; done
LOCK_KEYS="$LOCK_KEYS CLT_FILE CLT_SHA256"

if [ ! -f "$LOCK" ] && [ "$UPDATE_LOCK" -eq 0 ]; then
  warn "No tools.lock yet: pinning the newest stable releases now (same as --update-lock)."
  UPDATE_LOCK=1
fi

resolve_node_version() {
  case "$1" in
    *.*.*) printf '%s\n' "$1" ;;
    ''|*[!0-9]*) return 1 ;;
    *) curl -fsSL https://nodejs.org/dist/index.json \
         | grep -o "\"version\":\"v$1\.[0-9]*\.[0-9]*\"" | sed -n '1p' | cut -d'"' -f4 | sed 's/^v//' ;;
  esac
}

resolve_rust_version() { # <spec> <meta-dir>
  case "$1" in
    *.*.*) printf '%s\n' "$1"; return 0 ;;
    stable) ;;
    *) return 1 ;;
  esac
  download https://static.rust-lang.org/dist/channel-rust-stable.toml "$2/channel-rust-stable.toml" >&2
  awk '/^\[pkg\.rust\]$/ {f=1; next} f && /^version = / {match($0, /[0-9]+\.[0-9]+\.[0-9]+/); print substr($0, RSTART, RLENGTH); exit}' \
    "$2/channel-rust-stable.toml"
}

set_var() { printf -v "$1" '%s' "$2"; }   # set_var NAME VALUE (bash 3.2 friendly)

META_DIR="$OUTPUT_DIR/meta"
[ "$RESOLVE_ONLY" -eq 1 ] && META_DIR="$(mktemp -d)"
mkdir -p "$META_DIR"
INSTALLERS="$OUTPUT_DIR/installers"

if [ "$UPDATE_LOCK" -eq 1 ]; then
  step "Resolving the newest stable releases"
  NODE_VERSION="$(resolve_node_version "$(kv_get "$POLICY" NODE_VERSION)")" || true
  [ -n "$NODE_VERSION" ] || die "Could not resolve NODE_VERSION in setup/versions.env"
  RUST_VERSION="$(resolve_rust_version "$(kv_get "$POLICY" RUST_VERSION)" "$META_DIR")" || true
  [ -n "$RUST_VERSION" ] || die "Could not resolve RUST_VERSION in setup/versions.env"
  RUST_BASE="https://static.rust-lang.org/dist"
  NODE_FILE="node-v$NODE_VERSION.pkg";                          NODE_URL="https://nodejs.org/dist/v$NODE_VERSION/$NODE_FILE"
  RUST_PKG_ARM_FILE="rust-$RUST_VERSION-aarch64-apple-darwin.pkg";   RUST_PKG_ARM_URL="$RUST_BASE/$RUST_PKG_ARM_FILE"
  RUST_PKG_X64_FILE="rust-$RUST_VERSION-x86_64-apple-darwin.pkg";    RUST_PKG_X64_URL="$RUST_BASE/$RUST_PKG_X64_FILE"
  RUST_STD_ARM_FILE="rust-std-$RUST_VERSION-aarch64-apple-darwin.tar.xz"; RUST_STD_ARM_URL="$RUST_BASE/$RUST_STD_ARM_FILE"
  RUST_STD_X64_FILE="rust-std-$RUST_VERSION-x86_64-apple-darwin.tar.xz";  RUST_STD_X64_URL="$RUST_BASE/$RUST_STD_X64_FILE"
  CLT_FILE=""; CLT_SHA256=""
else
  step "Using pinned versions from $LOCK"
  for key in $LOCK_KEYS; do set_var "$key" "$(kv_get "$LOCK" "$key")"; done
  for id in $ARTIFACTS; do
    for part in FILE URL SHA256; do
      name="${id}_$part"
      [ -n "${!name}" ] || die "tools.lock has no $name. Run again with --update-lock to rebuild it."
    done
  done
fi

info "Node.js   $NODE_VERSION   $NODE_URL"
info "Rust      $RUST_VERSION   (Apple Silicon + Intel packages and targets)"
if [ "$RESOLVE_ONLY" -eq 1 ]; then rm -rf "$META_DIR"; ok "Resolve-only mode: nothing downloaded, lock unchanged."; exit 0; fi
mkdir -p "$INSTALLERS"

if [ "$UPDATE_LOCK" -eq 1 ]; then
  # Vendor-published checksums to verify the first download against.
  download "https://nodejs.org/dist/v$NODE_VERSION/SHASUMS256.txt" "$META_DIR/node-SHASUMS256.txt"
  NODE_SHA256="$(sha_from_sums_file "$META_DIR/node-SHASUMS256.txt" "$NODE_FILE")"
  for id in RUST_PKG_ARM RUST_PKG_X64 RUST_STD_ARM RUST_STD_X64; do
    url_var="${id}_URL"; file_var="${id}_FILE"
    download "${!url_var}.sha256" "$META_DIR/${!file_var}.sha256"
    set_var "${id}_SHA256" "$(first_token "$META_DIR/${!file_var}.sha256")"
  done
fi

fetch_artifact() { # <id> <label>
  local file_var="${1}_FILE" url_var="${1}_URL" sha_var="${1}_SHA256"
  local file="${!file_var}" url="${!url_var}" expected="${!sha_var}"
  local path="$INSTALLERS/$file"
  step "$2"
  [ -n "$expected" ] || die "No checksum known for $file."
  if [ -f "$path" ] && [ "$(sha256_of "$path")" = "$expected" ]; then
    ok "Already in the bundle, checksum matches: $file"
    return
  fi
  download "$url" "$path"
  if [ "$(sha256_of "$path")" != "$expected" ]; then
    rm -f "$path"
    die "$file does not match the expected checksum. If the vendor legitimately changed it, run with --update-lock and review."
  fi
  ok "Checksum verified: $file"
}

fetch_artifact NODE "Node.js $NODE_VERSION"
fetch_artifact RUST_PKG_ARM "Rust $RUST_VERSION for Apple Silicon"
fetch_artifact RUST_PKG_X64 "Rust $RUST_VERSION for Intel"
fetch_artifact RUST_STD_ARM "Rust $RUST_VERSION Apple Silicon target (for universal builds)"
fetch_artifact RUST_STD_X64 "Rust $RUST_VERSION Intel target (for universal builds)"

step "Xcode Command Line Tools"
if [ -n "$CLT_DMG" ]; then
  [ -f "$CLT_DMG" ] || die "File not found: $CLT_DMG"
  new_sha="$(sha256_of "$CLT_DMG")"
  if [ "$UPDATE_LOCK" -eq 0 ] && [ -n "$CLT_SHA256" ] && [ "$new_sha" != "$CLT_SHA256" ]; then
    die "$(basename "$CLT_DMG") differs from the one in tools.lock ($CLT_FILE). Use --update-lock to accept a new version."
  fi
  CLT_FILE="$(basename "$CLT_DMG")"; CLT_SHA256="$new_sha"
  cp "$CLT_DMG" "$INSTALLERS/$CLT_FILE"
  ok "Copied $CLT_FILE"
elif [ -n "$CLT_FILE" ] && [ -f "$INSTALLERS/$CLT_FILE" ]; then
  [ "$(sha256_of "$INSTALLERS/$CLT_FILE")" = "$CLT_SHA256" ] || die "$CLT_FILE in the bundle does not match tools.lock."
  ok "Already in the bundle, checksum matches: $CLT_FILE"
else
  warn "No Command Line Tools .dmg (--clt-dmg): the Mac will need internet for that one step."
fi

if [ "$UPDATE_LOCK" -eq 1 ]; then
  step "Updating tools.lock"
  for key in NODE_VERSION RUST_VERSION CLT_FILE; do
    old="$( [ -f "$LOCK" ] && kv_get "$LOCK" "$key" || true )"
    if [ "$old" != "${!key}" ]; then info "$(printf '%-14s %s -> %s' "$key" "${old:-(new)}" "${!key}")"; fi
  done
  {
    echo "# SkinDocJyotsna tool lock - macOS. Exact versions + SHA-256 of every installer in the setup bundle."
    echo "# Generated by prepare-offline-bundle.sh --update-lock. Review changes and keep this file with the code."
    echo "# Locked at $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    for key in $LOCK_KEYS; do echo "$key=${!key}"; done
  } > "$LOCK"
  ok "Written: $LOCK"
fi

step "Copying installer scripts and the lock into the bundle"
mkdir -p "$OUTPUT_DIR/lib"
for f in install-from-bundle.sh Install-DevMac.command Install-BuildMac.command Install-ClinicMac.command tools.lock; do
  cp "$SCRIPT_DIR/$f" "$OUTPUT_DIR/"
done
cp "$SCRIPT_DIR/lib/common.sh" "$OUTPUT_DIR/lib/"
cp "$SCRIPT_DIR/../README.md" "$OUTPUT_DIR/README.md"
chmod +x "$OUTPUT_DIR/install-from-bundle.sh" "$OUTPUT_DIR"/*.command
ok "Scripts copied"

step "Project dependencies (npm packages + Rust crates) for offline builds"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
if [ -f "$REPO_ROOT/package-lock.json" ] && command -v node >/dev/null 2>&1; then
  node "$REPO_ROOT/scripts/offline-deps.mjs" vendor "$OUTPUT_DIR/project-deps"
else
  warn "Node.js or package-lock.json not found here: project dependencies were not added to the bundle."
fi

step "Writing manifest.txt and SHA256SUMS"
{
  echo "# SkinDocJyotsna offline setup bundle (macOS). Generated - do not edit."
  echo "BUNDLE_PLATFORM=macos"
  echo "CREATED_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  for key in $LOCK_KEYS; do echo "$key=${!key}"; done
  echo "NODE_MIN_VERSION=$(kv_get "$POLICY" NODE_MIN_VERSION)"
  echo "RUST_MIN_VERSION=$(kv_get "$POLICY" RUST_MIN_VERSION)"
} > "$OUTPUT_DIR/manifest.txt"
( cd "$OUTPUT_DIR" && for f in installers/*; do printf '%s  %s\n' "$(sha256_of "$f")" "$f"; done ) > "$OUTPUT_DIR/SHA256SUMS"
ok "Manifest and checksums written"

step "Bundle ready: $OUTPUT_DIR ($(du -sh "$OUTPUT_DIR" | awk '{print $1}'))"
info "Copy this whole folder to the Mac, then double-click:"
info "  Install-DevMac.command     developer Mac: keeps tools you already have if they are new enough"
info "  Install-BuildMac.command   production build Mac: Node.js and Rust exactly as in tools.lock"
info "  Install-ClinicMac.command  clinic Mac: the SkinDocJyotsna app"
info "(If macOS blocks a .command file: right-click > Open, or run: bash install-from-bundle.sh --role dev)"
