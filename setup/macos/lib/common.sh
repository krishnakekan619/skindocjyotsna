#!/usr/bin/env bash
# Shared helpers for the SkinDocJyotsna macOS setup scripts.
# Compatible with the bash 3.2 that ships with macOS (no associative arrays, no mapfile).

step() { printf '\n\033[36m==> %s\033[0m\n' "$*"; }
ok()   { printf '    \033[32m[OK]\033[0m   %s\n' "$*"; }
info() { printf '    %s\n' "$*"; }
warn() { printf '    \033[33m[WARN]\033[0m %s\n' "$*"; }
fail() { printf '    \033[31m[FAIL]\033[0m %s\n' "$*" >&2; }
die()  { fail "$*"; exit 1; }

sha256_of() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

download() { # <url> <output-file>  (writes <file>.download, renamed into place only when complete)
  mkdir -p "$(dirname "$2")"
  info "Downloading $1"
  rm -f "$2.download"
  if ! curl --fail --location --retry 3 --retry-delay 5 --silent --show-error --output "$2.download" "$1"; then
    rm -f "$2.download"
    die "Download failed: $1 (check the internet connection and free disk space, then run again)"
  fi
  mv -f "$2.download" "$2"
}

assert_sha256() { # <file> <expected-hash>
  local actual
  actual="$(sha256_of "$1")"
  [ "$actual" = "$2" ] || die "Checksum mismatch for $1 (expected $2, got $actual)"
  ok "Checksum verified: $(basename "$1")"
}

first_token() { # prints the first word of a file (format of Rust's *.sha256 files)
  awk 'NR==1 {print $1}' "$1"
}

sha_from_sums_file() { # <sums-file> <file-name>
  awk -v name="$2" '{ f=$2; sub(/^\*/, "", f); n=split(f, p, "/"); if (p[n] == name) { print $1 } }' "$1" | sed -n '1p'
}

kv_get() { # <file> <key>   reads KEY=VALUE files without executing them
  sed -n "s/^$2=//p" "$1" | sed -e 's/[[:space:]]*#.*$//' -e 's/[[:space:]]*$//' | sed -n '1p'
}

version_of() { # extracts "1.2.3" from tool output ("v24.21.0", "rustc 1.98.1 (...)"), or nothing
  printf '%s\n' "$1" | grep -o '[0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*' | sed -n '1p'
}

version_cmp() { # <a> <b> -> prints -1, 0 or 1 (numeric, dot-separated)
  awk -v a="$1" -v b="$2" 'BEGIN {
    na = split(a, x, "."); nb = split(b, y, "."); n = (na > nb) ? na : nb
    for (i = 1; i <= n; i++) { xi = x[i] + 0; yi = y[i] + 0
      if (xi < yi) { print -1; exit } if (xi > yi) { print 1; exit } }
    print 0 }'
}

# <installed-output> <locked> <minimum> <role> <strict:0|1> -> prints reuse | install | mismatch
#   dev   : reuse anything >= minimum, otherwise install the locked version
#   build : strict tools must equal the locked version exactly
tool_action() {
  local v; v="$(version_of "$1")"
  if [ -z "$v" ]; then echo install; return; fi
  if [ "$4" = "build" ] && [ "$5" = "1" ]; then
    if [ "$(version_cmp "$v" "$2")" = "0" ]; then echo reuse; else echo mismatch; fi
    return
  fi
  if [ -n "$3" ] && [ "$(version_cmp "$v" "$3")" = "-1" ]; then echo install; else echo reuse; fi
}

tool_version() { # <command> [args...]  prints first line of output, or nothing
  command -v "$1" >/dev/null 2>&1 || return 0
  "$@" 2>/dev/null | sed -n '1p' || true
}
