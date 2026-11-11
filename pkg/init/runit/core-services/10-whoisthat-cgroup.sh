#!/bin/sh
# Runit stage-1 core service: prepare cgroups and clear session markers before
# runsvdir starts the user services.
exec /usr/lib/whoisthat/init-hook
