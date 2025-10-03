HEXDUMP_FLAGS = --no-squeezing --format '/1 "%02x"'
V = @

command: main.bin
	$(V) printf 'prog write 0x%x\n' $(shell cat $^ | wc --bytes)
	$(V) hexdump $(HEXDUMP_FLAGS) $^
	$(V) echo
	$(V) echo 'prog run'

raw-command: main.bin
	$(V) printf 'write $(ADDR) 0x%x\n' $(shell cat $^ | wc --bytes)
	$(V) hexdump $(HEXDUMP_FLAGS) $^
	$(V) echo
	$(V) echo 'call $(shell printf "0x%X\n" $$(($(ADDR) + 1)))'

run: main.bin
	$(V) printf "prog write 0x%x\n%s\nprog run\n" $(shell cat $^ | wc --bytes) $(shell hexdump $(HEXDUMP_FLAGS) $^) | nc -N $(HOST) 23
