#!/usr/bin/env bash
# Fail unless every given binary is statically linked (musl builds must have
# no INTERP segment).
#
# Usage: check-static.sh <binary>...
set -euo pipefail

if [ "$#" -eq 0 ]; then
    echo 'usage: check-static.sh <binary>...' >&2
    exit 2
fi

for bin in "$@"; do
    if readelf -l "$bin" | grep -q INTERP; then
        echo "$bin is dynamically linked; musl build failed" >&2
        exit 1
    fi
done
