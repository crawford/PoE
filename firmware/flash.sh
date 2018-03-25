#!/bin/sh

# XXX: Use a vanilla build of OpenOCD at some point
~/code/openocd/src/openocd \
	--search ~/code/openocd/tcl \
	--file openocd.cfg \
	--command "program $1 verify" \
	--command "init" \
	--command "reset run" \
	--command "resume" \
	--command "exit"
