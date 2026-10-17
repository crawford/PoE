file target/thumbv7m-none-eabi/debug/slstk3701a

# cargo-embed throws the following error when non-stop is enabled:
#     During the execution of GDB an error was encountered:
#     failed to fill the whole buffer
set non-stop off
target extended-remote :1337

monitor reset halt
monitor rtt setup 0x20000000 16384 "SEGGER RTT"
monitor rtt start
monitor rtt server start 9090 0
monitor rtt server start 9091 1
