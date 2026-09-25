#!/usr/bin/env bash
# Sets up a Mac for SkinDocJyotsna from an offline bundle. No internet needed
# (except for Xcode Command Line Tools if their .dmg is not in the bundle).
#
# Safe to run again: it checks what is already installed and only installs what is needed.
# A log is written to <bundle>/logs/ (or ~/Library/Logs/SkinDocJyotsna-setup if the bundle is read-only).
#
# Usage:
#   ./install-from-bundle.sh [--role dev|build|clinic] [--bundle DIR] [--verify-only] [--force]
#
# Roles:
#   dev     Developer Mac. Tools already installed at or above the minimum version
#           (setup/versions.env) are KEPT; missing or too-old tools get the locked version.
#   build   Production build Mac. Node.js and Rust must be EXACTLY the versions in tools.lock,
#           so releases are reproducible. A different installed version stops the setup with an
#           explanation; it is never silently replaced.
#   clinic  Clinic Mac: the SkinDocJyotsna .dmg from the bundle (macOS already has the WebView).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$SCRIPT_DIR/lib/common.sh"

BUNDLE_DIR="$SCRIPT_DIR"
ROLE="dev"
VERIFY_ONLY=0
FORCE=0
while [ $# -gt 0 ]; do
  case "$1" in
    --role) ROLE="$2"; shift 2 ;;
    --bundle) BUNDLE_DIR="$(cd "$2" && pwd)"; shift 2 ;;
    --verify-only) VERIFY_ONLY=1; shift ;;
    --force) FORCE=1; shift ;;
    -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
    *) die "Unknown option: $1 (see --help)" ;;
  esac
done
case "$ROLE" in dev|build|clinic) ;; *) die "--role must be dev, build or clinic" ;; esac
[ "$(uname -s)" = "Darwin" ] || die "This script must run on macOS."

LOG_DIR="$BUNDLE_DIR/logs"
if ! mkdir -p "$LOG_DIR" 2>/dev/null || ! touch "$LOG_DIR/.write-test" 2>/dev/null; then
  LOG_DIR="$HOME/Library/Logs/SkinDocJyotsna-setup"; mkdir -p "$LOG_DIR"
fi
LOG_FILE="$LOG_DIR/install-$ROLE-$(date +%Y%m%d-%H%M%S).log"
exec > >(tee -a "$LOG_FILE") 2>&1

on_error() { fail "Setup stopped (line $1). Fix the problem above and run again - it resumes safely. Log: $LOG_FILE"; }
trap 'on_error $LINENO' ERR

export PATH="/usr/local/bin:$PATH"
ARCH="$(uname -m)"
case "$ARCH" in
  arm64) HOST_TARGET="aarch64-apple-darwin"; OTHER_TARGET="x86_64-apple-darwin" ;;
  x86_64) HOST_TARGET="x86_64-apple-darwin"; OTHER_TARGET="aarch64-apple-darwin" ;;
  *) die "Unsupported CPU architecture: $ARCH" ;;
esac

MANIFEST="$BUNDLE_DIR/manifest.txt"
m() { if [ -f "$MANIFEST" ]; then kv_get "$MANIFEST" "$1"; fi; }   # manifest value or empty

printf 'SkinDocJyotsna setup - role: %s - bundle: %s - macOS %s (%s)\n' "$ROLE" "$BUNDLE_DIR" "$(sw_vers -productVersion)" "$ARCH"

has_clt() { xcode-select -p >/dev/null 2>&1 && xcrun --find clang >/dev/null 2>&1; }

maybe_sudo() { # runs with sudo only when the target directory is not writable
  if [ -w "$1" ]; then shift; "$@"; else shift; sudo "$@"; fi
}

install_pkg() { # <pkg>
  pkgutil --check-signature "$1" >/dev/null || die "Package signature check failed: $1"
  ok "Signature valid: $(basename "$1")"
  sudo installer -pkg "$1" -target /
}

# <name> <installed-output> <locked> <minimum> <strict:0|1> -> returns 0 if an install is needed
needs_install() {
  if [ "$FORCE" -eq 1 ]; then info "--force: installing locked $1 $3"; return 0; fi
  case "$(tool_action "$2" "$3" "$4" "$ROLE" "$5")" in
    reuse)
      if [ "$ROLE" = "build" ] && [ "$5" = "1" ]; then ok "Using the installed $1 ($2): matches tools.lock"
      else ok "Using the installed $1 ($2): meets minimum $4"; fi
      return 1 ;;
    mismatch)
      die "Build role needs $1 $3 exactly (tools.lock), but '$2' is installed. Uninstall it and run again, use a clean build Mac, or update tools.lock deliberately." ;;
    *)
      if [ -n "$2" ]; then info "Installed $1 ($2) is older than the minimum $4; installing $3"; fi
      return 0 ;;
  esac
}

