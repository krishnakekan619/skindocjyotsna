#!/usr/bin/env bash
# Double-click to set up a PRODUCTION BUILD Mac from this offline bundle.
# Node.js and Rust must match tools.lock exactly, so release installers are reproducible.
cd "$(dirname "$0")" || exit 1
bash ./install-from-bundle.sh --role build "$@"
status=$?
echo
read -r -p "Press Enter to close..." _
exit $status
