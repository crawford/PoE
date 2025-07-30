	.text
	.syntax unified
	.thumb
	.global _start
_start:
	movw r0, #0xBF5A	;@ proc: RegisterHandler
	movt r0, #0xD35D
	movw r1, #0xABCD	;@ event id: 0xABCD
	ldr  r2, =handler	;@ func: handler
	orr  r2, r2, #1
	svc  0

	movw r0, #0x8A43	;@ proc: TriggerEvent
	movt r0, #0x6543
	movw r1, #0xABCD	;@ event id: 0xABCD
	svc  0

	bx lr

	bkpt
handler:
	bx lr
