	.text
	.global _start
_start:
	movw %r1, #0xBEEF	;@ move 0xdeadbeef
	movt %r1, #0xDEAD
	ldr  %r1, [%r1, #0]	;@ read 0xdeadbeef
	bx   %lr
