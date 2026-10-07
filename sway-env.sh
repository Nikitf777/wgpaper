#!/usr/bin/env bash
# Run a command inside the environment of the nested Sway session.
#
# The nested session dumps its own environment to a file on startup (see
# sway-config.sh). This helper re-uses the session-identifying variables from
# that file so that Wayland clients connect to the nested compositor.
set -euo pipefail

env_file="${WGPAPER_SWAY_ENV_FILE:-${XDG_RUNTIME_DIR:-/tmp}/wgpaper-sway.env}"

if [[ ! -r $env_file ]]; then
  echo "sway-exec: no env file at $env_file" >&2
  echo "start the nested session first (mise rds / mise rsds)" >&2
  exit 1
fi

if [[ $# -eq 0 ]]; then
  echo "usage: sway-exec <command> [args...]" >&2
  exit 2
fi

# Keep the variables that identify/locate the session, plus renderer hints.
# Parsed line-wise, which is safe because `env` output is newline-separated.
keep='^(WAYLAND_DISPLAY|WAYLAND_SOCKET|XDG_RUNTIME_DIR|SWAYSOCK|DISPLAY|WLR_[A-Z0-9_]+|LIBGL_[A-Z0-9_]+|VK_[A-Z0-9_]+|EGL[A-Z0-9_]*|MESA_[A-Z0-9_]+|__GLX_VENDOR_LIBRARY_NAME|WLR_RENDERER|GDK_BACKEND|QT_QPA_PLATFORM)='

session_env=()
while IFS= read -r line; do session_env+=("$line"); done < <(grep -E "$keep" "$env_file")

exec env -i \
  PATH="$PATH" \
  HOME="$HOME" \
  TERM="${TERM:-xterm-256color}" \
  "${session_env[@]}" \
  "$@"
