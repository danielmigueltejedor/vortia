#!/usr/bin/env bash
# Source a system OpenFOAM environment if present. Safe to `source` from harness scripts.
# Prefer the concrete Ubuntu ESI package (openfoam1912) over the incomplete
# /usr/share/openfoam meta bashrc that often ships without foamEtcFile.
_source_of_bashrc() {
  local rc="$1"
  # shellcheck disable=SC1090
  set +u
  # shellcheck disable=SC1091
  source "$rc"
  set -u
}

if [[ -f /usr/lib/openfoam/openfoam1912/etc/bashrc ]]; then
  _source_of_bashrc /usr/lib/openfoam/openfoam1912/etc/bashrc
elif [[ -f /usr/share/openfoam/etc/bashrc ]]; then
  _source_of_bashrc /usr/share/openfoam/etc/bashrc
fi
unset -f _source_of_bashrc
