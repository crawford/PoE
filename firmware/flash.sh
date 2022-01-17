#!/usr/bin/env nix-shell
#!nix-shell -i sh -p openocd

post_program="exit"

if [[ "${1}" == "--remain" ]]
then
    shift
    post_program=""
fi

openocd \
	--file openocd.cfg \
	--command "program ${1} verify reset ${post_program}"
