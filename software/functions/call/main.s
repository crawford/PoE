	.text
	.syntax unified
	.thumb
	.global _start
_start:
	push {r4, lr} ;@ push r4 to maintain stack alignment

register:
	movw r0, #0xBF5A    ;@ proc: RegisterHandler
	movt r0, #0xD35D
	movw r1, #0xABCD    ;@ event id: 0xABCD
	ldr  r2, =handler   ;@ func: handler
	orr  r2, r2, #1
	svc  0
trigger:
	movw r0, #0x8A43    ;@ proc: TriggerEvent
	movt r0, #0x6543
	movw r1, #0xABCD    ;@ event id: 0xABCD
	svc  0
return:
	pop {r4, lr}
	bx  lr

handler:
	push {r4, lr} ;@ push r4 to maintain stack alignment

	movw r0, #0x6986    ;@ proc: PrintString
	movt r0, #0x0A06
	ldr  r1, =message   ;@ str: message
	svc  0

	pop {r4, lr}
	bx lr

	.section .rodata
message:
	.asciz "Hello, world!"
