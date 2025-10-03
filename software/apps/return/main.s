	.text
	.syntax unified
	.thumb
	.global _start
_start:
	movw r0, #0xE6E8	;@ proc: ReturnValue
	movt r0, #0x1035
	movw r1, #0xEF01	;@ value: 0xABCDEF01
	movt r1, #0xABCD
	svc  0

	bx lr