install_clt() {
  step "Xcode Command Line Tools (clang, git)"
  if has_clt && [ "$FORCE" -eq 0 ]; then ok "Using the installed Command Line Tools ($(xcode-select -p))"; return; fi
  local file dmg mnt pkg
  file="$(m CLT_FILE)"
  if [ -n "$file" ] && [ -f "$BUNDLE_DIR/installers/$file" ]; then
    dmg="$BUNDLE_DIR/installers/$file"
    mnt="$(hdiutil attach -nobrowse -readonly "$dmg" | awk -F'\t' '/\/Volumes\// {print $NF}' | sed -n '$p')"
    pkg="$(find "$mnt" -maxdepth 1 -name '*.pkg' | sed -n '1p')"
    [ -n "$pkg" ] || { hdiutil detach "$mnt" >/dev/null; die "No .pkg found inside $file"; }
    install_pkg "$pkg"
    hdiutil detach "$mnt" >/dev/null
  else
    warn "The CLT .dmg is not in this bundle. Starting Apple's online installer - click Install in the dialog."
    xcode-select --install || true
    info "Waiting for the Command Line Tools installation to finish..."
    until has_clt; do sleep 10; done
  fi
  ok "Installed ($(xcode-select -p))"
}

install_node() {
  local locked; locked="$(m NODE_VERSION)"
  step "Node.js (locked v$locked, minimum v$(m NODE_MIN_VERSION))"
  if ! needs_install "Node.js" "$(tool_version node -v)" "$locked" "$(m NODE_MIN_VERSION)" 1; then return; fi
  install_pkg "$BUNDLE_DIR/installers/$(m NODE_FILE)"
  ok "Installed ($(tool_version node -v))"
}

install_rust() {
  local locked; locked="$(m RUST_VERSION)"
  step "Rust (locked $locked, minimum $(m RUST_MIN_VERSION)): $HOST_TARGET host + $OTHER_TARGET target"
  local installed; installed="$(tool_version rustc -V)"
  if [ "$ROLE" = "build" ] && [ -n "$installed" ] && [ -x "$HOME/.cargo/bin/rustup" ] \
     && [ "$(tool_action "$installed" "$locked" "" build 1)" = "mismatch" ]; then
    die "Build role needs Rust $locked exactly, but rustup provides '$installed'. Run: rustup default $locked (needs internet), then run this again."
  fi
  if needs_install "Rust" "$installed" "$locked" "$(m RUST_MIN_VERSION)" 1; then
    install_pkg "$BUNDLE_DIR/installers/rust-$locked-$HOST_TARGET.pkg"
    ok "Installed ($(tool_version rustc -V))"
  fi

  local sysroot tmp ver
  sysroot="$(rustc --print sysroot)"
  if [ -d "$sysroot/lib/rustlib/$OTHER_TARGET" ] && [ "$FORCE" -eq 0 ]; then
    ok "Target $OTHER_TARGET already present"
    return
  fi
  if [ -x "$HOME/.cargo/bin/rustup" ] && [[ "$sysroot" == "$HOME/.rustup/"* ]]; then
    warn "Rust is managed by rustup; add the target with: rustup target add $OTHER_TARGET (needs internet)"
    return
  fi
  # The target must match the installed compiler exactly; the bundle has it for the locked version.
  ver="$(version_of "$(tool_version rustc -V)")"
  [ "$ver" = "$locked" ] || die "The bundle has the $OTHER_TARGET target for Rust $locked, but Rust $ver is installed. Use rustup (online) or install Rust $locked."
  tmp="$(mktemp -d)"
  tar -xf "$BUNDLE_DIR/installers/rust-std-$locked-$OTHER_TARGET.tar.xz" -C "$tmp"
  maybe_sudo "$sysroot" "$tmp/rust-std-$locked-$OTHER_TARGET/install.sh" --prefix="$sysroot" --disable-ldconfig
  rm -rf "$tmp"
  ok "Target $OTHER_TARGET installed"
}

install_clinicapp() {
  step "SkinDocJyotsna app"
  local dmg="" best="" candidate v mnt app target
  # Newest version wins (compared numerically, so 0.10.0 > 0.9.0).
  for candidate in "$BUNDLE_DIR"/installers/SkinDocJyotsna_*.dmg; do
    [ -f "$candidate" ] || continue
    v="$(version_of "$(basename "$candidate")")"
    if [ -z "$best" ] || [ "$(version_cmp "$v" "$best")" = "1" ]; then best="$v"; dmg="$candidate"; fi
  done
  if [ -z "$dmg" ]; then
    warn "No SkinDocJyotsna .dmg in this bundle yet. Add one with: node scripts/build-release.mjs --bundle <this folder>. Skipped."
    return
  fi
  mnt="$(hdiutil attach -nobrowse -readonly "$dmg" | awk -F'\t' '/\/Volumes\// {print $NF}' | sed -n '$p')"
  app="$(find "$mnt" -maxdepth 1 -name '*.app' | sed -n '1p')"
  [ -n "$app" ] || { hdiutil detach "$mnt" >/dev/null; die "No .app found inside $(basename "$dmg")"; }
  target="/Applications/$(basename "$app")"
  # Replacing the app never touches data: that lives in ~/Library/Application Support.
  sudo rm -rf "$target"
  sudo cp -R "$app" /Applications/
  hdiutil detach "$mnt" >/dev/null
  # Not yet signed with an Apple Developer ID (DESIGN Q15): clear the quarantine flag so
  # Gatekeeper lets it open. Integrity was already checked against SHA256SUMS.
  sudo xattr -dr com.apple.quarantine "$target" 2>/dev/null || true
  ok "Installed $target (open it from Applications or Launchpad)"
}

