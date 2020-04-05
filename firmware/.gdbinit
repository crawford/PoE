target extended-remote :3333
set remotetimeout 1500
file target/thumbv7m-none-eabi/release/poe
monitor reset halt
