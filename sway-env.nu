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

# The name of an env dump line, or nothing when the line is not `NAME=VALUE`.
def line-key [line: string] {
  let sep = $line | str index-of '='
  if $sep == null { null } else { $line | str substring 0..<$sep }
}

# Kept lines as `NAME=VALUE` pairs, i.e. ready to be handed to `env`.
def keep-lines [dump: string] {
  $dump
  | lines
  | where {|line| keep (line-key $line | default '')}
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
  let session_env = keep-lines $env_dump

  # The env dump outlives the compositor, so check the socket it points to.
  let session = (
    $session_env
    | reduce --fold {} {|line, acc|
        let sep = $line | str index-of '='
        $acc | upsert ($line | str substring 0..<$sep) ($line | str substring ($sep + 1)..)
      }
  )

  let socket = (
    $session | get --optional SWAYSOCK
    | default ($session | get --optional WAYLAND_SOCKET)
    | default (do {
      let display = $session | get --optional WAYLAND_DISPLAY
      let runtime = $session | get --optional XDG_RUNTIME_DIR
      if $display != null and $runtime != null { $runtime | path join $display } else { null }
    })
  )

  let alive = if $socket == null { true } else { $socket | path exists }

  if $alive == false {
    # No parentheses in interpolated strings: `(` opens an interpolation.
    print --stderr $"sway-exec: the session in ($env_file) is gone, no socket at ($socket)"
    print --stderr 'start a new one (mise rsds) or clean up (mise clean)'
    exit 1
  }

  let base_env = [
    $"PATH=(stringify (env-or 'PATH' '/usr/bin'))"
    $"HOME=(stringify (env-or 'HOME' '/'))"
    $"TERM=(stringify (env-or 'TERM' 'xterm-256color'))"
  ]

  let command = $args | each {|arg| stringify $arg}

  # `^env` replaces this process, so the exit status is the command's own.
  ^env --ignore-environment ...$base_env ...$session_env ...$command
}