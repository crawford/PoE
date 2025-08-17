file target/thumbv7m-none-eabi/debug/slstk3701a

target extended-remote | \
	openocd --command "log_output openocd.log; gdb_port pipe" --file supervisor/openocd.cfg

monitor reset halt
