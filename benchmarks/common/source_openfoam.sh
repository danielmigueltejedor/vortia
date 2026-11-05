#!/usr/bin/env bash
# Source a system OpenFOAM environment if present. Safe to `source` from harness scripts.
if [[ -f /usr/share/openfoam/etc/bashrc ]]; then
  # shellcheck disable=SC1091
  set +u
  source /usr/share/openfoam/etc/bashrc
  set -u
elif [[ -f /usr/lib/openfoam/openfoam1912/etc/bashrc ]]; then
  # shellcheck disable=SC1091
  set +u
  source /usr/lib/openfoam/openfoam1912/etc/bashrc
  set -u
fi
