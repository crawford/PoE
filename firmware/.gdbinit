target extended-remote | \
	openocd --command "log_output openocd.log; gdb_port pipe" --file openocd.cfg

file target/thumbv7m-none-eabi/debug/poe
monitor reset halt
