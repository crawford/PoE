#!/bin/sh

# XXX: Use a vanilla build of OpenOCD at some point
~/openocd/src/openocd \
	--search ~/openocd/tcl \
	--file openocd.cfg \
	--command "program $1 verify" \
	--command "init" \
	--command "reset run" \
	--command "resume" \
	--command "exit"