show_summary() {
  step "Verification for role '$ROLE'"
  local problems=0 sysroot=""
  # <name> <needed-roles> <found> [locked] [minimum] [strict]
  report() {
    local needed=0 locked_note="" status=reuse
    case " $2 " in *" $ROLE "*) needed=1 ;; esac
    [ -n "${4:-}" ] && locked_note=" (locked ${4})"
    if [ -z "$3" ]; then
      if [ "$needed" -eq 1 ]; then fail "$(printf '%-24s MISSING%s' "$1" "$locked_note")"; problems=$((problems + 1))
      else info "$(printf '[--]   %-24s not installed (not needed for %s)' "$1" "$ROLE")"; fi
      return 0
    fi
    if [ -n "${4:-}${5:-}" ]; then status="$(tool_action "$3" "${4:-}" "${5:-}" "$ROLE" "${6:-0}")"; fi
    case "$status" in
      mismatch) fail "$(printf '%-24s %s  <- must be exactly %s for build' "$1" "$3" "$4")"; [ "$needed" -eq 1 ] && problems=$((problems + 1)) ;;
      install)  fail "$(printf '%-24s %s  <- older than minimum %s' "$1" "$3" "$5")"; [ "$needed" -eq 1 ] && problems=$((problems + 1)) ;;
      *)        ok "$(printf '%-24s %s%s' "$1" "$3" "$locked_note")" ;;
    esac
    return 0
  }
  report "Xcode CLT"   "dev build" "$(has_clt && xcode-select -p || true)"
  report "clang"       "dev build" "$(tool_version clang --version)"
  report "git"         "dev build" "$(tool_version git --version)"
  report "node"        "dev build" "$(tool_version node -v)" "$(m NODE_VERSION)" "$(m NODE_MIN_VERSION)" 1
  report "npm"         "dev build" "$(tool_version npm -v)"
  report "rustc"       "dev build" "$(tool_version rustc -V)" "$(m RUST_VERSION)" "$(m RUST_MIN_VERSION)" 1
  report "cargo"       "dev build" "$(tool_version cargo -V)"
  if command -v rustc >/dev/null 2>&1; then sysroot="$(rustc --print sysroot)"; fi
  report "target $HOST_TARGET"  "dev build" "$([ -n "$sysroot" ] && [ -d "$sysroot/lib/rustlib/$HOST_TARGET" ] && echo present || true)"
  report "target $OTHER_TARGET" "dev build" "$([ -n "$sysroot" ] && [ -d "$sysroot/lib/rustlib/$OTHER_TARGET" ] && echo present || true)"
  report "SkinDocJyotsna.app"   "clinic"    "$([ -d /Applications/SkinDocJyotsna.app ] && echo /Applications/SkinDocJyotsna.app || true)"
  return $problems
}

if [ "$VERIFY_ONLY" -eq 0 ]; then
  step "Checking bundle integrity"
  [ -f "$MANIFEST" ] || die "manifest.txt not found in $BUNDLE_DIR. Is this a bundle made by prepare-offline-bundle.sh?"
  [ "$(m BUNDLE_PLATFORM)" = "macos" ] || die "This is not a macOS bundle."
  info "Bundle created $(m CREATED_UTC): Node $(m NODE_VERSION), Rust $(m RUST_VERSION)"
  while read -r hash rel; do
    if [ -n "$hash" ]; then assert_sha256 "$BUNDLE_DIR/$rel" "$hash"; fi
  done < "$BUNDLE_DIR/SHA256SUMS"

  info "Administrator password is needed to install packages."
  sudo -v
  if [ "$ROLE" = "clinic" ]; then
    install_clinicapp
  else
    install_clt
    install_node
    install_rust
  fi
fi

trap - ERR
if show_summary; then
  printf '\n\033[32mAll required tools for "%s" are in place.\033[0m\n' "$ROLE"
  if [ "$ROLE" != "clinic" ] && [ "$VERIFY_ONLY" -eq 0 ]; then info "Open a NEW Terminal window so the updated PATH is picked up."; fi
  exit 0
else
  fail "Problems found (see above). Log: $LOG_FILE"
  exit 1
fi
