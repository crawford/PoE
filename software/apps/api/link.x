ENTRY(_start)

MEMORY {
  MEM (rwx) : ORIGIN = 0x20000000, LENGTH = 512k
}

SECTIONS
{
  .text : ALIGN(4) {
    KEEP(*(.text._start))
    *(.text .text.*)
    *(.rodata .rodata.*)
  } > MEM

  .data : ALIGN(4) {
    *(.data .data.*)
  } > MEM

  .bss (NOLOAD) : ALIGN(4) {
    *(.bss .bss.*)
    *(COMMON)
  } > MEM
}
