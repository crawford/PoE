#!/usr/bin/env nix-shell
#!nix-shell -i sh -p openocd

set -e

cd -- "$(dirname -- "$0")"

(
    trap 'kill 0' EXIT

    openocd \
        --file openocd.cfg \
        --command 'reset halt' \
        --command "program ${1} verify reset" &

    sleep 1

    nc localhost 9091
)
