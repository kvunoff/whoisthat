#!/bin/sh
# File capabilities are not stored by dpkg/rpm, so re-apply them on every
# install and upgrade. A failure here is a warning: proxy mode still works.
if command -v setcap >/dev/null 2>&1; then
    setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/bin/whoisthat-core 2>/dev/null || {
        echo "warning: could not set capabilities on /usr/bin/whoisthat-core" >&2
        echo "run: sudo setcap cap_net_admin,cap_net_raw,cap_setpcap=+ep /usr/bin/whoisthat-core" >&2
    }
else
    echo "warning: setcap not found; TUN mode needs libcap" >&2
fi
