#!/usr/bin/env bash
# Install musl-tools on the Ubuntu runners.
#
# The runners occasionally cannot resolve the Ubuntu mirrors: ForceIPv4 and
# retries help with IPv6-broken DNS, and the pinned addresses cover the case
# where resolution fails entirely.
set -euo pipefail

printf 'Acquire::ForceIPv4 "true";\nAcquire::Retries "5";\n' | sudo tee /etc/apt/apt.conf.d/99whoisthat-net >/dev/null
sudo sed -i 's/azure\.archive\.ubuntu\.com/archive.ubuntu.com/g' /etc/apt/apt-mirrors.txt /etc/apt/sources.list /etc/apt/sources.list.d/* 2>/dev/null || true
grep -q ' ports.ubuntu.com' /etc/hosts || sudo sh -c 'printf "91.189.91.102 ports.ubuntu.com\n185.125.190.81 archive.ubuntu.com\n185.125.190.81 security.ubuntu.com\n" >> /etc/hosts'
sudo apt-get update
sudo apt-get install -y musl-tools
