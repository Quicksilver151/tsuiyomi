#!/usr/bin/env bash
set -euo pipefail

# Prefer the active devshell. Fall back to `nix develop` when invoked from outside it.
if [[ -n "${TSUIYOMI_DEVSHELL:-}" ]] \
    || { [[ -n "${IN_NIX_SHELL:-}" ]] \
        && command -v just >/dev/null 2>&1 \
        && command -v cargo >/dev/null 2>&1; }; then
    exec just run "$@"
fi

if ! command -v nix >/dev/null 2>&1; then
    echo "run.sh: not in the devshell and nix is not installed" >&2
    exit 1
fi

exec nix develop --command just run "$@"
