#!/usr/bin/env bash
# Double-click to set up a DEVELOPER Mac from this offline bundle.
cd "$(dirname "$0")" || exit 1
bash ./install-from-bundle.sh --role dev "$@"
status=$?
echo
read -r -p "Press Enter to close..." _
exit $status
