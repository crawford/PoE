	.text
	.global _start
_start:
	movw %r0, #0xBF5A	;@ proc: RegisterHandler
	movt %r0, #0xD35D
	movw %r1, #0xABCD	;@ event id: 0xABCD
	ldr  %r2, =_start	;@ func: <self>
	svc 0
	bx lr
