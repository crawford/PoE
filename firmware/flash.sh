#!/usr/bin/env nix-shell
#!nix-shell -i sh -p openocd

openocd \
	--file openocd.cfg \
	--command "program $1 verify reset exit"
