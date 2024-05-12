#!/usr/bin/env nix-shell
#!nix-shell -i sh -p openocd

set -e

openocd \
    --file openocd.cfg \
    --command 'reset halt' \
    --command "program ${1} verify reset exit"

openocd \
    --file openocd.cfg \
    --command 'rtt setup 0x20000000 0x4000 "SEGGER RTT"' \
    --command 'rtt start' \
    --command 'rtt server start 9090 0' \
    --command 'rtt server start 9091 1' &

sleep 1

nc localhost 9091
