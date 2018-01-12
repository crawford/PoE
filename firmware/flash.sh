#!/bin/sh

# XXX: Use a vanilla build of OpenOCD at some point
~/code/crawford/openocd/src/openocd \
	--search ~/code/crawford/openocd/tcl \
	--file openocd.cfg \
	--command "program $1 verify" \
	--command "init" \
	--command "reset run" \
	--command "resume" \
	--command "exit"
