HEXDUMP_FLAGS := --no-squeezing --format '/1 "%02x"'

command: main.bin
	@printf 'prog write 0x%x\n' $(shell cat $^ | wc --bytes)
	@hexdump $(HEXDUMP_FLAGS) $^
	@echo
	@echo 'prog run'

raw-command: main.bin
	@printf 'write $(ADDR) 0x%x\n' $(shell cat $^ | wc --bytes)
	@hexdump $(HEXDUMP_FLAGS) $^
	@echo
	@echo 'call $(shell printf "0x%X\n" $$(($(ADDR) + 1)))'

run: main.bin
	printf "prog write 0x%x\n%s\nprog run\n" $(shell cat $^ | wc --bytes) $(shell hexdump $(HEXDUMP_FLAGS) $^) | nc -N $(HOST) 23

image: main.bin

clean:
	rm -f main.elf main.bin

main.bin: main.elf
	arm-none-eabi-objcopy -O binary $< $@

main.elf: main.s
	arm-none-eabi-gcc -march=armv7-m -Wl,-Ttext $(ADDR) -Xassembler -mthumb -nostdlib -o $@ $^
