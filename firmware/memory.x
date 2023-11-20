MEMORY
{
	FLASH (rx) : ORIGIN = 0x00000000, LENGTH = 2M
	/* FLASH (rx) : ORIGIN = 0x00004000, LENGTH = 2M-16K */
	RAM (rwx)  : ORIGIN = 0x20000000, LENGTH = 512K
}

/* SECTIONS */
/* { */
/*     .text : { *(.text*) } > RAM */
/*     .rodata : { *(.rodata*) } > RAM */
/*     .bss : { *(.bss*) } > RAM */
/* } */
