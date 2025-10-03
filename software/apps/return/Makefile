include ../flash.mk

image: main.bin

clean:
	$(V) rm -f main.elf main.bin

main.bin: main.elf
	$(V) arm-none-eabi-objcopy -O binary $< $@

main.elf: main.s
	$(V) arm-none-eabi-gcc -march=armv7-m -Wl,-Ttext $(ADDR) -Xassembler -mthumb -nostdlib -o $@ $^
