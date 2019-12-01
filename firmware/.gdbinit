target remote :3333
set remotetimeout 1500
file target/thumbv7m-none-eabi/debug/poe
monitor reset halt
