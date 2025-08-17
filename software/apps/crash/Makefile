command: main.bin
	@printf 'prog write 0x%x\n' $(shell cat $^ | wc -c)
	@hexdump -e '/1 "%02x"' $^
	@echo
	@echo 'prog run'

raw-command: main.bin
	@printf 'write $(START_ADDR) 0x%x\n' $(shell cat $^ | wc -c)
	@hexdump -e '/1 "%02x"' $^
	@echo
	@echo 'call $(shell printf "0x%X\n" $$(($(START_ADDR) + 1)))'

run: main.bin
	printf "prog write 0x%x\n%s\nprog run\n" $(shell cat $^ | wc -c) $(shell hexdump -e '/1 "%02x"' $^) | nc -N $(HOST) 23

image: main.bin

clean:
	rm -f main.elf main.bin

main.bin: main.elf
	arm-none-eabi-objcopy -O binary $< $@

main.elf: main.s
	arm-none-eabi-gcc -march=armv7-m -Wl,-Ttext $(START_ADDR) -Xassembler -mthumb -nostdlib -o $@ $^
