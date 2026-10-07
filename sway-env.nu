#!/usr/bin/env nu

# Run a command inside the environment of the nested Sway session.
#
# The nested session dumps its own environment to a file on startup (see
# sway-config). This helper re-uses the session-identifying variables from
# that file so that Wayland clients connect to the nested compositor.

# Variables that identify/locate the session.
const KEEP_EXACT = [
  WAYLAND_DISPLAY
  WAYLAND_SOCKET
  XDG_RUNTIME_DIR
  SWAYSOCK
  DISPLAY
  __GLX_VENDOR_LIBRARY_NAME
  GDK_BACKEND
  QT_QPA_PLATFORM
]

# Prefixes of the remaining session variables and of the renderer hints.
const KEEP_PREFIXES = [WLR_ LIBGL_ VK_ EGL MESA_]

# Parsed line-wise, which is safe because `env` output is newline-separated.
def keep [name: string] {
  $name in $KEEP_EXACT or ($KEEP_PREFIXES | any {|prefix| $name | str starts-with $prefix })
}

# nu values into plain strings: `PATH` can be a list, a `null` argument to nothing.
def stringify [value: any] {
  match ($value | describe) {
    'nothing' => ''
    'list<string>' => ($value | str join ':')
    $kind => ($value | into string)
  }
}

# Empty environment variables act as if they were unset (`${VAR:-default}`).
def env-or [name: string, fallback: string] {
  let value = $env | get --optional $name | default '' | str trim
  if ($value | is-empty) { $fallback } else { $value }
}

# `--` stops nu from parsing the arguments as its own flags, but they are still
# parsed as nu values, hence the `any` and the stringification of every argument.
def main [...args: any] {
  let runtime_dir = env-or 'XDG_RUNTIME_DIR' '/tmp'
  let env_file = env-or 'WGPAPER_SWAY_ENV_FILE' ($runtime_dir | path join 'wgpaper-sway.env')

  if ($args | is-empty) {
    print --stderr 'usage: sway-exec <command> [args...]'
    exit 2
  }

  let env_dump = try { open --raw $env_file } catch { null }

  if ($env_dump | describe) == 'nothing' {
    print --stderr $"sway-exec: no readable env file at ($env_file)"
    print --stderr 'start the nested session first (mise rds / mise rsds)'
    exit 1
  }

  # `env` output lines are already `KEY=VALUE`, so they are passed on as-is.
  let session_env = (
    $env_dump
    | lines
    | each {|line| {line: $line, key: ($line | split row '=' | first)}}
    | where {|entry| keep $entry.key}
    | get line
  )

  let base_env = [
    $"PATH=(stringify (env-or 'PATH' '/usr/bin'))"
    $"HOME=(stringify (env-or 'HOME' '/'))"
    $"TERM=(stringify (env-or 'TERM' 'xterm-256color'))"
  ]

  let command = $args | each {|arg| stringify $arg}

  # `^env` replaces this process, so the exit status is the command's own.
  ^env --ignore-environment ...$base_env ...$session_env ...$command
}